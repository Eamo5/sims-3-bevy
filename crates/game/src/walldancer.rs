//! The cats' wall dancer, the Pets pack's (`CatToyWallDancer`: a bird or a feather on a spring,
//! hung on a wall). A bored cat plays with it on its own (`Play`, `CatWallToyPlay`: the cat's half
//! of the pack's clips, the toy bobbing with its own: `petneeds`, `pettoys`). A Sim plays with a
//! cat at it ("Play With... ›" a cat of theirs that doesn't dislike them, the game's
//! `kMiniumRequiredLtr`): the cat's called over before it, the Sim stands to one side (the left
//! side's clips mirrored), and they play it together (`catwalltoyplaywith`: its start, rounds of
//! play, its end) for half an hour (`kMaxTimeToPlayWithToyInMinutes`), both having fun and
//! company (`RunPlayWith`: Fun 60, Social 40 an hour) and the cat liking them the better for it
//! (`kLtrGain`).

use bevy::prelude::*;
use rand::Rng;

use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{ActionKind, ActionQueue, GameObject, Phase};
use crate::pets::{Pet, PetAnim, PetBonds, PetBusy};
use crate::sim::Sim;

pub struct WallDancerPlugin;

impl Plugin for WallDancerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (play_with, walldancer_test).chain().run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`Sims3/Gameplay/Objects/Pets/CatToyWallDancer`).
pub const PLAY_WITH: &str = "Play With";
/// How long they play (game minutes), how much the cat warms to them for it, and how little it
/// may like them to be asked.
pub const SET_MINUTES: f64 = 30.0;
const LIKING: f32 = 20.0;
pub const MIN_LIKING: f32 = -10.0;
/// What playing together's worth an hour, to both (`RunPlayWith`'s tuning).
const FUN_RATE: f32 = 60.0;
const SOCIAL_RATE: f32 = 40.0;

/// The toy's places, in its frame (out from the wall along +Z): the cat's before it, facing it;
/// a grown-up's or teen's to either side, a child's a little nearer (`mAdultSlots`, `mChildSlots`),
/// facing across it.
pub const CAT_SLOT: Vec2 = Vec2::new(0.0, 0.389);
const SIDE_Z: f32 = 0.15;
const ADULT_X: f32 = 0.6;
const CHILD_X: f32 = 0.474;

/// Where the cat stands to play, and the way it faces (the wall).
pub fn cat_spot(toy: &Transform) -> (Vec2, Vec2) {
    let at = toy.transform_point(Vec3::new(CAT_SLOT.x, 0.0, CAT_SLOT.y)).xz();
    (at, -(toy.rotation * Vec3::Z).xz().normalize_or(Vec2::Y))
}

/// Where a Sim stands to play with a cat at it (slot 0 the left, played mirrored; 1 the right),
/// and the way they face (across, at the cat).
pub fn sim_spot(toy: &Transform, slot: u8, child: bool) -> (Vec2, Vec2) {
    let x = if child { CHILD_X } else { ADULT_X };
    let x = if slot == 0 { -x } else { x };
    let at = toy.transform_point(Vec3::new(x, 0.0, SIDE_Z)).xz();
    let across = (toy.rotation * Vec3::X).xz().normalize_or(Vec2::X);
    (at, if slot == 0 { across } else { -across })
}

/// The side a Sim coming from `from` plays from: the nearer.
pub fn nearer_slot(toy: &Transform, from: Vec2) -> u8 {
    let local = toy.compute_affine().inverse().transform_point3(Vec3::new(from.x, toy.translation.y, from.y));
    if local.x < 0.0 { 0 } else { 1 }
}

/// A part of the set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    /// The cat on its way over.
    Fetch,
    Start,
    Loop(u8),
    Stop,
}

impl Part {
    fn name(self) -> String {
        match self {
            Part::Fetch => "breathe".into(),
            Part::Start => "start".into(),
            Part::Loop(n) => format!("loop{n}"),
            Part::Stop => "stop".into(),
        }
    }
}

/// The Sim's clip for a part: theirs by age (`a2ac`, `t2ac`, `c2ac`), mirrored on the left.
pub fn sim_clip(age: &str, part: Part, mirrored: bool) -> String {
    let m = if mirrored { "_mirrored" } else { "" };
    format!("{age}2ac_wallDancerToy_{}{m}_x", part.name())
}

/// The cat's half of it: the same for its own (as the game has it, mirrored or not), else the
/// grown-up's.
pub fn cat_clip(age: &str, part: Part, mirrored: bool, has: impl Fn(&str) -> bool) -> String {
    let n = part.name();
    let m = if mirrored { "_mirrored" } else { "" };
    [format!("{age}2ac_wallDancerToy_{n}{m}_y"), format!("a2ac_wallDancerToy_{n}{m}_y"), format!("a2ac_wallDancerToy_{n}_y")].into_iter().find(|c| has(c)).unwrap_or_else(|| format!("a2ac_wallDancerToy_{n}_y"))
}

/// The toy's own half of a part (`o_wallDancerToy_loop4_wallDancerToy`, mirrored if it has one).
pub fn toy_clip(part: Part, mirrored: bool, has: impl Fn(&str) -> bool) -> Option<String> {
    let n = part.name();
    let plain = format!("o_wallDancerToy_{n}_wallDancerToy");
    let m = format!("{plain}_mirrored");
    if mirrored && has(&m) {
        Some(m)
    } else {
        has(&plain).then_some(plain)
    }
}

/// The toy's half of a cat's clip played on its own (`a2ac_wallDancerToy_loop1_y`:
/// `o_wallDancerToy_loop1_wallDancerToy`).
pub fn half_of(names: &[String], clip: &str) -> Option<String> {
    let part = clip.strip_prefix("a2ac_wallDancerToy_")?.strip_suffix("_y")?;
    let want = format!("o_wallDancerToy_{part}_wallDancerToy");
    names.iter().find(|n| n.eq_ignore_ascii_case(&want)).cloned()
}

/// The Sims' age in the clips' names.
pub fn age_of(sim: &Sim) -> &'static str {
    match sim.age {
        crate::sim::Age::Child => "c",
        crate::sim::Age::Teen => "t",
        _ => "a",
    }
}

/// A cat at the wall dancer with a Sim (theirs to see to here).
#[derive(Component)]
pub struct AtDancer;

/// A Sim playing with a cat at the wall dancer: the cat, the toy, the part they're at and how far
/// in, since when, and the cat's way over.
#[derive(Component)]
pub struct DancerSet {
    cat: Entity,
    toy: Entity,
    part: Part,
    time: f32,
    length: f32,
    since: f64,
    route: Vec<crate::nav::Waypoint>,
    mirrored: bool,
    toy_clip: Option<String>,
    cat_clip: String,
}

/// A Sim's menu on the wall dancer: "Play With... ›" the household's grown cats that don't
/// dislike them (none: not offered).
pub fn options(toy: Entity, slot: u8, child: bool, cats: &[(Entity, String)], pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>, submenu: impl Fn(usize) -> ActionKind) -> Vec<(String, ActionKind)> {
    if cats.is_empty() {
        return Vec::new();
    }
    let list: Vec<(String, ActionKind)> = cats.iter().map(|(c, name)| (name.clone(), ActionKind::WallDancerWith { toy, cat: *c, slot, child })).collect();
    let out = vec![(format!("{PLAY_WITH}... ›"), submenu(pie_submenus.len()))];
    pie_submenus.push((format!("{PLAY_WITH}..."), list));
    out
}

/// A Sim and a cat playing with the wall dancer: the cat called over, then the set's parts, the
/// cat's and the toy's halves with the Sim's.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn play_with(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Transform, &mut crate::sim::Motives, Option<&mut DancerSet>), Without<Pet>>,
    mut pets: Query<(&Pet, &mut Transform, &mut PetAnim, &mut crate::petneeds::PetMotives, Option<&mut PetBonds>), Without<Sim>>,
    toys: Query<(&GlobalTransform, Option<&crate::nav::Floor>), With<GameObject>>,
    (rigs, mut bones): (Query<&crate::objanim::ObjectRig>, Query<&mut Transform, (Without<Pet>, Without<Sim>, Without<GameObject>)>),
    (world, building, grid, upper): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::nav::NavGrid>>, Option<Res<crate::nav::UpperFloors>>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    let has = |n: &str| data.0.clip_names.iter().any(|c| c.eq_ignore_ascii_case(n));
    for (e, sim, mut queue, mut stf, mut motives, session) in &mut sims {
        let running = queue.0.front().and_then(|a| match a.kind {
            ActionKind::WallDancerWith { toy, cat, slot, .. } if matches!(a.phase, Phase::Running(_)) && !a.cancel => Some((toy, cat, slot)),
            _ => None,
        });
        let Some(mut s) = session else {
            // (There: the cat's called over, out of whatever it was doing.)
            if let Some((toy, cat, slot)) = running {
                let (Ok((_, ptf, ..)), Ok((ttf, floor))) = (pets.get(cat), toys.get(toy)) else {
                    queue.0.pop_front();
                    continue;
                };
                let ttf = ttf.compute_transform();
                let level = floor.map_or(1, |f| f.0);
                let (to, _) = cat_spot(&ttf);
                let mut route = grid.as_deref().and_then(|g| crate::nav::plan_route(g, upper.as_deref(), ptf.translation.xz(), 1, to, level)).unwrap_or_default();
                route.push(crate::nav::Waypoint { p: to, level, climb: None, spiral: None });
                let (_, face) = sim_spot(&ttf, slot, sim.age == crate::sim::Age::Child);
                stf.rotation = Quat::from_rotation_y(face.x.atan2(face.y));
                info!("wall dancer: {} calls a cat over to play", sim.first);
                commands.entity(e).insert(DancerSet { cat, toy, part: Part::Fetch, time: 0.0, length: 0.0, since: now, route, mirrored: slot == 0, toy_clip: None, cat_clip: String::new() });
                commands.entity(cat).insert((PetBusy, AtDancer)).remove::<crate::petneeds::PetTask>();
            }
            continue;
        };
        let Ok((pet, mut ptf, mut anim, mut pm, bonds)) = pets.get_mut(s.cat) else {
            commands.entity(e).remove::<(DancerSet, ActionClip)>();
            continue;
        };
        let kind = pet.kind.clone();
        let (cat, toy_e) = (s.cat, s.toy);
        let rest = |bones: &mut Query<&mut Transform, (Without<Pet>, Without<Sim>, Without<GameObject>)>| {
            if let Ok(rig) = rigs.get(toy_e) {
                for (b, pose) in rig.rest_pose() {
                    if let Ok(mut btf) = bones.get_mut(b) {
                        *btf = pose;
                    }
                }
            }
        };
        // (Off to something else, or told to stop: over, the cat free.)
        if running.is_none_or(|r| r.1 != cat) {
            commands.entity(e).remove::<(DancerSet, ActionClip)>();
            commands.entity(cat).remove::<(PetBusy, AtDancer)>();
            anim.play(crate::pets::idle_clip(&kind, false, &mut rng));
            rest(&mut bones);
            continue;
        }
        let Ok((ttf, _)) = toys.get(s.toy) else { continue };
        let ttf = ttf.compute_transform();
        let mut next = None;
        if s.part == Part::Fetch {
            // Over to its place before the toy.
            if !s.route.is_empty() {
                let step = pet.walk_speed() * 1.6 * dt;
                crate::petbath::walk_route(&mut s.route, &mut ptf, step, |l, p| crate::nav::floor_height(&world.data, building.as_deref(), l, p));
                anim.play(crate::pets::hurry_clip(&kind));
                // (Waiting too long for it: given up.)
                if now - s.since > 60.0 {
                    queue.0.pop_front();
                }
                continue;
            }
            let (at, face) = cat_spot(&ttf);
            ptf.translation = Vec3::new(at.x, ptf.translation.y, at.y);
            ptf.rotation = Quat::from_rotation_y(face.x.atan2(face.y));
            next = Some(Part::Start);
        } else {
            s.time += dt;
            if matches!(s.part, Part::Loop(_)) && hours > 0.0 {
                motives.add(crate::sim::FUN, FUN_RATE * hours);
                motives.add(crate::sim::SOCIAL, SOCIAL_RATE * hours);
                pm.add(crate::petneeds::FUN, FUN_RATE * hours);
                pm.add(crate::petneeds::SOCIAL, SOCIAL_RATE * hours);
            }
            // The toy's half, as far in as they are.
            if let (Some(h), Ok(rig)) = (s.toy_clip.as_ref(), rigs.get(s.toy))
                && let Some(hc) = lib.get(&data, h)
            {
                for (b, pose) in rig.clip_pose(&hc, s.time.clamp(0.0, hc.duration), 0) {
                    if let Ok(mut btf) = bones.get_mut(b) {
                        *btf = pose;
                    }
                }
            }
            if s.time >= s.length {
                next = match s.part {
                    Part::Start => Some(Part::Loop(rng.random_range(1..=4))),
                    Part::Loop(_) if now - s.since >= SET_MINUTES => Some(Part::Stop),
                    Part::Loop(_) => Some(Part::Loop(rng.random_range(1..=4))),
                    _ => {
                        // Done: the cat the fonder of them for it.
                        if let Some(mut b) = bonds {
                            b.add(sim.id, true, LIKING);
                        }
                        info!("wall dancer: {} played with {} ({:.0} minutes)", sim.first, pet.name, now - s.since);
                        queue.0.pop_front();
                        commands.entity(e).remove::<(DancerSet, ActionClip)>();
                        commands.entity(cat).remove::<(PetBusy, AtDancer)>();
                        anim.play(crate::pets::idle_clip(&kind, false, &mut rng));
                        rest(&mut bones);
                        continue;
                    }
                };
            }
        }
        let Some(part) = next else { continue };
        let age = age_of(sim);
        let x = sim_clip(age, part, s.mirrored);
        if !has(&x) {
            warn!("wall dancer: no {x}");
            queue.0.pop_front();
            continue;
        }
        if part == Part::Start {
            s.since = now;
        }
        s.part = part;
        s.time = 0.0;
        s.cat_clip = cat_clip(age, part, s.mirrored, has);
        // (Done a moment before the clips are, the cat's not looping back to its start.)
        s.length = lib.get(&data, &x).map_or(2.0, |c| c.duration).min(lib.get(&data, &s.cat_clip).map_or(f32::MAX, |c| c.duration)).max(0.3) - 0.05;
        s.toy_clip = toy_clip(part, s.mirrored, has);
        let x: &'static str = crate::pets::intern(x);
        let clips: &'static [&'static str] = Box::leak(Box::new([x]));
        commands.entity(e).insert(ActionClip::new(None, clips));
        anim.replay(s.cat_clip.clone());
        debug!("wall dancer: {:?}: {x} / {} / {:?}", s.part, s.cat_clip, s.toy_clip);
    }
}

/// `WALLDANCER_TEST=<cat>` (tests): ten seconds in, a wall dancer hung on the house's nearest
/// wall, the camera on it; `WALLDANCER_WITH=1`, the selected Sim then plays with the cat at it,
/// else the cat's sent to play on its own.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn walldancer_test(
    mut commands: Commands,
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), With<crate::pets::HomePet>>,
    dancers: Query<(Entity, &GameObject, &Transform), Without<Pet>>,
    mut sel: Query<(&Sim, &Transform, &mut ActionQueue), (With<crate::sim::Selected>, Without<Pet>)>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<Baked>),
    (building, grid): (Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("WALLDANCER_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let Some((cat, pet, ptf)) = pets.iter().find(|p| p.1.name.eq_ignore_ascii_case(&name)) else { return };
    let toy = dancers.iter().find(|(_, o, _)| o.kind == crate::interact::ObjectKind::CatWallToy);
    if let (Ok(mut c), Some((_, _, ttf))) = (cams.single_mut(), toy) {
        let out = (ttf.rotation * Vec3::Z).xz();
        c.look_at(ttf.translation + Vec3::Y * 0.3);
        c.yaw = out.x.atan2(out.y);
        c.pitch = 0.35;
        c.distance = 3.5;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let (Ok((_, stf, _)), Some(b)) = (sel.single(), building) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("catToyWallDancer")).map(|c| c.objd) else { return };
            // (On the wall nearest the Sim, half a tile out from it, facing into the room.)
            let here = b.local(stf.translation);
            let y = b.levels.get(1).copied().unwrap_or(0.0);
            let mut walls: Vec<&s3bake::types::WallBaked> = b.data.walls.iter().filter(|w| w.level.max(1) == 1 && ((w.a[0] - w.b[0]).abs() < 0.01 || (w.a[1] - w.b[1]).abs() < 0.01)).collect();
            let dist = |w: &&s3bake::types::WallBaked| {
                let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
                let t = ((here - a).dot(c - a) / (c - a).length_squared()).clamp(0.0, 1.0);
                here.distance(a + (c - a) * t)
            };
            walls.sort_by(|x, y| dist(x).total_cmp(&dist(y)));
            for w in walls {
                let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
                let n = (c - a).normalize().perp();
                for side in [1.0, -1.0] {
                    let lp = (a + c) / 2.0 + n * 0.4 * side;
                    let ray = Ray3d::new(b.world(lp.x, lp.y, y) + Vec3::Y * 5.0, Dir3::NEG_Y);
                    let Some((pos, rot, _)) = crate::build::snap_to_wall(&b, ray, 1) else { continue };
                    if b.opening_behind(pos, rot, 1) {
                        continue;
                    }
                    // (With room before it for the cat and to either side.)
                    let t = Transform::from_translation(pos).with_rotation(rot);
                    let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p)).is_some_and(|(x, z)| !grid.as_deref().unwrap().is_blocked(x, z));
                    if !free(cat_spot(&t).0) || !free(sim_spot(&t, 1, false).0) {
                        continue;
                    }
                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                    if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, pos, rot.to_euler(EulerRot::YXZ).0) {
                        commands.entity(o.entity).insert(crate::save::Bought);
                        info!("wall dancer test: hung at {pos:.1?}");
                        return;
                    }
                }
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((t, _, ttf)) = toy else { return };
            *stage = (2, now);
            if std::env::var_os("WALLDANCER_WITH").is_some() {
                if let Ok((sim, stf, mut q)) = sel.single_mut() {
                    let slot = nearer_slot(ttf, stf.translation.xz());
                    q.push_player(crate::interact::Action::new(PLAY_WITH, ActionKind::WallDancerWith { toy: t, cat, slot, child: sim.age == crate::sim::Age::Child }, false));
                    info!("wall dancer test: {} plays with {}", sim.first, pet.name);
                }
            } else if let Some(grid) = grid.as_deref() {
                let what = crate::petneeds::PetDo::Play(t, crate::pettoys::Toy::WallDancer);
                let (to, _) = cat_spot(ttf);
                if let Some(task) = crate::petneeds::plan(pet, ptf.translation.xz(), what, Some(to), Some(*ttf), grid, None, &mut rand::rng()) {
                    commands.entity(cat).insert(task);
                    info!("wall dancer test: {} sent to play", pet.name);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cat_before_the_toy_and_the_sim_to_its_side() {
        let toy = Transform::from_xyz(2.0, 0.0, 3.0);
        let (cat, face) = cat_spot(&toy);
        assert!((cat - Vec2::new(2.0, 3.389)).length() < 1e-4);
        assert!((face - Vec2::new(0.0, -1.0)).length() < 1e-4, "facing the wall");
        let (left, look) = sim_spot(&toy, 0, false);
        assert!((left - Vec2::new(1.4, 3.15)).length() < 1e-4);
        assert!((look - Vec2::X).length() < 1e-4, "across, at the cat");
        assert_eq!(nearer_slot(&toy, Vec2::new(5.0, 4.0)), 1);
        assert_eq!(nearer_slot(&toy, Vec2::new(0.0, 4.0)), 0);
    }

    #[test]
    fn the_clips_are_the_games() {
        let names = ["a2ac_wallDancerToy_loop4_mirrored_y", "a2ac_wallDancerToy_loop1_y", "o_wallDancerToy_loop4_wallDancerToy", "o_wallDancerToy_loop4_wallDancerToy_mirrored", "o_wallDancerToy_loop1_wallDancerToy"];
        let has = |n: &str| names.contains(&n);
        assert_eq!(sim_clip("a", Part::Loop(1), true), "a2ac_wallDancerToy_loop1_mirrored_x");
        assert_eq!(sim_clip("c", Part::Start, false), "c2ac_wallDancerToy_start_x");
        assert_eq!(cat_clip("a", Part::Loop(4), true, has), "a2ac_wallDancerToy_loop4_mirrored_y");
        assert_eq!(cat_clip("c", Part::Loop(1), true, has), "a2ac_wallDancerToy_loop1_y", "the grown-up's, unmirrored, if that's all there is");
        assert_eq!(toy_clip(Part::Loop(4), true, has).as_deref(), Some("o_wallDancerToy_loop4_wallDancerToy_mirrored"));
        assert_eq!(toy_clip(Part::Loop(1), true, has).as_deref(), Some("o_wallDancerToy_loop1_wallDancerToy"));
        let owned: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        assert_eq!(half_of(&owned, "a2ac_wallDancerToy_loop1_y").as_deref(), Some("o_wallDancerToy_loop1_wallDancerToy"));
    }
}
