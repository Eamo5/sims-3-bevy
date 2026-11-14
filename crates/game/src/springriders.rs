//! Playground spring riders, Generations' (`PlaygroundRide`: the Bust R Bronco, the Rockin' Rocket,
//! the High Seas Hijinks pirate ship, the Snappin' Seahorse, the Busy Bee Bouncing Bobble, the
//! future's Springy Speeder and BoingBike). A child rides one a quarter to half an hour
//! (`kTimeRangeForRiding`): up onto it from its side, rocking on its springs, the ride rocking with
//! them (its own half of the game's `playgroundRide` clips), and off; fun (`Ride`: Fun 100 an
//! hour). While it's ridden it plays its tune (`GetMusicLoopName`: the rocket's, the pirate
//! ship's...; the bronco has none in the game's sounds).
//!
//! A teen or grown-up sets a toddler on one ("Let <Toddler> Ride", `LetCarriedSimRide`; picked up
//! first if they're not in their arms, "Let Toddler Ride..." for one of several): from beside it,
//! the toddler set on and rocked, a while (the ride's own time), and taken back into their arms,
//! now and then unhappy at it (`kChanceForToddlerSadReaction`); fun and company for both
//! (`LetCarriedSimRide`: Fun 20, Social 10 an hour; the toddler's `kToddlerFunDelta`,
//! `kToddlerSocialDelta`).

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, ClipLibrary};
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::sim::{Age, Sim};

pub struct SpringRidersPlugin;

impl Plugin for SpringRidersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tunes, toddler_rides, spring_rider_test).chain().after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`Sims3/Gameplay/Objects/Environment/PlaygroundRide`).
pub fn let_ride(first: &str) -> String {
    format!("Let {first} Ride")
}
pub const LET_RIDE_PICKER: &str = "Let Toddler Ride...";
/// How long a ride lasts (game minutes: `kTimeRangeForRiding`), how likely a toddler's unhappy at
/// it, and what it's worth an hour: the grown-up's fun and company, the toddler's.
const RIDE_MINUTES: (f64, f64) = (15.0, 30.0);
const SAD_CHANCE: f64 = 0.4;
const ADULT_FUN: f32 = 20.0;
const ADULT_SOCIAL: f32 = 10.0;
const TODDLER_FUN: f32 = 10.0;
const TODDLER_SOCIAL: f32 = 10.0;
/// Where a grown-up stands to set a toddler on (its routing slot 3, `kAdultRoutingSlots`'s
/// unmirrored one), facing it.
const ADULT_AT: Vec2 = Vec2::new(0.5, 0.0);

/// Where a grown-up stands beside a spring rider standing so, and the way they face.
pub fn adult_spot(ride: &Transform) -> (Vec2, Vec2) {
    let at = ride.transform_point(Vec3::new(ADULT_AT.x, 0.0, ADULT_AT.y)).xz();
    (at, (ride.translation.xz() - at).normalize_or(Vec2::X))
}

/// A spring rider's menu for a teen or grown-up: "Let <Toddler> Ride", the one in their arms, or
/// the one about (picked up first); "Let Toddler Ride... ›" one of several.
pub fn adult_options(ride: Entity, rtf: &Transform, holding: Option<(Entity, &str)>, about: &[(Entity, String)], pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>, submenu: impl Fn(usize) -> ActionKind) -> Vec<(String, ActionKind)> {
    let (at, face) = adult_spot(rtf);
    let act = |child: Entity| ActionKind::ToddlerRide { ride, child, at, face };
    match (holding, about) {
        (Some((c, name)), _) => vec![(let_ride(name), act(c))],
        (None, []) => Vec::new(),
        (None, [(c, name)]) => vec![(let_ride(name), act(*c))],
        (None, many) => {
            let list: Vec<(String, ActionKind)> = many.iter().map(|(c, n)| (n.clone(), act(*c))).collect();
            let out = vec![(format!("{LET_RIDE_PICKER} ›"), submenu(pie_submenus.len()))];
            pie_submenus.push((LET_RIDE_PICKER.to_string(), list));
            out
        }
    }
}

/// A toddler on a spring rider (its clips theirs to see to here, not the arms').
#[derive(Component)]
pub struct OnRide;

/// A part of a toddler's ride: set on, rocked, taken off (glad or not).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    On,
    Start,
    Loop(u8),
    Off(bool),
    Back(bool),
}

impl Part {
    /// The clip's name for it (`a2p_playgroundRide_Toddler_loop1`: `_x` the grown-up's, `_y` the
    /// toddler's, `_springRider` the ride's).
    fn stem(self) -> String {
        match self {
            Part::On => "a2p_playgroundRide_Toddler_start".into(),
            Part::Start => "a2p_playgroundRide_Toddler_start1".into(),
            Part::Loop(n) => format!("a2p_playgroundRide_Toddler_loop{n}"),
            Part::Off(sad) => format!("a2p_playgroundRide_Toddler_{}_stop", if sad { "negative" } else { "positive" }),
            Part::Back(sad) => format!("a2p_playgroundRide_Toddler_{}", if sad { "stopBad2" } else { "stop2" }),
        }
    }
}

/// A grown-up's toddler ride under way: the part, how far in, how long it is, till when it lasts.
#[derive(Component)]
struct ToddlerRiding {
    child: Entity,
    part: Part,
    time: f32,
    length: f32,
    until: f64,
    sad: bool,
}

/// Toddlers set on spring riders: picked up first if need be, then the two's clips part by part
/// (the ride's own half with them), fun for both, and back into the arms.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn toddler_rides(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<ClipLibrary>),
    mut grown: Query<(Entity, &Sim, &mut ActionQueue, &Transform, &mut crate::sim::Motives, Option<&crate::carry::Holding>, Option<&mut ToddlerRiding>), (Without<OnRide>, Without<crate::little::Carried>)>,
    mut littles: Query<(&Sim, &mut Transform, &mut crate::sim::Motives, Has<crate::little::InCrib>), (With<crate::little::Carried>, Without<ToddlerRiding>)>,
    free: Query<(&Sim, Has<crate::little::InCrib>), Without<crate::little::Carried>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = delta.0 / 60.0;
    let mut rng = rand::rng();
    for (e, sim, mut q, tf, mut m, holding, riding) in &mut grown {
        let front = q.0.front().filter(|a| !a.cancel).and_then(|a| match a.kind {
            ActionKind::ToddlerRide { child, .. } => Some((child, a.phase)),
            _ => None,
        });
        let Some(mut r) = riding else {
            let Some((child, phase)) = front else { continue };
            // (Not in their arms: picked up first, where they are.)
            if holding.is_none_or(|h| h.0 != child) {
                if matches!(phase, Phase::Start)
                    && let Ok((little, in_crib)) = free.get(child)
                    && !in_crib
                {
                    let step = crate::carry::CarryStep::PickUp { crib: None };
                    q.0.push_front(Action::new(crate::carry::label(step, &little.first, false), ActionKind::Carry { child, step, secs: f32::INFINITY }, false));
                } else if !matches!(phase, Phase::Start) {
                    q.0.pop_front();
                }
                continue;
            }
            if !matches!(phase, Phase::Running(_)) {
                continue;
            }
            let until = clock.minutes + rng.random_range(RIDE_MINUTES.0..RIDE_MINUTES.1);
            let sad = rng.random_bool(SAD_CHANCE);
            commands.entity(child).insert(OnRide);
            commands.entity(e).remove::<crate::anim::Carrying>().insert(ToddlerRiding { child, part: Part::On, time: 0.0, length: 0.0, until, sad });
            info!("spring riders: {} sets a toddler on the ride", sim.first);
            continue;
        };
        let child = r.child;
        let Ok((little, mut ltf, mut lm, _)) = littles.get_mut(child) else {
            commands.entity(e).remove::<(ToddlerRiding, ActionClip)>();
            continue;
        };
        // (Off to something else midway: taken back into the arms as they are.)
        if front.is_none_or(|f| f.0 != child) {
            commands.entity(e).remove::<(ToddlerRiding, ActionClip)>();
            commands.entity(child).remove::<(OnRide, ActionClip)>();
            continue;
        }
        // (On the ride, where the clips have them: in the grown-up's frame.)
        ltf.translation = tf.translation;
        ltf.rotation = tf.rotation;
        if r.length > 0.0 {
            r.time += dt;
            if matches!(r.part, Part::Loop(_)) && hours > 0.0 {
                m.add(crate::sim::FUN, ADULT_FUN * hours);
                m.add(crate::sim::SOCIAL, ADULT_SOCIAL * hours);
                lm.add(crate::sim::FUN, TODDLER_FUN * hours);
                lm.add(crate::sim::SOCIAL, TODDLER_SOCIAL * hours);
            }
            if r.time < r.length {
                continue;
            }
        }
        let next = match r.part {
            _ if r.length == 0.0 => Some(Part::On),
            Part::On => Some(Part::Start),
            Part::Start | Part::Loop(_) if clock.minutes < r.until => Some(Part::Loop(rng.random_range(1..=3))),
            Part::Start | Part::Loop(_) => Some(Part::Off(r.sad)),
            Part::Off(sad) => Some(Part::Back(sad)),
            Part::Back(_) => None,
        };
        let Some(part) = next else {
            // Done: back in their arms.
            info!("spring riders: {} takes {} off the ride{}", sim.first, little.first, if r.sad { " (unhappy)" } else { "" });
            q.0.pop_front();
            commands.entity(e).remove::<(ToddlerRiding, ActionClip)>();
            commands.entity(child).remove::<(OnRide, ActionClip)>();
            continue;
        };
        let stem = part.stem();
        let (x, y) = (format!("{stem}_x"), format!("{stem}_y"));
        if !data.0.clip_names.iter().any(|n| n.eq_ignore_ascii_case(&x)) {
            warn!("spring riders: no {x}");
            q.0.pop_front();
            continue;
        }
        r.part = part;
        r.time = 0.0;
        r.length = lib.get(&data, &x).map_or(2.0, |c| c.duration).max(0.3);
        let (x, y): (&'static str, &'static str) = (crate::pets::intern(x), crate::pets::intern(y));
        commands.entity(e).insert(ActionClip::new(None, Box::leak(Box::new([x]))));
        commands.entity(child).insert(ActionClip::new(None, Box::leak(Box::new([y]))));
        debug!("spring riders: {:?}: {x}", part);
    }
}

/// A ride's tune by its kind (its instance name), as its class names it.
pub fn tune(instance: &str) -> Option<&'static str> {
    let i = instance.to_ascii_lowercase();
    Some(match () {
        _ if i.contains("spaceship") => "pground_loop_rocket",
        _ if i.contains("pirate") => "pground_loop_pirate",
        _ if i.contains("seahorse") => "pground_loop_seahorse",
        _ if i.contains("season") => "pground_loop_bumblebee",
        _ if i.contains("futurecar") => "pground_loop_future_car",
        _ if i.contains("futurebike") => "pground_loop_future_bike",
        _ => return None,
    })
}

/// A ride being ridden plays its tune; let alone, it stops.
fn tunes(
    mut commands: Commands,
    data: Res<crate::baked::Baked>,
    riders: Query<&ActionQueue>,
    rides: Query<(Entity, &GameObject, Has<crate::sound::AmbientSound>)>,
) {
    let ridden: Vec<Entity> = riders
        .iter()
        .filter_map(|q| q.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel))
        .filter_map(|a| match a.kind {
            ActionKind::Object { target, .. } | ActionKind::ToddlerRide { ride: target, .. } => Some(target),
            _ => None,
        })
        .collect();
    for (e, o, playing) in &rides {
        if o.kind != ObjectKind::SpringRider {
            continue;
        }
        let on = ridden.contains(&e);
        if on && !playing {
            let instance = data.0.catalog.iter().find(|c| c.objd == o.objd).map(|c| c.instance_name.as_str()).unwrap_or("");
            if let Some(t) = tune(instance) {
                commands.entity(e).insert(crate::sound::AmbientSound(t.to_string()));
            }
        } else if !on && playing {
            commands.entity(e).remove::<crate::sound::AmbientSound>();
        }
    }
}

/// `SPRING_RIDER_TEST=<instance>` (tests): ten seconds in, that spring rider (`springRiderSpaceship`)
/// put down near the household's first child, who rides it; the camera on it.
#[allow(clippy::type_complexity)]
fn spring_rider_test(
    mut commands: Commands,
    time: Res<Time>,
    mut kids: Query<(&crate::sim::Sim, &Transform, &mut ActionQueue, Entity), With<crate::sim::HouseholdMember>>,
    rides: Query<(Entity, &GameObject, &Transform), Without<crate::sim::Sim>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
    grid: Option<Res<crate::nav::NavGrid>>,
) {
    let Ok(name) = std::env::var("SPRING_RIDER_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let ride = rides.iter().find(|r| r.1.kind == ObjectKind::SpringRider);
    if let (Ok(mut c), Some((_, _, rtf))) = (cams.single_mut(), ride) {
        let side = (rtf.rotation * Vec3::X).xz();
        c.look_at(rtf.translation + Vec3::Y * 0.5);
        // (From the other side for a toddler's, the grown-up not in the way.)
        let flip = if std::env::var_os("SPRING_RIDER_TODDLER").is_some() { std::f32::consts::PI } else { 0.0 };
        c.yaw = side.x.atan2(side.y) + 0.6 + flip;
        c.pitch = 0.4;
        c.distance = 4.0;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let toddler = std::env::var_os("SPRING_RIDER_TODDLER").is_some();
            let Some((_, ktf, ..)) = kids.iter().find(|k| if toddler { k.0.age == crate::sim::Age::Toddler } else { k.0.age == crate::sim::Age::Child }) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&name)).map(|c| c.objd) else { return };
            // (On open ground near them, round it free.)
            let g = grid.as_deref();
            let free = |p: Vec2| g.is_none_or(|g| g.cell_of(p).is_some_and(|(x, z)| !g.is_blocked(x, z)));
            let here = ktf.translation.xz();
            let spot = (2..20).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32)).find(|c| (-2..=2).all(|i| (-2..=2).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.6)))).unwrap_or(here + Vec2::Y * 3.0);
            let p = Vec3::new(spot.x, ktf.translation.y, spot.y);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("spring rider test: {name} at {p:.1}");
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((r, _, rtf)) = ride else { return };
            *stage = (2, now);
            // (SPRING_RIDER_TODDLER=1: the household's toddler set on it by a grown-up.)
            if std::env::var_os("SPRING_RIDER_TODDLER").is_some() {
                let Some((child, little)) = kids.iter().find(|k| k.0.age == crate::sim::Age::Toddler).map(|k| (k.3, k.0.first.clone())) else { return };
                let (at, face) = adult_spot(rtf);
                if let Some((sim, _, mut q, _)) = kids.iter_mut().find(|k| crate::carry::can_carry(k.0.age)) {
                    q.push_player(Action::new(let_ride(&little), ActionKind::ToddlerRide { ride: r, child, at, face }, false));
                    info!("spring rider test: {} lets {little} ride", sim.first);
                }
                return;
            }
            if let Some((sim, _, mut q, _)) = kids.iter_mut().find(|k| k.0.age == crate::sim::Age::Child) {
                let def = crate::interact::interactions_for(ObjectKind::SpringRider).iter().position(|d| d.name == "Ride").unwrap_or(0);
                q.push_player(crate::interact::Action::new("Ride", ActionKind::Object { target: r, def }, false));
                info!("spring rider test: {} rides", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_ride_has_its_tune() {
        assert_eq!(tune("springRiderSpaceship"), Some("pground_loop_rocket"));
        assert_eq!(tune("springRiderVenueSeahorse"), Some("pground_loop_seahorse"));
        assert_eq!(tune("springRiderSeason"), Some("pground_loop_bumblebee"));
        assert_eq!(tune("springRiderHorse"), None, "the bronco's isn't in the game's sounds");
        assert_eq!(ObjectKind::from_script("Sims3.Gameplay.Objects.Environment.SpringRiderPirateShip", "springRiderPirateShip"), ObjectKind::SpringRider);
        assert!(ObjectKind::SpringRider.usable_by(crate::sim::Age::Child) && !ObjectKind::SpringRider.usable_by(crate::sim::Age::Adult));
    }

    #[test]
    fn a_toddler_is_let_ride_from_beside_it() {
        let ride = Entity::from_raw_u32(1).unwrap();
        let (a, b) = (Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let tf = Transform::IDENTITY;
        let (at, face) = adult_spot(&tf);
        assert!((at - Vec2::new(0.5, 0.0)).length() < 1e-5 && (face - Vec2::NEG_X).length() < 1e-5);
        let mut subs = Vec::new();
        let held = adult_options(ride, &tf, Some((a, "Dakota")), &[], &mut subs, |_| ActionKind::Dismount);
        assert_eq!(held[0].0, "Let Dakota Ride");
        assert!(matches!(held[0].1, ActionKind::ToddlerRide { child, .. } if child == a));
        assert!(adult_options(ride, &tf, None, &[], &mut subs, |_| ActionKind::Dismount).is_empty());
        let many = adult_options(ride, &tf, None, &[(a, "Dakota".into()), (b, "Lara".into())], &mut subs, |_| ActionKind::Dismount);
        assert_eq!(many[0].0, "Let Toddler Ride... ›");
        assert_eq!(subs[0].1.len(), 2);
        assert_eq!(Part::Off(true).stem(), "a2p_playgroundRide_Toddler_negative_stop");
        assert_eq!(Part::Back(false).stem(), "a2p_playgroundRide_Toddler_stop2");
    }
}
