//! The activity table's blocks, the game's (`ActivityTable`, its `PlayWithBlocks`, `Blocks` and
//! `KnockDown`): four places round the table, each with its own spot for a child and a nearer one
//! for a toddler (the table's `SeatDefinitions`). A toddler shakes and chews a block (the game's
//! `p2o_toyBlocks` clips); a child builds: a pile of blocks on the table before them, up to a
//! half-built structure and on to a whole one after six loops each (`kBuildLoopsToNextStage`),
//! the loops low, middling and high as it goes up, the block in hand; the structure as their
//! Blocks (a hidden skill) has it: a house at first, then a castle, a sphinx, a bridge or a tower
//! (`kBuildingsAtSkill0` to `4`), the blocks object's own geometry states. Another child can knock
//! a structure down ("Knock Down", the game's: greyed "wait for them to finish building" over a
//! pile): from beside it, the pack of clips for both (the builder's reaction), fun for the one,
//! the other less fond of them (`kKnockDownRelationshipDeltaSTC`), and the pile to build again.

use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills, Special};
use crate::sim::{Age, Sim};

pub struct BlocksPlugin;

impl Plugin for BlocksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Structures>().add_systems(Update, (blocks_test, build, knock_down).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The structures standing (not a pile): the table, the place, the child building it.
#[derive(Resource, Default)]
pub struct Structures(pub Vec<(Entity, usize, Entity)>);

/// The table's places (`SeatDefinitions`): which way out from its middle each is.
const SIDES: [Vec2; 4] = [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), Vec2::new(0.0, -1.0)];
/// How far out a child plays (`k_route_*`), a walking toddler (`k_route_*_toddlerWalk*`), the
/// spots beside a place a structure's knocked down from (`KnockDownRoutingSlots`), and where the
/// blocks stand on the table (`k_ctnm_0_blocksNPaper_*`).
const CHILD_AT: f32 = 0.867;
const TODDLER_AT: f32 = 0.53;
const KNOCK_ASIDE: f32 = 0.5;
const BLOCKS_AT: f32 = 0.275;
const BLOCKS_UP: f32 = 0.343;
/// Loops to the next stage (more than `kBuildLoopsToNextStage`), and how long one is (seconds,
/// the clips' own).
const LOOPS_TO_NEXT: f32 = 7.0;
const LOOP_SECONDS: f32 = 6.0;
/// When the blocks are put out (getting in, then `gather`'s `blocks_vis_on`) and put away
/// (`cleanUp`'s `blocks_vis_off`), and when a knock down scatters them and they're a pile again
/// (another's: `knockDown2`'s events; their own: `knockDown1`'s), seconds in.
const SHOWN_AT: f32 = 2.2 + 1.5;
const PUT_AWAY_AT: f32 = 0.73;
const KNOCKED: [(f32, f32); 2] = [(1.33, 2.87), (0.73, 1.7)];
/// The blocks object, its stages' geometry states, and the structures by Blocks level.
const BLOCKS_OBJECT: &str = "activityTableBlocks";
const PILE: &str = "pile";
const MID: &str = "midPointStructure";
const BUILDINGS: [&[&str]; 5] = [&["house"], &["house", "castle"], &["castle", "sphinx"], &["sphinx", "bridge"], &["castle", "bridge", "tower"]];
/// Liking the builder loses for the one knocking it down (`kKnockDownRelationshipDeltaSTC`, its
/// short-term part, as liking here), and the knocker's fun (`KnockDown`'s Fun, once).
const KNOCK_LIKING: f32 = -5.0;
const KNOCK_FUN: f32 = 20.0;

/// A child's loops as the structure goes up: low over a pile, middling half built, high whole.
pub const CHILD_LOOPS: [&[&str]; 3] = [
    &["c2o_toyBlocks_play_loopLow_x", "c2o_toyBlocks_play_loopLow_x", "c2o_toyBlocks_play_breathe_x"],
    &["c2o_toyBlocks_play_loopMed_x", "c2o_toyBlocks_play_loopMed_x", "c2o_toyBlocks_play_breathe_x"],
    &["c2o_toyBlocks_play_loopHigh_x", "c2o_toyBlocks_play_loopHigh_x", "c2o_toyBlocks_play_breathe_x"],
];

/// A child's play: in at the table, the blocks gathered, built with; at the end, cleared up and
/// up again. (`start` and the loops as the building goes, the end kept.)
pub fn child_clip(start: Option<&'static str>, stage: usize) -> ActionClip {
    ActionClip::new(start, CHILD_LOOPS[stage.min(2)]).ending(&["c2o_toyBlocks_play_cleanUp_x", "c2o_toyBlocks_getOut_x"], 4.8)
}

pub fn child_start() -> ActionClip {
    ActionClip { steps: &["c2o_toyBlocks_play_gather_x"], ..child_clip(Some("c2o_toyBlocks_getIn_x"), 0) }
}

/// A toddler's play: down to the table, a block shaken and chewed, up again.
pub fn toddler_clip() -> ActionClip {
    ActionClip::new(Some("p2o_toyBlocks_trans_stand2start_x"), &["p2o_toyBlocks_play_shakeR_x", "p2o_toyBlocks_play_shakeL_x", "p2o_toyBlocks_play_chew_x", "p2o_toyBlocks_play_breathe_x"])
        .ending(&["p2o_toyBlocks_trans_start2stand_x"], 1.5)
}

/// A place at a table standing so: where a child (or a toddler) plays, and the way they face.
pub fn place(otf: &Transform, i: usize, toddler: bool) -> (Vec2, Vec2) {
    let d = SIDES[i % 4];
    let at = d * if toddler { TODDLER_AT } else { CHILD_AT };
    let w = otf.transform_point(Vec3::new(at.x, 0.0, at.y)).xz();
    let face = (otf.rotation * Vec3::new(-d.x, 0.0, -d.y)).xz().normalize_or(Vec2::Y);
    (w, face)
}

/// The spots a place's structure is knocked down from, either side of it.
pub fn knock_spots(otf: &Transform, i: usize) -> [Vec2; 2] {
    let d = SIDES[i % 4];
    let side = Vec2::new(-d.y, d.x);
    [1.0, -1.0].map(|s| {
        let at = d * CHILD_AT + side * KNOCK_ASIDE * s;
        otf.transform_point(Vec3::new(at.x, 0.0, at.y)).xz()
    })
}

/// The knocking spot nearest a point: its place, which side (the builder's right, the game's
/// mirrored `IsKnockDownMirrored`), where it is and the way it faces (in at the blocks, slantwise).
pub fn knock_spot(otf: &Transform, near: Vec2) -> (usize, bool, Vec2, Vec2) {
    let (i, s, at) = (0..4).flat_map(|i| knock_spots(otf, i).into_iter().enumerate().map(move |(s, p)| (i, s, p))).min_by(|a, b| a.2.distance(near).total_cmp(&b.2.distance(near))).unwrap_or((0, 0, near));
    let d = SIDES[i];
    let side = Vec2::new(-d.y, d.x) * if s == 0 { 1.0 } else { -1.0 };
    let v = -(d + side);
    let face = (otf.rotation * Vec3::new(v.x, 0.0, v.y)).xz().normalize_or(Vec2::Y);
    (i, s == 1, at, face)
}

/// The place nearest where a Sim came to (or the knocking spot), and the way they face there.
pub fn nearest_place(otf: &Transform, came: Vec2, toddler: bool, knock: bool) -> (Vec2, Vec2) {
    if knock {
        let (_, _, at, face) = knock_spot(otf, came);
        return (at, face);
    }
    (0..4).map(|i| place(otf, i, toddler)).min_by(|a, b| a.0.distance(came).total_cmp(&b.0.distance(came))).unwrap_or((came, Vec2::Y))
}

/// The place nearest a point.
fn seat_of(otf: &Transform, p: Vec2) -> usize {
    (0..4).min_by(|a, b| place(otf, *a, false).0.distance(p).total_cmp(&place(otf, *b, false).0.distance(p))).unwrap_or(0)
}

/// Where a place's blocks stand on the table.
fn blocks_spot(otf: &Transform, i: usize) -> Vec3 {
    let d = SIDES[i % 4] * BLOCKS_AT;
    otf.transform_point(Vec3::new(d.x, BLOCKS_UP, d.y))
}

/// The structures a Blocks level builds.
pub fn buildings(level: u32) -> &'static [&'static str] {
    BUILDINGS[(level as usize).min(BUILDINGS.len() - 1)]
}

/// What's on the menu at a table for a Sim: blocks to play with for toddlers and children, a
/// structure to knock down for a child (greyed, "wait for them to build", while there's none).
pub fn offers(special: Special, sim: &Sim) -> bool {
    match special {
        Special::PlayBlocks => matches!(sim.age, Age::Toddler | Age::Child),
        Special::KnockDownBlocks => sim.age == Age::Child,
        _ => true,
    }
}

/// A child building at a place: the table, the place, the blocks on it, the stage (pile, half,
/// whole), the structure it's to be, the loops at this stage, how long they've played (seconds),
/// a knock down under way (another's or their own, how far in), how long since they stopped.
#[derive(Component)]
pub struct Building {
    table: Entity,
    seat: usize,
    blocks: Option<Entity>,
    stage: usize,
    target: &'static str,
    loops: f32,
    played: f32,
    knock: Option<(usize, f32)>,
    leaving: f32,
}

/// Toddlers and children at the blocks: a child's put out before them, built up a stage each
/// seven loops (pile, half built, whole: then proud, and after as long again knocked down by
/// their own hand and begun again), Blocks learned; cleared away when they're done.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    data: Option<Res<crate::baked::Baked>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut sims: Query<(Entity, &Sim, &ActionQueue, &Transform, &mut Skills, Option<&mut Building>)>,
    tables: Query<(&GameObject, &Transform), Without<Sim>>,
    mut shown: Query<&mut Visibility, Without<Sim>>,
    mut structures: ResMut<Structures>,
) {
    let Some(data) = data else { return };
    let objd = data.0.catalog.iter().find(|c| c.instance_name == BLOCKS_OBJECT).map(|c| c.objd);
    // (At the clips' pace.)
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let mut rng = rand::rng();
    let mut standing = Vec::new();
    // (The blocks in one of their states.)
    let mut parts_in = |state: &str| {
        let objd = objd?;
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
        Some(assets.object_state_design(&mut ctx, objd, Some(s3pkg::fnv32(state)), None))
    };

    let mut show = |blocks: Option<Entity>, on: bool| {
        if let Some(mut v) = blocks.and_then(|b| shown.get_mut(b).ok()) {
            let want = if on { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    };
    for (e, sim, queue, tf, mut skills, building) in &mut sims {
        let playing = queue.0.front().and_then(|a| match a.kind {
            ActionKind::Object { target, def } if matches!(a.phase, Phase::Running(_)) && !a.cancel => {
                let (o, otf) = tables.get(target).ok()?;
                (o.kind == ObjectKind::ActivityTable && crate::interact::interactions_for(o.kind).get(def)?.special == Special::PlayBlocks).then_some((target, *otf))
            }
            _ => None,
        });
        match (playing, building) {
            (Some((table, otf)), None) => {
                // (A child's blocks, a pile, put out before them as they gather them.)
                let seat = seat_of(&otf, tf.translation.xz());
                let target = buildings(skills.level("Blocks")).choose(&mut rng).copied().unwrap_or("house");
                let blocks = (sim.age == Age::Child).then(|| parts_in(PILE)).flatten().map(|parts| {
                    let at = blocks_spot(&otf, seat);
                    let yaw = otf.rotation * Quat::from_rotation_y(SIDES[seat].x.atan2(SIDES[seat].y));
                    let b = crate::objects::spawn_parts(&mut commands, &parts, Transform::from_translation(at).with_rotation(yaw));
                    commands.entity(b).insert(Visibility::Hidden);
                    b
                });
                info!("blocks: {} plays at place {seat} (building a {target})", sim.first);
                commands.entity(e).insert(Building { table, seat, blocks, stage: 0, target, loops: 0.0, played: 0.0, knock: None, leaving: 0.0 });
            }
            (Some(_), Some(mut b)) => {
                // (Blocks learned as they play, as any practice.)
                if delta.0 > 0.0 {
                    let points = delta.0 / 60.0 * crate::interact::practice_points("") * crate::life::skill_rate(&sim.traits, "Blocks");
                    skills.add_points("Blocks", points);
                }
                let Some(blocks) = b.blocks else { continue };
                b.leaving = 0.0;
                b.played += dt;
                // A knock down: the blocks scattered, a pile again.
                if let Some((who, t)) = b.knock {
                    let t = t + dt;
                    let (off, on) = KNOCKED[who];
                    if t >= off && b.stage > 0 {
                        b.stage = 0;
                        if let Some(parts) = parts_in(PILE) {
                            crate::objects::reshape_parts(&mut commands, blocks, &parts);
                        }
                    }
                    show(Some(blocks), t < off || t >= on);
                    b.knock = (t < on).then_some((who, t));
                    if t >= on {
                        b.loops = 0.0;
                    }
                    continue;
                }
                show(Some(blocks), b.played >= SHOWN_AT);
                if b.played < SHOWN_AT {
                    continue;
                }
                b.loops += dt / LOOP_SECONDS;
                if b.loops >= LOOPS_TO_NEXT {
                    b.loops = 0.0;
                    if b.stage < 2 {
                        b.stage += 1;
                        let state = if b.stage == 1 { MID } else { b.target };
                        if let Some(parts) = parts_in(state) {
                            crate::objects::reshape_parts(&mut commands, blocks, &parts);
                        }
                        // (Done: proud of it, `BeProud`.)
                        let start = (b.stage == 2).then_some("c2o_toyBlocks_play_react_x");
                        commands.entity(e).insert(child_clip(start, b.stage));
                        debug!("blocks: {}'s blocks now {state}", sim.first);
                    } else {
                        // (Theirs knocked down by their own hand, and the next one begun.)
                        b.target = buildings(skills.level("Blocks")).choose(&mut rng).copied().unwrap_or("house");
                        b.knock = Some((1, 0.0));
                        commands.entity(e).insert(child_clip(Some("c2o_toyBlocks_play_knockDown1_x"), 0));
                        debug!("blocks: {} knocks their own down (next a {})", sim.first, b.target);
                    }
                }
                if b.stage > 0 {
                    standing.push((b.table, b.seat, e));
                }
            }
            (None, Some(mut b)) => {
                // (Cleared away as they clean up.)
                b.leaving += dt;
                if b.leaving >= PUT_AWAY_AT || b.blocks.is_none() {
                    if let Some(blocks) = b.blocks {
                        commands.entity(blocks).try_despawn();
                    }
                    commands.entity(e).remove::<Building>();
                }
            }
            (None, None) => {}
        }
    }
    if structures.0 != standing {
        structures.0 = standing;
    }
}

/// A child knocking a structure down: from beside it, mirrored on the builder's right, the builder
/// reacting; fun for the one, the other less fond of them, the pile to build again.
#[allow(clippy::type_complexity)]
fn knock_down(
    mut commands: Commands,
    mut knockers: Query<(Entity, &Sim, &ActionQueue, &Transform, &mut crate::sim::Motives), Without<Building>>,
    mut builders: Query<(Entity, &Sim, &mut Building, &mut crate::social::Relationships)>,
    tables: Query<(&GameObject, &Transform), Without<Sim>>,
    mut done: Local<Vec<Entity>>,
) {
    let running = |q: &ActionQueue| q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Object { .. }) && matches!(a.phase, Phase::Running(_)));
    done.retain(|e| knockers.get(*e).is_ok_and(|(_, _, q, ..)| running(q)));
    for (e, sim, queue, tf, mut motives) in &mut knockers {
        if done.contains(&e) {
            continue;
        }
        let Some((table, otf)) = queue.0.front().and_then(|a| match a.kind {
            ActionKind::Object { target, def } if matches!(a.phase, Phase::Running(_)) => {
                let (o, otf) = tables.get(target).ok()?;
                (o.kind == ObjectKind::ActivityTable && crate::interact::interactions_for(o.kind).get(def)?.special == Special::KnockDownBlocks).then_some((target, *otf))
            }
            _ => None,
        }) else {
            continue;
        };
        done.push(e);
        // (The structure they're beside, at its place.)
        let (seat, mirrored, ..) = knock_spot(&otf, tf.translation.xz());
        let near = |b: &Building| if b.seat == seat { 0 } else { 1 };
        let Some((builder, b_sim, mut b, mut rels)) = builders.iter_mut().filter(|(_, _, b, _)| b.table == table && b.stage > 0 && b.knock.is_none()).min_by_key(|(_, _, b, _)| near(b)) else {
            continue;
        };
        info!("blocks: {} knocks down {}'s {}", sim.first, b_sim.first, if b.stage == 2 { b.target } else { MID });
        b.knock = Some((0, 0.0));
        let clip = ActionClip::new(None, &["c2o_toyBlocks_knockDown2_y"]);
        commands.entity(e).insert(if mirrored { clip.mirrored() } else { clip });
        commands.entity(builder).insert(child_clip(Some("c2o_toyBlocks_knockDown2_react_x"), 0));
        motives.add(crate::sim::FUN, KNOCK_FUN);
        rels.add(e, KNOCK_LIKING, 0.0);
    }
}
/// `BLOCKS_TEST=1` (tests): ten seconds in, an activity table put down a few metres from the
/// household's first child (or toddler), who plays with its blocks, the camera on it
/// (`BLOCKS_DIST` away); a second child, if there is one, knocks the structure down once it's
/// built (`BLOCKS_KNOCK=<seconds>` in).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn blocks_test(
    mut commands: Commands,
    time: Res<Time>,
    mut stage: Local<(u8, Option<Entity>, Option<Entity>)>,
    mut kids: Query<(Entity, &Sim, &Transform, &mut ActionQueue, Has<crate::sim::Selected>), With<crate::sim::HouseholdMember>>,
    tables: Query<(Entity, &GameObject, &Transform)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if std::env::var_os("BLOCKS_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let young: Vec<(Entity, Vec3, Age, bool)> = kids.iter().filter(|(_, s, ..)| matches!(s.age, Age::Child | Age::Toddler)).map(|(e, s, tf, _, sel)| (e, tf.translation, s.age, sel)).collect();
    // (The same one throughout, the selected one if young: the query's order changes as they
    // start playing.)
    let Some(&(first, at, ..)) = stage.2.and_then(|p| young.iter().find(|k| k.0 == p)).or(young.iter().find(|k| k.3)).or(young.iter().find(|k| k.2 == Age::Child)).or(young.first()) else { return };
    stage.2 = Some(first);
    match stage.0 {
        0 if now > 10.0 => {
            if let Some(key) = data.0.catalog.iter().find(|c| c.instance_name == "activityTableTurtle").map(|c| c.objd) {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                let p = at + Vec3::new(2.5, 0.0, 0.0);
                crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0);
                info!("blocks test: a table at {p}");
            }
            stage.0 = 1;
        }
        1 => {
            let Some((t, ..)) = tables.iter().find(|(_, o, _)| o.kind == ObjectKind::ActivityTable) else { return };
            if let Ok((_, s, _, mut q, _)) = kids.get_mut(first) {
                info!("blocks test: {} plays", s.first);
                q.push_player(crate::interact::Action::new("Play with Blocks", ActionKind::Object { target: t, def: 0 }, false));
            }
            (stage.0, stage.1) = (2, Some(t));
        }
        2 => {
            let knock_at = std::env::var("BLOCKS_KNOCK").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(f32::MAX);
            if let Some(t) = stage.1
                && now > knock_at
                && let Some(&(other, ..)) = young.iter().find(|k| k.0 != first && k.2 == Age::Child)
                && let Ok((_, s, _, mut q, _)) = kids.get_mut(other)
            {
                info!("blocks test: {} knocks it down", s.first);
                q.push_player(crate::interact::Action::new("Knock Down", ActionKind::Object { target: t, def: 1 }, false));
                stage.0 = 3;
            }
        }
        _ => {}
    }
    if let (Ok(mut c), Some(t)) = (cams.single_mut(), stage.1)
        && let Ok((_, _, ttf)) = tables.get(t)
    {
        c.look_at(ttf.translation);
        c.distance = std::env::var("BLOCKS_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(5.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_places_face_it() {
        let otf = Transform::from_xyz(10.0, 0.0, 5.0);
        let (at, face) = place(&otf, 1, false);
        assert!((at - Vec2::new(10.0, 5.867)).length() < 1e-4);
        assert!((face - Vec2::new(0.0, -1.0)).length() < 1e-4, "facing the table");
        let (at, _) = place(&otf, 0, true);
        assert!((at - Vec2::new(10.53, 5.0)).length() < 1e-4, "a toddler nearer in");
        let k = knock_spots(&otf, 1);
        assert!(k.iter().all(|p| (p.y - 5.867).abs() < 1e-4 && (p.x - 10.0).abs() > 0.49));
        assert_eq!(seat_of(&otf, Vec2::new(9.0, 5.0)), 2);
    }

    #[test]
    fn structures_grow_with_the_childs_blocks() {
        assert_eq!(buildings(0), &["house"]);
        assert!(buildings(4).contains(&"tower"));
        assert_eq!(buildings(9), buildings(4));
    }
}
