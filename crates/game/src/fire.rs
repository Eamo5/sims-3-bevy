//! Fire: a stove left to a poor cook catches fire and spreads to what's around it. Sims nearby
//! panic (and those too close catch fire themselves: they may stop, drop and roll, or die), a
//! smoke alarm calls the fire department (or someone phones), and a firefighter comes in the fire
//! truck, in uniform, to put each fire out with the extinguisher.

use bevy::prelude::*;
use rand::Rng;

use crate::anim::ActionClip;
use crate::camera::SimsCamera;
use crate::clock::GameClock;
use crate::interact::{ActionQueue, GameObject, Notifications};
use crate::sim::HouseholdMember;
use crate::nav::Floor;
use crate::sim::{Age, Sim};
use crate::{AppState, PlayMode};

pub struct FirePlugin;

impl Plugin for FirePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FireDepartment>()
            .add_message::<StartFire>()
            .add_systems(Update, (start_fires, spread, panic, burning, call_firefighters, firefighting, flicker).chain().run_if(in_state(PlayMode::Live)))
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
}

/// A Sim panicking at a fire.
#[derive(Component)]
pub struct Panicking;

/// The firefighter come to put the fires out.
#[derive(Component)]
pub struct Firefighter {
    /// The fire being put out, and since when.
    target: Option<(Entity, f64)>,
    leaving: bool,
}

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

/// Whether a cook's dinner goes up in flames: likelier for the unskilled (and the clumsy).
pub fn cooking_fire(cooking_level: u32, clumsy: bool) -> bool {
    let p = (0.07 - cooking_level as f64 * 0.007).max(0.004) * if clumsy { 2.0 } else { 1.0 };
    rand::rng().random_bool(p)
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
#[derive(Default)]
struct FireLooks {
    quad: Option<Handle<Mesh>>,
    mat: Option<Handle<StandardMaterial>>,
}

#[allow(clippy::too_many_arguments)]
fn start_fires(
    mut commands: Commands,
    mut starts: MessageReader<StartFire>,
    clock: Res<GameClock>,
    fires: Query<&Transform, With<Fire>>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut looks: Local<FireLooks>,
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
        let quad = looks.quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
        let mat = looks
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
    objects: Query<(Entity, &GlobalTransform), (With<GameObject>, Without<crate::interact::Broken>, Without<crate::visit::LotObject>)>,
    mut starts: MessageWriter<StartFire>,
) {
    let count = fires.iter().count();
    let mut rng = rand::rng();
    for (tf, mut f) in &mut fires {
        // What's in the flames is ruined.
        for (e, otf) in &objects {
            if otf.translation().xz().distance(tf.translation.xz()) < 0.9 && (otf.translation().y - tf.translation.y).abs() < 1.5 {
                commands.entity(e).insert(crate::interact::Broken);
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
        (Entity, &Transform, &mut ActionQueue, &Sim, Has<Panicking>, Has<OnFire>),
        (Without<Firefighter>, Without<crate::death::Dying>, Without<crate::interact::AtWork>, Without<crate::rabbitholes::AtRabbitHole>),
    >,
) {
    for (e, tf, mut q, sim, panicking, burning) in &mut sims {
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
        if !burning && nearest < CATCH_RANGE {
            q.0.clear();
            commands
                .entity(e)
                .remove::<(Panicking, crate::nav::PathFollow)>()
                .insert((OnFire { since: clock.minutes, next: clock.minutes + 5.0 }, ActionClip::new(Some("a_fire_onFire_panic_start_x"), &["a_fire_onFire_panic_loop1_x", "a_fire_onFire_panic_loop2_x"])));
            continue;
        }
        if burning {
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

/// A burning Sim may stop, drop and roll; burning too long is the end of them.
fn burning(mut commands: Commands, clock: Res<GameClock>, mut q: Query<(Entity, &Sim, &mut OnFire, &mut crate::sim::Motives)>, mut notes: ResMut<Notifications>) {
    let mut rng = rand::rng();
    for (e, sim, mut f, mut m) in &mut q {
        m.add(crate::sim::HYGIENE, -0.5);
        if clock.minutes < f.next {
            continue;
        }
        f.next = clock.minutes + 5.0;
        if rng.random_bool(0.3) {
            commands.entity(e).remove::<OnFire>().insert(ActionClip::new(Some("a_fire_onFire_putOut_start_x"), &["a_fire_onFire_putOut_stop_x"]));
            notes.push(format!("{} stopped, dropped and rolled, and put the flames out!", sim.first));
        } else if clock.minutes - f.since > 30.0 {
            commands.entity(e).remove::<(OnFire, Panicking)>().insert(crate::death::Dying::in_fire());
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
    (exit, grid, upper): (Option<Res<crate::interact::LotExit>>, Option<Res<crate::nav::NavGrid>>, Option<Res<crate::nav::UpperFloors>>),
    world: Res<crate::loading::CurrentWorld>,
    cas: Option<Res<crate::simbody::CasData>>,
    mut rides: ResMut<crate::traffic::PendingRides>,
    mut notes: ResMut<Notifications>,
) {
    let Some(called) = dept.called else { return };
    let (Some(exit), Some(grid)) = (exit, grid) else { return };
    let mut rng = rand::rng();
    // The truck arrives with the firefighter.
    if dept.firefighter.is_none() {
        if fires.is_empty() {
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
                Firefighter { target: None, leaving: false },
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
    if f.leaving {
        if tf.translation.xz().distance(exit.0) < 2.5 || path.is_some() {
            commands.entity(me).despawn();
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
                commands.entity(target).despawn();
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
