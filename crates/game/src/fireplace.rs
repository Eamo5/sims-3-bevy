//! Fireplaces: Sims light a fire (it burns a few hours, flames dancing in the hearth and a warm
//! flickering light on the room), warm their hands by it, and put it out. Now and then a spark
//! catches the floor in front. Candles likewise: one small flame, a soft light, no sparks.
//! A Daredevil teen or older will "Play with Fire" while it's lit (the game's `PlayWithFire`:
//! fun 50 an hour), and past its first five minutes, every two a chance in ten (the Clumsy's a
//! quarter more: `ChanceBurnSelfWhenPlaying`, `kClumsyTraitChanceBurnSelfOnFireplace`) of being
//! burned: fun 15 less and the burnt ending. Upgraded to "Change Fire Color" (Handiness 9), a lit
//! fire can be turned Blue, Green or Purple and back to Normal ("Change Fire Color ›": the game's
//! `ChangeFireColor`), its flames and light in the colour (the tuning's
//! `...FlameLightChannel`s), kept for the next fire and in saves.

use bevy::prelude::*;
use rand::Rng;

use crate::clock::GameClock;
use crate::interact::{ActionKind, ActionQueue, GameObject, Phase, Special};
use crate::nav::Floor;
use crate::{AppState, PlayMode};

pub struct FireplacePlugin;

impl Plugin for FireplacePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FireplaceRequest>()
            .add_systems(Update, (requests, burn, glow, cozy).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (play_with_fire, fire_test).chain().after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// A fire's colour (the game's `FireColor`).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FireColor {
    #[default]
    Normal,
    Blue,
    Green,
    Purple,
}

impl FireColor {
    pub const ALL: [FireColor; 4] = [FireColor::Normal, FireColor::Blue, FireColor::Green, FireColor::Purple];

    /// The game's word for it (`ChangeFireColor:Normal`...), its entry in the menu.
    pub fn name(self) -> &'static str {
        match self {
            FireColor::Normal => "Normal",
            FireColor::Blue => "Blue",
            FireColor::Green => "Green",
            FireColor::Purple => "Purple",
        }
    }

    pub fn of(name: &str) -> Option<FireColor> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// The light it casts: a normal fire's warm orange, else the tuning's channels
    /// (`BlueFlameLightChannelB` 0.5...), at full strength.
    pub fn light(self) -> Color {
        let [r, g, b] = match self {
            FireColor::Normal => return Color::srgb(1.0, 0.58, 0.25),
            FireColor::Blue => [0.0, 0.0, 0.5],
            FireColor::Green => [0.0, 0.5, 0.0],
            FireColor::Purple => [0.5, 0.0, 0.5],
        };
        let most = f32::max(r, f32::max(g, b));
        Color::linear_rgb(r / most, g / most, b / most)
    }

    /// Its flames' colours, at their tips and at their hot roots (the game's `...BurningBlue`
    /// effects).
    fn flames(self) -> ([f32; 3], [f32; 3]) {
        match self {
            FireColor::Normal => ([1.0, 0.25, 0.05], [1.0, 1.0, 0.65]),
            FireColor::Blue => ([0.1, 0.3, 1.0], [0.8, 0.95, 1.0]),
            FireColor::Green => ([0.15, 1.0, 0.2], [0.85, 1.0, 0.75]),
            FireColor::Purple => ([0.65, 0.15, 1.0], [1.0, 0.8, 1.0]),
        }
    }
}

/// The game's words: the menu (`ChangeFireColor:InteractionName`), and playing with it.
pub const CHANGE_COLOR: &str = "Change Fire Color";
pub const PLAY_WITH_FIRE: &str = "Play with Fire";

/// Whether one's offered (the game's tests): playing with fire a Daredevil's, teens and up; the
/// colours once it's upgraded, any but the one it is, children and up.
pub fn offers(special: Special, sim: &crate::sim::Sim, upgrades: Option<&crate::upgrades::Upgrades>, color: Option<&FireColor>, name: &str) -> bool {
    let teen_up = sim.age == crate::sim::Age::Teen || sim.age.is_grown();
    match special {
        Special::PlayWithFire => teen_up && sim.traits.contains(&crate::life::Trait::Daredevil),
        Special::ChangeFireColor => {
            !sim.age.is_little() && crate::upgrades::has(upgrades, crate::upgrades::Upgrade::ChangeColor) && FireColor::of(name).is_some_and(|c| c != color.copied().unwrap_or_default())
        }
        _ => true,
    }
}

/// The tuning for playing with fire: how long before a burn can come, how often it's chanced,
/// the chance (per cent; the Clumsy's more), and the fun lost to it.
const MIN_BEFORE_BURN: f64 = 5.0;
const BURN_TEST_MINUTES: f64 = 2.0;
const BURN_CHANCE: f64 = 10.0;
const CLUMSY_BURN_CHANCE: f64 = 25.0;
pub const BURN_FUN: f32 = -15.0;
/// The burnt ending (`ExitBurned`) and how long it is.
pub const BURNED_END: &[&str] = &["a2o_fireplace_playFire_stopFail_x"];
const BURNED_SECS: f32 = 6.9;

/// The chance in a test of being burned.
pub fn burn_chance(clumsy: bool) -> f64 {
    (BURN_CHANCE + if clumsy { CLUMSY_BURN_CHANCE } else { 0.0 }) / 100.0
}

/// At a fire, playing with it: when the next chance of a burn is.
#[derive(Component)]
struct Playing {
    next: f64,
}

/// Playing with fire: the chances of a burn, and burned, the fun lost and the burnt ending.
#[allow(clippy::type_complexity)]
fn play_with_fire(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut sims: Query<(Entity, &crate::sim::Sim, &mut ActionQueue, &mut crate::sim::Motives, Option<&mut Playing>)>,
    objects: Query<&GameObject>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, sim, mut q, mut motives, playing) in &mut sims {
        let at_it = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel && a.ending.is_none()).is_some_and(|a| match a.kind {
            ActionKind::Object { target, def } => objects.get(target).ok().and_then(|o| crate::interact::interactions_for(o.kind).get(def)).is_some_and(|d| d.special == Special::PlayWithFire),
            _ => false,
        });
        let mut p = match (playing, at_it) {
            (Some(p), true) => p,
            (None, true) => {
                commands.entity(e).insert(Playing { next: now + MIN_BEFORE_BURN });
                continue;
            }
            (Some(_), false) => {
                commands.entity(e).remove::<Playing>();
                continue;
            }
            (None, false) => continue,
        };
        let chance = burn_chance(sim.traits.contains(&crate::life::Trait::Clumsy));
        let mut burned = false;
        while now >= p.next {
            p.next += BURN_TEST_MINUTES;
            burned |= rng.random_bool(chance) || std::env::var_os("FIRE_BURN").is_some();
        }
        if burned {
            motives.add(crate::sim::FUN, BURN_FUN);
            if let Some(a) = q.0.front_mut() {
                a.ending = Some((BURNED_END, BURNED_SECS));
                a.phase = Phase::Running(1.0e9);
            }
            commands.entity(e).remove::<Playing>();
            info!("fireplace: {} got burned playing with fire", sim.first);
        }
    }
}

/// `FIRE_TEST=play|color:<Blue|Green|Purple|Normal>` (tests): the lot's fireplace lit, ten
/// seconds in, and the selected Sim (a Daredevil for it) playing with it; or it upgraded to
/// change colour and the colour changed. `FIRE_BURN=1`: burned at the first chance.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn fire_test(
    time: Res<Time>,
    mut sims: Query<(&mut crate::sim::Sim, &mut ActionQueue, &mut crate::interact::Skills), With<crate::sim::Selected>>,
    fireplaces: Query<(Entity, &GameObject, Has<Lit>, &GlobalTransform)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut commands: Commands,
    mut requests: MessageWriter<FireplaceRequest>,
    mut stage: Local<u8>,
) {
    let Ok(what) = std::env::var("FIRE_TEST") else { return };
    let t = time.elapsed_secs();
    let Some((fp, _, lit, at)) = fireplaces.iter().find(|(_, o, ..)| o.kind == crate::interact::ObjectKind::Fireplace) else { return };
    let Ok((mut sim, mut q, mut skills)) = sims.single_mut() else { return };
    // (The view on the fireplace's front, `FIRE_YAW` round from it, `FIRE_DIST` off.)
    if t > 7.0
        && let Ok(mut cam) = cams.single_mut()
    {
        let front = at.rotation() * Vec3::Z;
        cam.look_at(at.translation() + Vec3::Y * 0.5);
        cam.distance = std::env::var("FIRE_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(6.0);
        cam.yaw = front.x.atan2(front.z) + std::env::var("FIRE_YAW").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.5);
    }
    match *stage {
        0 if t > 8.0 => {
            requests.write(FireplaceRequest::Light(fp));
            if what.starts_with("color") || what == "menu" {
                commands.entity(fp).insert(crate::upgrades::Upgrades(crate::upgrades::Upgrade::ChangeColor.bit()));
            }
            *stage = 1;
        }
        1 if t > 10.0 && lit => {
            *stage = 2;
            // (FIRE_TEST=menu: a Daredevil's menu at the upgraded fire; =upgrade: a handy one's
            // at the fire not yet upgraded.)
            if what == "menu" || what == "upgrade" {
                if !sim.traits.contains(&crate::life::Trait::Daredevil) {
                    sim.traits.push(crate::life::Trait::Daredevil);
                }
                if what == "upgrade" {
                    skills.0.insert("Handiness", 9.0);
                }
                commands.insert_resource(crate::hud::PieFor(fp));
                return;
            }
            let (special, name) = match what.split_once(':') {
                Some((_, c)) => (Special::ChangeFireColor, c.to_string()),
                None => (Special::PlayWithFire, PLAY_WITH_FIRE.to_string()),
            };
            if special == Special::PlayWithFire && !sim.traits.contains(&crate::life::Trait::Daredevil) {
                sim.traits.push(crate::life::Trait::Daredevil);
            }
            let Some(def) = crate::interact::interactions_for(crate::interact::ObjectKind::Fireplace).iter().position(|d| d.special == special && d.name == name) else { return };
            q.push_player(crate::interact::Action::new(name.clone(), ActionKind::Object { target: fp, def }, false));
            info!("fire test: {name}");
        }
        _ => {}
    }
}

/// A fire in the hearth, burning until then.
#[derive(Component)]
pub struct Lit {
    until: f64,
    flames: Entity,
    next_spark: f64,
}

/// Light or put out a fireplace's fire, or turn it a colour.
#[derive(Message)]
pub enum FireplaceRequest {
    Light(Entity),
    PutOut(Entity),
    Recolor(Entity, FireColor),
}

/// A fire poked up to raging (the game's `PokeFireSuccess`), till then.
#[derive(Component)]
pub struct Raging(pub f64);

/// The game's `PokeFireTime`: how long a poked fire rages (game minutes).
pub const RAGING_MINUTES: f64 = 20.0;
/// The game's `CozyFireBuffNormal`, `CozyFireBuffRaging` and `CozyFireBuffRange` (metres).
const COZY: i32 = 10;
const COZY_RAGING: i32 = 20;
const COZY_RANGE: f32 = 10.0;
/// How long Cozy lasts, kept up while near (the Flame Fruit's leaves it at an hour to go).
const COZY_MINUTES: f64 = 120.0;

/// How long a fire burns (game minutes), and a candle.
const BURN_MINUTES: f64 = 300.0;
const CANDLE_MINUTES: f64 = 240.0;

/// The hearth's flames and light.
#[derive(Component)]
struct HearthLight;

/// A candle's light.
#[derive(Component)]
struct CandleLight;

#[derive(Default)]
struct Looks {
    quad: Option<Handle<Mesh>>,
    /// The flames' material, a colour's each.
    mats: [Option<Handle<StandardMaterial>>; 4],
}

impl Looks {
    fn material(&mut self, color: FireColor, mats: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) -> Handle<StandardMaterial> {
        self.mats[color as usize]
            .get_or_insert_with(|| {
                let (image, tint) = match color {
                    FireColor::Normal => (crate::fire::flame_image(), LinearRgba::new(2.4, 1.6, 1.0, 1.0)),
                    c => {
                        let (tip, root) = c.flames();
                        (crate::fire::flame_image_tinted(tip, root), LinearRgba::new(2.0, 2.0, 2.0, 1.0))
                    }
                };
                mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(tint),
                    base_color_texture: Some(images.add(image)),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .clone()
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn requests(
    mut commands: Commands,
    mut reqs: MessageReader<FireplaceRequest>,
    clock: Res<GameClock>,
    places: Query<(&GameObject, Option<&Lit>, Option<&FireColor>)>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut looks: Local<Looks>,
    ui: Option<Res<crate::icons::GameUi>>,
    (children, mut flames, mut hearths): (Query<&Children>, Query<&mut MeshMaterial3d<StandardMaterial>, With<crate::fire::Flame>>, Query<&mut PointLight, With<HearthLight>>),
) {
    for r in reqs.read() {
        match *r {
            // (A colour thrown in: the flames and the light turned it, and kept for the next.)
            FireplaceRequest::Recolor(e, color) => {
                let Ok((o, lit, _)) = places.get(e) else { continue };
                commands.entity(e).insert(color);
                let mat = looks.material(color, &mut mats, &mut images);
                for f in lit.and_then(|l| children.get(l.flames).ok()).into_iter().flatten() {
                    if let Ok(mut m) = flames.get_mut(*f) {
                        m.0 = mat.clone();
                    }
                    if let Ok(mut l) = hearths.get_mut(*f) {
                        l.color = color.light();
                    }
                }
                info!("fireplace: the {}'s fire is {}", o.name, color.name());
            }
            FireplaceRequest::Light(e) => {
                let Ok((o, lit, color)) = places.get(e) else { continue };
                if lit.is_some() {
                    continue;
                }
                let quad = looks.quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
                let color = color.copied().unwrap_or_default();
                let mat = looks.material(color, &mut mats, &mut images);
                // The hearth: the fireplace's effect slot, else low down in the middle of the
                // firebox, a little in from the front. (A candle's wick: its slot, else its top.)
                let candle = o.kind == crate::interact::ObjectKind::Candle;
                // (A fire pit's fire in its bowl.)
                let pit = o.kind == crate::interact::ObjectKind::FirePit;
                let slot = ui.as_ref().and_then(|ui| ui.data.fx_slots.iter().find(|(k, _)| *k == o.objd)).and_then(|(_, s)| s.first().copied());
                let at = match (candle, slot) {
                    (true, Some(s)) => Vec3::from(s),
                    (true, None) => Vec3::new(o.center.x, o.height + 0.02, o.center.y),
                    // (Its effect slot can be down at its foot, under the logs: up on them then.)
                    (false, s) if pit => s.map(Vec3::from).filter(|s| s.y > 0.15).unwrap_or(Vec3::new(o.center.x, o.height * 0.55, o.center.y)),
                    (false, s) => s.map_or(Vec3::new(o.center.x, 0.08, o.center.y + o.half.y * 0.15), |s| Vec3::from(s) - Vec3::Y * 0.12),
                };
                let mut rng = rand::rng();
                if candle {
                    let flames = commands
                        .spawn((Transform::from_translation(at), Visibility::default(), DespawnOnExit(AppState::InGame), ChildOf(e)))
                        .with_children(|f| {
                            let size = Vec2::new(0.035, 0.07);
                            let offset = Vec3::Y * 0.03;
                            f.spawn((Mesh3d(quad.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(offset), crate::fire::Flame::new(size, rng.random_range(0.0..6.0), offset)));
                            f.spawn((
                                PointLight { color: Color::srgb(1.0, 0.65, 0.32), intensity: 2_500.0, range: 3.5, shadow_maps_enabled: false, ..default() },
                                Transform::from_xyz(0.0, 0.1, 0.0),
                                CandleLight,
                            ));
                        })
                        .id();
                    // (No sparks from a candle.)
                    commands.entity(e).insert(Lit { until: clock.minutes + CANDLE_MINUTES, flames, next_spark: f64::MAX });
                    continue;
                }
                let flames = commands
                    .spawn((Transform::from_translation(at), Visibility::default(), DespawnOnExit(AppState::InGame), ChildOf(e)))
                    .with_children(|f| {
                        for k in 0..5 {
                            let size = Vec2::new(rng.random_range(0.16..0.26), rng.random_range(0.28..0.5));
                            let offset = Vec3::new(rng.random_range(-0.22..0.22), 0.0, rng.random_range(-0.05..0.05));
                            f.spawn((Mesh3d(quad.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(offset), crate::fire::Flame::new(size, k as f32 * 1.9 + rng.random_range(0.0..3.0), offset)));
                        }
                        f.spawn((
                            PointLight { color: color.light(), intensity: 30_000.0, range: 7.0, shadow_maps_enabled: false, ..default() },
                            Transform::from_xyz(0.0, 0.4, if pit { 0.0 } else { 0.35 }),
                            HearthLight,
                        ));
                    })
                    .id();
                commands.entity(e).insert(Lit { until: clock.minutes + BURN_MINUTES, flames, next_spark: clock.minutes + 60.0 });
                debug!("the {} ({e:?}) is lit, flames at {at:?} (half {:?})", o.name, o.half);
            }
            FireplaceRequest::PutOut(e) => {
                if let Ok((_, Some(l), _)) = places.get(e) {
                    commands.entity(l.flames).try_despawn();
                    commands.entity(e).remove::<Lit>();
                }
            }
        }
    }
}

/// Fires burn down in time; once an hour a spark may catch the floor in front.
fn burn(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut lit: Query<(Entity, &mut Lit, &GameObject, &Transform, Option<&Floor>, Option<&crate::upgrades::Upgrades>)>,
    mut fire: MessageWriter<crate::fire::StartFire>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let mut rng = rand::rng();
    for (e, mut l, o, tf, floor, upgrades) in &mut lit {
        if clock.minutes >= l.until {
            commands.entity(l.flames).try_despawn();
            commands.entity(e).remove::<Lit>();
            continue;
        }
        if clock.minutes >= l.next_spark {
            l.next_spark = clock.minutes + 60.0;
            // (Never from a fireproof one: "Fireproof".)
            if rng.random_bool(0.015) && !crate::upgrades::has(upgrades, crate::upgrades::Upgrade::Fireproof) {
                let at = tf.translation + tf.rotation * Vec3::new(o.center.x, 0.0, o.center.y + o.half.y + 0.9);
                fire.write(crate::fire::StartFire { at, level: floor.map_or(1, |f| f.0) });
                notes.push(format!("A spark from the {} caught the floor!", o.name));
            }
        }
    }
}

/// The hearth's light flickers.
fn glow(
    time: Res<Time>,
    mut lights: Query<(&mut PointLight, &ChildOf), With<HearthLight>>,
    mut candles: Query<&mut PointLight, (With<CandleLight>, Without<HearthLight>)>,
    raging: Query<&Lit, With<Raging>>,
) {
    let t = time.elapsed_secs();
    // (A poked fire roars brighter.)
    let roaring: Vec<Entity> = raging.iter().map(|l| l.flames).collect();
    for (mut l, parent) in &mut lights {
        let up = if roaring.contains(&parent.parent()) { 1.6 } else { 1.0 };
        l.intensity = up * (26_000.0 + 9_000.0 * ((t * 11.0).sin() * 0.5 + (t * 27.0).sin() * 0.5));
    }
    for mut l in &mut candles {
        l.intensity = 2_300.0 + 400.0 * ((t * 13.0).sin() * 0.5 + (t * 31.0).sin() * 0.5);
    }
}

/// A fire's warmth: those near it Cozy (the game's `BuffCozyFire` pulse: stronger while it
/// rages), and a poked fire back to burning once its time's up.
fn cozy(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    mut last: Local<f32>,
    fires: Query<(Entity, &GameObject, &GlobalTransform, Option<&Raging>), With<Lit>>,
    mut sims: Query<(&GlobalTransform, &mut crate::life::Moodlets), With<crate::sim::Sim>>,
) {
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    for (e, o, tf, raging) in &fires {
        if o.kind == crate::interact::ObjectKind::Candle {
            continue;
        }
        if raging.is_some_and(|r| clock.minutes >= r.0) {
            commands.entity(e).remove::<Raging>();
        }
        let strength = if raging.is_some_and(|r| clock.minutes < r.0) { COZY_RAGING } else { COZY };
        let at = tf.translation();
        for (stf, mut m) in &mut sims {
            let p = stf.translation();
            if (p.y - at.y).abs() > 1.8 || p.xz().distance(at.xz()) > COZY_RANGE {
                continue;
            }
            m.add_valued(crate::life::MoodletKind::CozyFire, clock.minutes, strength, COZY_MINUTES);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(traits: &[crate::life::Trait], age: crate::sim::Age) -> crate::sim::Sim {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), age);
        s.traits = traits.to_vec();
        s
    }

    #[test]
    fn daredevils_play_with_fire_and_the_clumsy_get_burned_more() {
        use crate::life::Trait;
        use crate::sim::Age;
        assert!(offers(Special::PlayWithFire, &sim(&[Trait::Daredevil], Age::Teen), None, None, PLAY_WITH_FIRE));
        assert!(!offers(Special::PlayWithFire, &sim(&[Trait::Daredevil], Age::Child), None, None, PLAY_WITH_FIRE));
        assert!(!offers(Special::PlayWithFire, &sim(&[Trait::Clumsy], Age::Adult), None, None, PLAY_WITH_FIRE));
        assert!((burn_chance(false) - 0.10).abs() < 1e-9 && (burn_chance(true) - 0.35).abs() < 1e-9);
    }

    #[test]
    fn a_fire_changes_colour_once_upgraded() {
        use crate::sim::Age;
        let me = sim(&[], Age::Child);
        let up = crate::upgrades::Upgrades(crate::upgrades::Upgrade::ChangeColor.bit());
        assert!(!offers(Special::ChangeFireColor, &me, None, None, "Blue"));
        assert!(offers(Special::ChangeFireColor, &me, Some(&up), None, "Blue"));
        // (Not to the colour it is.)
        assert!(!offers(Special::ChangeFireColor, &me, Some(&up), None, "Normal"));
        assert!(offers(Special::ChangeFireColor, &me, Some(&up), Some(&FireColor::Purple), "Normal"));
        assert!(!offers(Special::ChangeFireColor, &sim(&[], Age::Toddler), Some(&up), None, "Blue"));
        // (The tuning's light: blue's all blue, purple's red and blue.)
        assert_eq!(FireColor::Blue.light().to_linear(), LinearRgba::rgb(0.0, 0.0, 1.0));
        assert_eq!(FireColor::Purple.light().to_linear(), LinearRgba::rgb(1.0, 0.0, 1.0));
        assert_eq!(FireColor::of("Green"), Some(FireColor::Green));
        for c in FireColor::ALL {
            assert_eq!(FireColor::ALL[c as usize], c);
        }
    }
}
