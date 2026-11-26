//! Pet rocks, the Pets pack's (`PetStone`: the Essence of Pure Love Rock, Sir Bricks-a-lot Jr.,
//! the Gonk Rock). Children and older "Play with" one (`Play`: Fun 50 an hour, a Loner's favourite):
//! it's picked up (`a2o_petStone_play_start`, a child's `c2o_`), cuddled, kissed, talked to and
//! tossed up (the pack's loops), its picture in their thoughts, till they've had their fun or are
//! told to stop, then it's kept in their inventory (`stop_normal`). Every minute of it there's a
//! one-in-ten chance it slips (`kPercentChanceDrop`) and breaks on their foot (`stop_drop`): Hurt
//! Foot, the sad sting (`sting_pet_stone_sad`), and bits of rock left on the floor
//! (`PetStoneBits`, by its name) to clean up like ash. "Pick up {name}" puts one in their
//! inventory (the game's swipe); from there it's played with ("Play with {name}") or put down at
//! their feet ("Put down {name}"). Bored, a Sim may play with one about of their own accord, a
//! Loner all the more.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::inventory::{Inventory, ItemKind, Stack};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct PetStonesPlugin;

impl Plugin for PetStonesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, hold_on.before(crate::interact::run_actions).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (from_pocket, run, autonomy, stones_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)));
    }
}

/// What's done with a pet rock.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoneAct {
    Play,
    PickUp,
    PutDown,
}

/// The game's tuning: what playing's worth an hour, how likely it slips each minute, how far off
/// it's played with; the swipe it's picked up and put down with; the sting it breaks to.
const FUN_PER_HOUR: f32 = 50.0;
const DROP_CHANCE: f64 = 0.1;
const FROM: f32 = 0.75;
const SWIPE: &str = "a2o_object_genericSwipe_x";
const SAD: &str = "sting_pet_stone_sad";
/// How full fun is when they've had all they can.
const FUN_FULL: f32 = 99.0;

/// Whether a catalogue instance is a pet rock (not its bits).
pub fn is_stone(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.starts_with("petstone") && k != "petstonebits"
}

/// Who plays with one: children and older (the tuning's `C,T,Y,A,E`).
pub fn plays(age: Age) -> bool {
    !age.is_little()
}

/// The game's words (`PetStone:PlayInteractionName`...).
pub fn play_with(name: &str) -> String {
    format!("Play with {name}")
}
pub fn pick_up(name: &str) -> String {
    format!("Pick up {name}")
}
pub fn put_down(name: &str) -> String {
    format!("Put down {name}")
}

/// The play's clips for an adult or a child: in, its loops, out, and dropped.
fn clips(child: bool) -> (&'static str, &'static [&'static str], &'static str, &'static str) {
    if child {
        (
            "c2o_petStone_play_start_x",
            &["c2o_petStone_play_cuddle_loop_x", "c2o_petStone_play_kiss_loop_x", "c2o_petStone_play_talkTo_loop_x", "c2o_petStone_play_tossUp_loop_x"],
            "c2o_petStone_play_stop_normal_x",
            "c2o_petStone_play_stop_drop_x",
        )
    } else {
        (
            "a2o_petStone_play_start_x",
            &["a2o_petStone_play_cuddle_loop_x", "a2o_petStone_play_kiss_loop_x", "a2o_petStone_play_talkTo_loop_x", "a2o_petStone_play_tossUp_loop_x"],
            "a2o_petStone_play_stop_normal_x",
            "a2o_petStone_play_stop_drop_x",
        )
    }
}

/// The action for something done with a rock: from a little way off it on their side, facing it.
pub fn act(stone: Entity, stf: &Transform, act: StoneAct, here: Vec2) -> ActionKind {
    let s = stf.translation.xz();
    let at = s + (here - s).normalize_or(Vec2::NEG_Y) * FROM;
    ActionKind::PetStone { stone, act, at, face: (s - at).normalize_or(Vec2::Y) }
}

/// A rock's menu: "Play with {name}", "Pick up {name}".
pub fn options(stone: Entity, stf: &Transform, name: &str, age: Age, here: Vec2) -> Vec<(String, ActionKind)> {
    if !plays(age) {
        return Vec::new();
    }
    vec![(play_with(name), act(stone, stf, StoneAct::Play, here)), (pick_up(name), act(stone, stf, StoneAct::PickUp, here))]
}

/// A rock taken out of their inventory: to play with, or put down.
#[derive(Component)]
pub struct FromPocket {
    pub item: Stack,
    pub play: bool,
}

/// Playing with a rock: in, at it, putting it away or dropping it; how far along, how long, when
/// the next minute's roll is, and whether to stop.
#[derive(Component)]
struct Playing {
    stage: u8,
    t: f32,
    len: f32,
    next_roll: f64,
    stop: bool,
}

/// Picking one up or putting it down: how far through the swipe.
#[derive(Component)]
struct Swiping {
    t: f32,
    len: f32,
}

/// A rock put down out of an inventory, out of sight till the swipe's done.
#[derive(Component)]
struct Unshown;

/// Told to stop playing: they put it away first (not dropped mid-way).
fn hold_on(mut sims: Query<(&mut ActionQueue, Option<&mut Playing>)>) {
    for (mut q, playing) in &mut sims {
        let Some(a) = q.0.front_mut() else { continue };
        if a.cancel
            && matches!(a.phase, Phase::Running(_))
            && matches!(a.kind, ActionKind::PetStone { act: StoneAct::Play, .. })
            && let Some(mut p) = playing
            && p.stage == 0
        {
            a.cancel = false;
            p.stop = true;
        }
    }
}

/// Rocks out of inventories: put down at their feet, and played with there or left.
#[allow(clippy::type_complexity)]
fn from_pocket(
    mut commands: Commands,
    (data, catalog): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut sims: Query<(Entity, &Sim, &Transform, &FromPocket, &mut ActionQueue, Option<&mut Inventory>)>,
) {
    for (e, sim, tf, f, mut q, inv) in &mut sims {
        commands.entity(e).remove::<FromPocket>();
        let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, FROM);
        let objd = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&f.item.key)).map(|c| c.objd);
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        let Some(o) = objd.and_then(|o| crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, o, at, 0.0)) else {
            // (No model: back where it was.)
            if let Some(mut i) = inv {
                i.put(f.item.clone());
            }
            continue;
        };
        commands.entity(o.entity).insert((crate::save::Bought, Visibility::Hidden, Unshown));
        let stf = Transform::from_translation(at);
        let (label, what) = if f.play { (play_with(&f.item.name), StoneAct::Play) } else { (put_down(&f.item.name), StoneAct::PutDown) };
        let mut kind = act(o.entity, &stf, what, tf.translation.xz());
        // (Right where they are.)
        if let ActionKind::PetStone { at, face, .. } = &mut kind {
            *at = tf.translation.xz();
            *face = (stf.translation.xz() - *at).normalize_or(Vec2::Y);
        }
        q.push_player(Action::new(label, kind, false));
        info!("pet stones: {} takes {} out", sim.first, f.item.name);
    }
}

/// What's done with rocks, played out.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib, catalog): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Res<crate::loading::Catalog>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &Transform, &mut SimAnim, &mut crate::sim::Motives, Option<&mut Playing>, Option<&mut Swiping>, Option<&mut Inventory>), Without<GameObject>>,
    stones: Query<(&GameObject, &Transform, Has<Unshown>), Without<Sim>>,
    mut sounds: MessageWriter<crate::sound::PlaySound>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let hours = delta.0 / 60.0;
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, sim, mut q, tf, mut anim, mut m, playing, swiping, inv) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match a.kind {
            ActionKind::PetStone { stone, act, .. } => Some((stone, act)),
            _ => None,
        });
        let Some((stone, what)) = front else {
            if playing.is_some() || swiping.is_some() {
                commands.entity(e).remove::<(Playing, Swiping, ActionClip)>();
            }
            continue;
        };
        let Ok((obj, stf, unshown)) = stones.get(stone) else {
            q.0.pop_front();
            commands.entity(e).remove::<(Playing, Swiping, ActionClip)>();
            continue;
        };
        // Into their inventory, as it is.
        let pocket = |commands: &mut Commands, inv: Option<Mut<Inventory>>| {
            let key = data.0.catalog.iter().find(|c| c.objd == obj.objd).map_or(String::new(), |c| c.instance_name.clone());
            let item = Stack { kind: ItemKind::Object, key, name: obj.name.clone(), quality: 0, count: 1, worth: obj.price as i64 };
            commands.entity(stone).try_despawn();
            match inv {
                Some(mut i) => i.put(item),
                None => crate::inventory::give(commands, e, item.kind, item.key, item.name, 0, item.worth, 1),
            }
        };
        match what {
            StoneAct::PickUp | StoneAct::PutDown => match swiping {
                None => {
                    let len = lib.get(&data, SWIPE).map_or(1.5, |c| c.duration);
                    anim.pose = Pose::Use;
                    commands.entity(e).insert((Swiping { t: 0.0, len }, ActionClip::new(Some(SWIPE), &[])));
                }
                Some(mut s) => {
                    s.t += dt;
                    if s.t < s.len {
                        continue;
                    }
                    if what == StoneAct::PickUp {
                        pocket(&mut commands, inv);
                        info!("pet stones: {} picks up {}", sim.first, obj.name);
                    } else if unshown {
                        commands.entity(stone).remove::<Unshown>().insert(Visibility::Inherited);
                        info!("pet stones: {} puts down {}", sim.first, obj.name);
                    }
                    q.0.pop_front();
                    anim.pose = Pose::Stand;
                    commands.entity(e).remove::<(Swiping, ActionClip)>();
                }
            },
            StoneAct::Play => {
                let (start, loops, stop, dropped) = clips(sim.age == Age::Child);
                let Some(mut p) = playing else {
                    if unshown {
                        commands.entity(stone).remove::<Unshown>();
                    }
                    anim.pose = Pose::Use;
                    commands.entity(e).insert((
                        Playing { stage: 0, t: 0.0, len: 0.0, next_roll: now + 1.0, stop: false },
                        ActionClip::new(Some(start), loops),
                        crate::balloons::BalloonRequest::thought(&s3bake::gamedata::thumb_name(obj.objd.2)),
                    ));
                    info!("pet stones: {} plays with {}", sim.first, obj.name);
                    continue;
                };
                match p.stage {
                    0 => {
                        if hours > 0.0 {
                            m.add(crate::sim::FUN, FUN_PER_HOUR * hours);
                        }
                        // (Each minute, a chance it slips.)
                        let mut slipped = false;
                        while now >= p.next_roll {
                            p.next_roll += 1.0;
                            slipped |= rng.random_bool(DROP_CHANCE);
                        }
                        if slipped && std::env::var_os("PETSTONE_NO_DROP").is_none() {
                            p.stage = 2;
                            p.t = 0.0;
                            p.len = lib.get(&data, dropped).map_or(3.0, |c| c.duration);
                            commands.entity(e).insert(ActionClip::new(Some(dropped), &[]));
                            info!("pet stones: {} drops {}", sim.first, obj.name);
                        } else if p.stop || m.0[crate::sim::FUN] >= FUN_FULL {
                            p.stage = 1;
                            p.t = 0.0;
                            p.len = lib.get(&data, stop).map_or(2.0, |c| c.duration);
                            commands.entity(e).insert(ActionClip::new(Some(stop), &[]));
                        }
                    }
                    1 => {
                        p.t += dt;
                        if p.t >= p.len {
                            pocket(&mut commands, inv);
                            q.0.pop_front();
                            anim.pose = Pose::Stand;
                            commands.entity(e).remove::<(Playing, ActionClip)>();
                            info!("pet stones: {} puts {} away", sim.first, obj.name);
                        }
                    }
                    _ => {
                        p.t += dt;
                        if p.t < p.len {
                            continue;
                        }
                        // Broken: its bits on the floor, by its name; a hurt foot and the sad sting.
                        let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 0.35);
                        commands.entity(stone).try_despawn();
                        if let Some(bits) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("petStoneBits")).map(|c| c.objd) {
                            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, bits, at, rng.random_range(0.0..std::f32::consts::TAU)) {
                                let name = obj.name.clone();
                                commands.entity(o.entity).insert(crate::save::Bought).queue_silenced(move |mut w: EntityWorldMut| {
                                    if let Some(mut g) = w.get_mut::<GameObject>() {
                                        g.name = name;
                                    }
                                });
                            }
                        }
                        commands.entity(e).queue_silenced(move |mut w: EntityWorldMut| {
                            if let Some(mut ml) = w.get_mut::<crate::life::Moodlets>() {
                                ml.add(crate::life::MoodletKind::HurtFoot, now);
                            }
                        });
                        sounds.write(crate::sound::PlaySound { name: SAD.into(), at: Some(at), volume: 1.0, speed: 1.0 });
                        notes.push(format!("{} dropped {} and it broke!", sim.first, obj.name));
                        q.0.pop_front();
                        anim.pose = Pose::Stand;
                        commands.entity(e).remove::<(Playing, ActionClip)>();
                    }
                }
            }
        }
    }
}

/// Bored Sims of the household play with a rock about now and then, a Loner sooner.
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(&Sim, &mut ActionQueue, &crate::sim::Motives, &Transform), (With<crate::sim::HouseholdMember>, Without<crate::sim::Selected>)>,
    stones: Query<(Entity, &GameObject, &Transform, Has<Unshown>), Without<Sim>>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    if std::env::var_os("PETSTONE_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    for (sim, mut q, m, tf) in &mut sims {
        let loner = sim.traits.contains(&crate::life::Trait::Loner);
        if !plays(sim.age) || !q.0.is_empty() || m.0[crate::sim::FUN] > if loner { 60.0 } else { 20.0 } || !rng.random_bool(if loner { 0.5 } else { 0.2 }) {
            continue;
        }
        let here = tf.translation.xz();
        let near = stones
            .iter()
            .filter(|(_, o, t, unshown)| o.kind == ObjectKind::PetStone && !unshown && t.translation.xz().distance(here) < 20.0)
            .min_by(|a, b| a.2.translation.xz().distance(here).total_cmp(&b.2.translation.xz().distance(here)));
        if let Some((s, o, stf, _)) = near {
            q.0.push_back(Action::new(play_with(&o.name), act(s, stf, StoneAct::Play, here), true));
            info!("pet stones: {} goes to play with {} of their own accord", sim.first, o.name);
        }
    }
}

/// `PETSTONE_TEST=<instance>` (tests: `petStoneBow`, `petStoneSirBricksalot`, `petStoneTeeth`):
/// ten seconds in, one put down near the selected Sim (or `PETSTONE_WHO=<first name>`), who plays
/// with it (`PETSTONE_DO=play|pickup|pocket`: picked up, or picked up and then played with from
/// their inventory); `PETSTONE_NO_DROP=1` keeps it from slipping; `PETSTONE_MINUTES=<n>`: told to
/// stop after that long; `PETSTONE_PIE=1`: its menu.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn stones_test(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>, Option<&Inventory>), With<crate::sim::HouseholdMember>>,
    stones: Query<(Entity, &GameObject, &Transform), (Without<Sim>, With<crate::save::Bought>)>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32, f64)>,
) {
    let Ok(name) = std::env::var("PETSTONE_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let who = env("PETSTONE_WHO");
    let is_me = |s: &Sim, sel: bool| who.as_ref().map_or(sel, |w| s.first.eq_ignore_ascii_case(w));
    if let (Ok(mut c), Some((_, _, tf, ..))) = (cams.single_mut(), sims.iter().find(|s| is_me(s.1, s.4))) {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(tf.translation);
        c.height_offset = f("PETSTONE_LOOK_Y", 0.9);
        c.distance = f("PETSTONE_DIST", 4.0);
        c.pitch = f("PETSTONE_PITCH", 0.3);
        // (Before them, a little to one side.)
        let fwd = tf.rotation * Vec3::Z;
        c.yaw = fwd.x.atan2(fwd.z) + f("PETSTONE_YAW", 0.5);
    }
    match stage.0 {
        0 => {
            *stage = (1, now, clock.minutes);
            let Some((_, _, tf, ..)) = sims.iter().find(|s| is_me(s.1, s.4)) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&name)).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            let spot = (2..12).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| free(*c)).unwrap_or(here + Vec2::X);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, tf.translation.y, spot.y), 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("pet stone test: {name} put down");
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((s, o, stf)) = stones.iter().find(|s| s.1.kind == ObjectKind::PetStone) else { return };
            *stage = (2, now, clock.minutes);
            if env("PETSTONE_PIE").is_some() {
                commands.insert_resource(crate::hud::PieFor(s));
                return;
            }
            let what = match env("PETSTONE_DO").as_deref() {
                Some("pickup") | Some("pocket") => StoneAct::PickUp,
                _ => StoneAct::Play,
            };
            if let Some((_, sim, tf, mut q, ..)) = sims.iter_mut().find(|s| is_me(s.1, s.4)) {
                let label = if what == StoneAct::Play { play_with(&o.name) } else { pick_up(&o.name) };
                q.push_player(Action::new(label, act(s, stf, what, tf.translation.xz()), false));
                info!("pet stone test: {} to {what:?}", sim.first);
            }
        }
        2 => {
            // (Picked up, then played with from the inventory.)
            if env("PETSTONE_DO").as_deref() == Some("pocket")
                && let Some((e, sim, _, q, _, Some(inv))) = sims.iter().find(|s| is_me(s.1, s.4))
                && q.0.is_empty()
                && let Some(st) = inv.0.iter().find(|s| is_stone(&s.key))
            {
                *stage = (3, now, clock.minutes);
                let item = Stack { count: 1, worth: st.each(), ..st.clone() };
                commands.entity(e).insert(FromPocket { item, play: true });
                commands.queue(move |w: &mut World| {
                    if let Some(mut i) = w.get_mut::<Inventory>(e)
                        && let Some(k) = i.0.iter().position(|s| is_stone(&s.key))
                    {
                        i.take_one(k);
                    }
                });
                info!("pet stone test: {} plays with it from their inventory", sim.first);
                return;
            }
            let minutes = env("PETSTONE_MINUTES").and_then(|v| v.parse::<f64>().ok());
            if minutes.is_some_and(|m| clock.minutes - stage.2 >= m)
                && let Some((_, sim, _, mut q, ..)) = sims.iter_mut().find(|s| is_me(s.1, s.4))
                && let Some(a) = q.0.front_mut()
            {
                a.cancel = true;
                *stage = (4, now, clock.minutes);
                info!("pet stone test: {} told to stop", sim.first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pet_rocks_and_their_words() {
        assert!(is_stone("petStoneSirBricksalot") && is_stone("petStoneTeeth") && !is_stone("petStoneBits") && !is_stone("catHuntRodent"));
        assert!(plays(Age::Child) && plays(Age::Elder) && !plays(Age::Toddler));
        assert_eq!(play_with("Gonk Rock"), "Play with Gonk Rock");
        assert_eq!(put_down("Gonk Rock"), "Put down Gonk Rock");
        assert_eq!(clips(true).3, "c2o_petStone_play_stop_drop_x");
        assert_eq!(clips(false).1.len(), 4);
        let e = Entity::from_raw_u32(1).unwrap();
        let ActionKind::PetStone { at, face, .. } = act(e, &Transform::from_xyz(2.0, 0.0, 0.0), StoneAct::Play, Vec2::new(10.0, 0.0)) else { panic!() };
        assert!((at - Vec2::new(2.75, 0.0)).length() < 1e-5 && (face - Vec2::NEG_X).length() < 1e-5);
    }
}
