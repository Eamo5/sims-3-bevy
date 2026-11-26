//! Lawn flamingos and garden gnomes, as the game's (`Flamingo`, `GardenGnome`, the magic gnomes
//! with them): a Sim "Kick"s one over (the gnome's "Kick", a magic one's "Kick {name}"), from
//! whichever of its two sides they come to (its `_route_0_W` and `_E`, the far one the game's
//! mirrored clips), with the game's kick (`a2o_flamingo_kick`, a child's `c2o_`, its own whoosh
//! and voice), the ornament toppling as its half of the clip has it and left lying there
//! (`floor_pose`), worth nothing to the decor till it's stood up again ("Stand Up", "Stand {name}
//! Up": `a2o_flamingo_standup`). A flamingo kicked has those about react at the kick
//! (`trigger_reactions_for_nearby_sims`): an awkward look from four in five where the one kicking
//! lives there (`kChanceOfAwkwardReaction`), and a boo from those whose home it is when anyone else
//! kicks it; a gnome kicked shocks those about (its `OneShotReaction`). An angry Sim may kick one
//! of their own accord (the kick's `MoodAngry`), a tidy one stands one up (`Dirtiness`). Kept in
//! saves.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase};
use crate::sim::{Age, Pose, Sim, SimAnim};

pub struct OrnamentsPlugin;

impl Plugin for OrnamentsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (run, autonomy, ornaments_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, pose.after(crate::anim::drive_skeletons).run_if(in_state(crate::AppState::InGame)));
    }
}

/// The game's words.
pub const KICK: &str = "Kick";
pub const STAND_UP: &str = "Stand Up";

/// The game's tuning: how far off its sides are (its routing slots), when the kick lands (the
/// clip's reaction event, seconds), how far off those who react are, how likely those whose home
/// it is are to look awkward at their own kicking it.
const SIDE: f32 = 0.4;
const KICK_LANDS: f32 = 1.333;
const REACT_WITHIN: f32 = 10.0;
const AWKWARD_CHANCE: f64 = 0.8;

/// A flamingo or a gnome (a magic one by its name in the words: the game's `MagicGnome`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ornament {
    Flamingo,
    Gnome,
    MagicGnome,
}

impl Ornament {
    pub fn of(script: &str) -> Ornament {
        let s = script.to_ascii_lowercase();
        if s.contains("flamingo") {
            Ornament::Flamingo
        } else if s.contains("magicgnome") || s.contains("hauntedgnome") || s.contains("graduationgnome") || s.contains("divergnome") {
            Ornament::MagicGnome
        } else {
            Ornament::Gnome
        }
    }

    /// Its kick's words, and its standing up's (`KickGnome:InteractionName`...).
    pub fn kick(self, name: &str) -> String {
        if self == Ornament::MagicGnome { format!("Kick {name}") } else { KICK.to_string() }
    }

    pub fn stand_up(self, name: &str) -> String {
        if self == Ornament::MagicGnome { format!("Stand {name} Up") } else { STAND_UP.to_string() }
    }
}

/// An ornament knocked over: from which side (the mirrored clips' far one).
#[derive(Component, Clone, Copy, Debug)]
pub struct KnockedOver {
    pub mirrored: bool,
}

/// An ornament posed as its half of a clip has it (seconds in; held at its end).
#[derive(Component, Clone, Copy, Debug)]
struct Posed {
    clip: &'static str,
    t: f32,
}

/// The clips, a Sim's and the ornament's, for kicking it over or standing it up, from which side,
/// by an adult or a child.
pub fn clips(kick: bool, mirrored: bool, child: bool) -> (&'static str, &'static str) {
    match (kick, mirrored, child) {
        (true, false, false) => ("a2o_flamingo_kick_x", "a2o_flamingo_kick_flamingo"),
        (true, true, false) => ("a2o_flamingo_kick_mirrored_x", "a2o_flamingo_kick_mirrored_flamingo"),
        (true, false, true) => ("c2o_flamingo_kick_x", "c2o_flamingo_kick_flamingo"),
        (true, true, true) => ("c2o_flamingo_kick_mirrored_x", "c2o_flamingo_kick_mirrored_flamingo"),
        (false, false, false) => ("a2o_flamingo_standup_x", "a2o_flamingo_standup_flamingo"),
        (false, true, false) => ("a2o_flamingo_standup_mirrored_x", "a2o_flamingo_standup_mirrored_flamingo"),
        (false, false, true) => ("c2o_flamingo_standup_x", "c2o_flamingo_standup_flamingo"),
        (false, true, true) => ("c2o_flamingo_standup_mirrored_x", "c2o_flamingo_standup_mirrored_flamingo"),
    }
}

/// Lying there, as its clip leaves it.
fn floor_pose(mirrored: bool) -> &'static str {
    if mirrored { "a2o_flamingo_floor_pose_mirrored_flamingo" } else { "a2o_flamingo_floor_pose_flamingo" }
}

/// Where a Sim stands at a side, and the way they face (`_route_0_W` behind it facing it; `_E`
/// before it, mirrored).
pub fn side(tf: &Transform, mirrored: bool) -> (Vec2, Vec2) {
    let z = if mirrored { SIDE } else { -SIDE };
    let at = tf.transform_point(Vec3::new(0.0, 0.0, z)).xz();
    (at, (tf.translation.xz() - at).normalize_or(Vec2::Y))
}

/// The action for kicking it or standing it up: from the side nearer them (standing up, the side
/// it fell from).
pub fn act(thing: Entity, tf: &Transform, kick: bool, knocked: Option<KnockedOver>, here: Vec2) -> ActionKind {
    let mirrored = match knocked {
        Some(k) => k.mirrored,
        None => side(tf, true).0.distance(here) < side(tf, false).0.distance(here),
    };
    let (at, face) = side(tf, mirrored);
    ActionKind::Ornament { thing, kick, mirrored, at, face }
}

/// Its menu: kicked over standing, stood up lying (children and older: the tuning's `C,T,Y,A,E`).
pub fn options(thing: Entity, tf: &Transform, what: Ornament, name: &str, knocked: Option<KnockedOver>, age: Age, here: Vec2) -> Vec<(String, ActionKind)> {
    if age.is_little() {
        return Vec::new();
    }
    match knocked {
        None => vec![(what.kick(name), act(thing, tf, true, None, here))],
        Some(k) => vec![(what.stand_up(name), act(thing, tf, false, Some(k), here))],
    }
}

/// Kicking one or standing it up: how far through, how long, whether the kick's landed.
#[derive(Component)]
struct AtOrnament {
    t: f32,
    len: f32,
    landed: bool,
}

/// Kicks and standings up played out; those about react to a kick.
#[allow(clippy::type_complexity)]
fn run(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Transform, &mut SimAnim, Option<&mut AtOrnament>, Option<&crate::visit::OnLot>, Has<crate::sim::HouseholdMember>), Without<GameObject>>,
    things: Query<(&GameObject, &Transform, Option<&crate::visit::LotObject>), Without<Sim>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let now = clock.minutes;
    let mut rng = rand::rng();
    // (Who kicked what where: the lot it's on, whether it's theirs, what it is, where.)
    let mut kicks: Vec<(Entity, Option<usize>, bool, Ornament, Vec3)> = Vec::new();
    for (e, sim, mut q, mut tf, mut anim, at, _, member) in &mut sims {
        let front = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match a.kind {
            ActionKind::Ornament { thing, kick, mirrored, at, face } => Some((thing, kick, mirrored, at, face)),
            _ => None,
        });
        let Some((thing, kick, mirrored, spot, face)) = front else {
            if at.is_some() {
                commands.entity(e).remove::<(AtOrnament, ActionClip)>();
            }
            continue;
        };
        let Ok((obj, otf, obj_lot)) = things.get(thing) else {
            q.0.pop_front();
            continue;
        };
        let (sim_clip, its_clip) = clips(kick, mirrored, sim.age == Age::Child);
        let Some(mut a) = at else {
            // (Right at its side, facing it, as the clips have them.)
            tf.translation.x = spot.x;
            tf.translation.z = spot.y;
            tf.rotation = Quat::from_rotation_y(face.x.atan2(face.y));
            anim.pose = Pose::Use;
            let len = lib.get(&data, sim_clip).map_or(3.37, |c| c.duration);
            commands.entity(e).insert((AtOrnament { t: 0.0, len, landed: false }, ActionClip::new(Some(sim_clip), &[])));
            commands.entity(thing).remove::<crate::objanim::HeldPose>().insert((Posed { clip: its_clip, t: 0.0 }, crate::objanim::PlayingClip { name: its_clip, time: 0.0, looping: false }));
            if kick {
                commands.entity(thing).insert(KnockedOver { mirrored });
            } else {
                commands.entity(thing).remove::<KnockedOver>();
            }
            info!("ornaments: {} {} {}", sim.first, if kick { "kicks" } else { "stands up" }, obj.name);
            continue;
        };
        a.t += dt;
        commands.entity(thing).insert((Posed { clip: its_clip, t: a.t }, crate::objanim::PlayingClip { name: its_clip, time: a.t, looping: false }));
        if kick && !a.landed && a.t >= KICK_LANDS {
            a.landed = true;
            // (On the household's own lot, kicked by one of them: their own.)
            let here_lot = obj_lot.map(|l| l.0);
            let what = data.0.catalog.iter().find(|c| c.objd == obj.objd).map_or(Ornament::Gnome, |c| Ornament::of(&c.script));
            kicks.push((e, here_lot, member && here_lot.is_none(), what, otf.translation));
        }
        if a.t >= a.len {
            q.0.pop_front();
            anim.pose = Pose::Stand;
            commands.entity(e).remove::<(AtOrnament, ActionClip)>();
            commands.entity(thing).remove::<crate::objanim::PlayingClip>();
            if kick {
                commands.entity(thing).insert((Posed { clip: floor_pose(mirrored), t: 1.0 }, crate::objanim::HeldPose(floor_pose(mirrored))));
            } else {
                commands.entity(thing).remove::<(Posed, crate::objanim::HeldPose)>();
                commands.entity(thing).queue_silenced(|mut w: EntityWorldMut| rest(&mut w));
            }
        }
    }
    // (Those about, at the kick: a flamingo's awkward looks and boos; a gnome's shock.)
    for (kicker, lot, kicker_home, what, at) in kicks {
        for (o, _, _, otf, _, _, olot, omember) in sims.iter() {
            if o == kicker || otf.translation.distance(at) > REACT_WITHIN || olot.map(|l| l.0) != lot {
                continue;
            }
            // (Whose home the lot is: the household's, those of it.)
            let their_home = omember && lot.is_none();
            if let Some(kinds) = reaction(what, kicker_home, their_home, rng.random_bool(AWKWARD_CHANCE)) {
                commands.entity(o).insert(crate::reactions::Pending::new(kinds, Some(at), now + 10.0));
            }
        }
    }
}

/// How one about reacts to an ornament kicked (a flamingo's `ReactToKickedFlamingo`, a gnome's
/// `OneShotReaction`): the kicker's own (and an awkward look in the cards), or theirs.
pub fn reaction(what: Ornament, kicker_home: bool, their_home: bool, awkward: bool) -> Option<&'static [&'static str]> {
    match what {
        Ornament::Flamingo if kicker_home => awkward.then_some(&["Awkward"][..]),
        Ornament::Flamingo if their_home => Some(&["Boo"]),
        Ornament::Flamingo => None,
        _ => Some(&["Shocked"]),
    }
}

/// Stood up again there and then, with nobody at it (a magic gnome's own doing: `gnometricks`).
pub fn stand_up(commands: &mut Commands, thing: Entity) {
    commands.entity(thing).remove::<(KnockedOver, Posed, crate::objanim::HeldPose, crate::objanim::PlayingClip)>().queue_silenced(|mut w: EntityWorldMut| rest(&mut w));
}

/// Back as it was made, standing.
fn rest(w: &mut EntityWorldMut) {
    let kids: Vec<Entity> = w.get::<Children>().map(|c| c.iter().collect()).unwrap_or_default();
    w.world_scope(|world| {
        for k in kids {
            if world.get::<Mesh3d>(k).is_some()
                && let Some(mut t) = world.get_mut::<Transform>(k)
            {
                *t = Transform::IDENTITY;
            }
        }
    });
}

/// Ornaments posed as their clips have them: the whole of it moved as its `transformBone` (it's
/// all one piece), its meshes with it.
#[allow(clippy::type_complexity)]
fn pose(
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    things: Query<(&Posed, &Children), Changed<Posed>>,
    mut meshes: Query<&mut Transform, With<Mesh3d>>,
    knocked: Query<(Entity, &KnockedOver, &Children), (Without<Posed>, Added<KnockedOver>)>,
    mut commands: Commands,
) {
    let bone = s3pkg::fnv32("transformBone");
    for (p, kids) in &things {
        let Some(clip) = lib.get(&data, p.clip) else { continue };
        let Some(track) = clip.tracks.get(&bone) else { continue };
        let t = p.t.clamp(0.0, clip.duration.max(0.0));
        let at = crate::anim::sample_track_vec(&track.translation, t).unwrap_or_default();
        let turn = crate::anim::sample_track_quat(&track.rotation, t).unwrap_or_default();
        for k in kids.iter() {
            if let Ok(mut tf) = meshes.get_mut(k) {
                *tf = Transform::from_translation(at).with_rotation(turn);
            }
        }
    }
    // (Loaded lying down: as it was left.)
    for (e, k, _) in &knocked {
        commands.entity(e).insert((Posed { clip: floor_pose(k.mirrored), t: 1.0 }, crate::objanim::HeldPose(floor_pose(k.mirrored))));
    }
}

/// Angry Sims of the household kick an ornament about now and then; a Neat one stands one up.
#[allow(clippy::type_complexity)]
fn autonomy(
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(&Sim, &mut ActionQueue, &Transform, &crate::life::Mood), (With<crate::sim::HouseholdMember>, Without<crate::sim::Selected>)>,
    things: Query<(Entity, &GameObject, &Transform, Option<&KnockedOver>), Without<Sim>>,
    mut next: Local<f64>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + 60.0;
    if std::env::var_os("ORNAMENTS_TEST").is_some() {
        return;
    }
    let mut rng = rand::rng();
    for (sim, mut q, tf, mood) in &mut sims {
        if sim.age.is_little() || !q.0.is_empty() {
            continue;
        }
        let here = tf.translation.xz();
        let near = |knocked: bool| {
            things
                .iter()
                .filter(|(_, o, t, k)| o.kind == ObjectKind::LawnOrnament && k.is_some() == knocked && t.translation.xz().distance(here) < 15.0)
                .min_by(|a, b| a.2.translation.xz().distance(here).total_cmp(&b.2.translation.xz().distance(here)))
        };
        let angry = mood.0 < -40.0 && sim.traits.iter().any(|t| matches!(t, crate::life::Trait::HotHeaded | crate::life::Trait::MeanSpirited | crate::life::Trait::Evil | crate::life::Trait::Grumpy));
        if angry
            && rng.random_bool(0.3)
            && let Some((e, o, t, _)) = near(false)
        {
            q.0.push_back(crate::interact::Action::new(KICK, act(e, t, true, None, here), true));
            info!("ornaments: {} kicks {} in a temper", sim.first, o.name);
        } else if sim.traits.contains(&crate::life::Trait::Neat)
            && rng.random_bool(0.5)
            && let Some((e, o, t, k)) = near(true)
        {
            q.0.push_back(crate::interact::Action::new(STAND_UP, act(e, t, false, k.copied(), here), true));
            info!("ornaments: {} stands {} up", sim.first, o.name);
        }
    }
}

/// `ORNAMENTS_TEST=<instance>` (tests: `Flamingo`, `GardenGnome`, `MagicGnome`): ten seconds in,
/// one put down near the selected Sim (or `ORNAMENTS_WHO=<first name>`), who kicks it over and, after
/// `ORNAMENTS_AFTER=<seconds>` (8), stands it up; `ORNAMENTS_PIE=1`: its menu.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn ornaments_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>), With<crate::sim::HouseholdMember>>,
    things: Query<(Entity, &GameObject, &Transform, Option<&KnockedOver>), (Without<Sim>, With<crate::save::Bought>)>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("ORNAMENTS_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let who = env("ORNAMENTS_WHO");
    let is_me = |s: &Sim, sel: bool| who.as_ref().map_or(sel, |w| s.first.eq_ignore_ascii_case(w));
    let thing = things.iter().find(|t| t.1.kind == ObjectKind::LawnOrnament);
    if let (Ok(mut c), Some((_, _, ttf, _))) = (cams.single_mut(), thing) {
        let f = |k: &str, d: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(ttf.translation);
        c.height_offset = f("ORNAMENTS_LOOK_Y", 0.6);
        c.distance = f("ORNAMENTS_DIST", 4.0);
        c.pitch = f("ORNAMENTS_PITCH", 0.35);
        let fwd = ttf.rotation * Vec3::X;
        c.yaw = fwd.x.atan2(fwd.z) + f("ORNAMENTS_YAW", 0.4);
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            let Some((_, _, tf, ..)) = sims.iter().find(|s| is_me(s.1, s.4)) else { return };
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&name)).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            let spot = (3..14).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| (-2..=2).all(|i| (-2..=2).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.4)))).unwrap_or(here + Vec2::X * 2.0);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, tf.translation.y, spot.y), 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                info!("ornaments test: {name} put down");
            }
        }
        1 | 2 if now - stage.1 > if stage.0 == 1 { 2.0 } else { env("ORNAMENTS_AFTER").and_then(|v| v.parse().ok()).unwrap_or(8.0) } => {
            let Some((t, o, ttf, k)) = thing else { return };
            if env("ORNAMENTS_PIE").is_some() && stage.0 == 1 {
                commands.insert_resource(crate::hud::PieFor(t));
                *stage = (9, now);
                return;
            }
            let kick = stage.0 == 1;
            *stage = (stage.0 + 1, now);
            if let Some((_, sim, tf, mut q, _)) = sims.iter_mut().find(|s| is_me(s.1, s.4)) {
                let label = if kick { KICK } else { STAND_UP };
                q.push_player(crate::interact::Action::new(label, act(t, ttf, kick, k.copied(), tf.translation.xz()), false));
                info!("ornaments test: {} to {} {}", sim.first, label, o.name);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ornaments_are_kicked_and_stood_up_as_the_game_has_them() {
        assert_eq!(Ornament::of("Sims3.Gameplay.Objects.Decorations.Flamingo"), Ornament::Flamingo);
        assert_eq!(Ornament::of("Sims3.Gameplay.Objects.Environment.GardenGnome"), Ornament::Gnome);
        assert_eq!(Ornament::of("Sims3.Gameplay.Objects.Environment.MagicGnomeEgypt"), Ornament::MagicGnome);
        assert_eq!(Ornament::Flamingo.kick("Shocking Pink Flamingo"), "Kick");
        assert_eq!(Ornament::MagicGnome.stand_up("Mysterious Mr. Gnome"), "Stand Mysterious Mr. Gnome Up");
        assert_eq!(clips(true, true, true), ("c2o_flamingo_kick_mirrored_x", "c2o_flamingo_kick_mirrored_flamingo"));
        assert_eq!(clips(false, false, false).1, "a2o_flamingo_standup_flamingo");
        // (Behind it facing it, or before it, mirrored; the nearer side.)
        let tf = Transform::from_xyz(5.0, 0.0, 5.0);
        let (w, face) = side(&tf, false);
        assert!((w - Vec2::new(5.0, 4.6)).length() < 1e-5 && (face - Vec2::Y).length() < 1e-5);
        let e = Entity::from_raw_u32(3).unwrap();
        assert!(matches!(act(e, &tf, true, None, Vec2::new(5.0, 9.0)), ActionKind::Ornament { mirrored: true, .. }));
        assert!(matches!(act(e, &tf, false, Some(KnockedOver { mirrored: false }), Vec2::new(5.0, 9.0)), ActionKind::Ornament { mirrored: false, .. }), "stood up from where it fell");
        assert_eq!(s3pkg::fnv32("transformBone"), 0xCD68_F001);
    }

    #[test]
    fn those_about_react_as_the_game_has_them() {
        assert_eq!(reaction(Ornament::Flamingo, true, true, true), Some(&["Awkward"][..]), "the family kicking their own");
        assert_eq!(reaction(Ornament::Flamingo, true, true, false), None);
        assert_eq!(reaction(Ornament::Flamingo, false, true, true), Some(&["Boo"][..]), "someone else kicking theirs");
        assert_eq!(reaction(Ornament::Flamingo, false, false, true), None);
        assert_eq!(reaction(Ornament::MagicGnome, false, false, false), Some(&["Shocked"][..]));
    }
}
