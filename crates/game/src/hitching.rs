//! Hitching posts, the Pets pack's (`HitchingPost`): each has two places for a horse
//! (`HitchingPostPart`, left and right), a hay bale and a water trough before it. A Sim up on a
//! horse can "Tie Up" at one (ridden to it, off, and the horse tied there, a rope from its halter
//! to the post), and a Sim on foot can "Tie Horse to Post" one of the horses about (called over
//! and tied); "Untie" from the horse's menu, or "Untie Hitched Horse" from the post's, frees it
//! again. A tied horse stays put, eating the post's hay and drinking from its trough when it's
//! hungry or thirsty (the game's `EatHay` and `Drink` there); wanting something the post can't
//! give it (sleep, the bathroom, company, a run), it breaks the rope and is off
//! (`BreakRope`, `ah_hitchingPost_breakRope`, the game's way out of being tied).

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{ActionKind, GameObject, ObjectKind};
use crate::petneeds::{BLADDER, ENERGY, HUNGER, PetMotives, SOCIAL, THIRST, URGE};
use crate::pets::{HomePet, Pet, PetAnim};

pub struct HitchingPlugin;

impl Plugin for HitchingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (hitching_test, called, tie_arrivals, tied_horses, ropes).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The post's parts' slots (model space, the same on all three posts): where a horse stands
/// facing the post (`kLeftHorseRoutingSlot`...), where a Sim stands to tie or untie it
/// (`kLeftTieUpRoutingSlot`...), and where the rope's tied (`kLeftFXSlot`...).
pub const HORSE_SLOTS: [Vec2; 2] = [Vec2::new(-0.585, 1.529), Vec2::new(0.640, 1.529)];
pub const TIE_SLOTS: [Vec2; 2] = [Vec2::new(-1.75, 0.75), Vec2::new(1.75, 0.75)];
pub const ROPE_SLOTS: [Vec3; 2] = [Vec3::new(-1.1, 1.507, -0.163), Vec3::new(1.1, 1.507, -0.163)];

/// The game's rates at the post (as at a box stall's hay and a water trough: an hour's worth).
const HAY_RATE: f32 = 100.0;
const DRINK_RATE: f32 = 250.0;
/// When a tied horse eats or drinks (its need under this), and stops (this full).
const PECKISH: f32 = -20.0;
const FULL: f32 = 95.0;
/// What a tied horse can't bear, the rope broken for it.
const BREAK_AT: f32 = -60.0;
/// How long the rope's breaking takes, if the clip isn't to hand (seconds).
const BREAK_SECS: f32 = 3.0;
pub const BREAK_CLIP: &str = "ah_hitchingPost_breakRope_x";
/// The game's names.
pub const TIE_UP: &str = "Tie Up";
pub const TIE_TO_POST: &str = "Tie Horse to Post";
pub const UNTIE: &str = "Untie";
pub const UNTIE_HITCHED: &str = "Untie Hitched Horse";
pub const POST_FULL: &str = "All available spaces on this hitching post are occupied.";
pub const ALL_IN_USE: &str = "All available horses are being used at the moment.";

/// A horse tied to a post: which, at which of its places; what it's at there (hay, water, the
/// rope breaking), and its rope.
#[derive(Component)]
pub struct Tied {
    pub post: Entity,
    pub part: usize,
    doing: Option<(usize, f32)>,
    breaking: Option<f32>,
    rope: Option<Entity>,
}

impl Tied {
    pub fn new(post: Entity, part: usize) -> Self {
        Self { post, part, doing: None, breaking: None, rope: None }
    }
}

/// A Sim's call to a horse to come over to the post (`social_callOver`), and their hand at its
/// rope to untie it; how long each takes (game minutes).
pub const CALL_CLIP: &str = "a_soc_callOver_x";
pub const UNTIE_CLIP: &str = "a2o_object_genericSwipe_x";
pub const CALL_MINUTES: f32 = 3.0;
pub const UNTIE_MINUTES: f32 = 2.0;

/// A horse called over to a post by the Sim tying it up.
#[derive(Component)]
pub struct CalledToPost {
    pub post: Entity,
    pub part: usize,
}

/// Untied by a Sim, when the command's seen to.
pub fn untie_later(commands: &mut Commands, horse: Entity) {
    commands.queue(move |w: &mut World| {
        let Ok(mut e) = w.get_entity_mut(horse) else { return };
        let rope = e.take::<Tied>().and_then(|t| t.rope);
        e.remove::<crate::petlitters::Held>();
        if let Some(r) = rope
            && let Ok(r) = w.get_entity_mut(r)
        {
            r.despawn();
        }
        info!("hitching: untied");
    });
}

/// Called over: off to its place at the post (it's tied once it's there).
#[allow(clippy::type_complexity)]
fn called(
    mut commands: Commands,
    horses: Query<(Entity, &Pet, &Transform, &CalledToPost)>,
    posts: Query<&Transform, (With<GameObject>, Without<Pet>)>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let Some(grid) = grid else { return };
    let mut rng = rand::rng();
    for (e, pet, tf, c) in &horses {
        commands.entity(e).remove::<CalledToPost>();
        let Ok(ptf) = posts.get(c.post) else { continue };
        let (at, _) = horse_spot(ptf, c.part);
        match crate::petneeds::plan(pet, tf.translation.xz(), crate::petneeds::PetDo::Go, Some(at), None, &grid, building.as_deref(), &mut rng) {
            Some(task) => {
                commands.entity(e).remove::<(crate::petcompany::Greeting, crate::petcompany::Greeted, crate::petlitters::Held)>().insert((task, TieWhenThere { post: c.post, part: c.part }));
            }
            None => notes.push(format!("{} can't get there.", pet.name)),
        }
    }
}

/// A horse on its way to be tied (called over to the post).
#[derive(Component)]
pub struct TieWhenThere {
    pub post: Entity,
    pub part: usize,
}

/// Where a horse stands at a post's part (world), and the way it faces (at the post).
pub fn horse_spot(post: &Transform, part: usize) -> (Vec2, Quat) {
    let s = HORSE_SLOTS[part.min(1)];
    (post.transform_point(Vec3::new(s.x, 0.0, s.y)).xz(), post.rotation * Quat::from_rotation_y(std::f32::consts::PI))
}

/// Where a Sim stands to tie or untie a horse there (world), and the way they face (the post).
pub fn tie_spot(post: &Transform, part: usize) -> (Vec2, Vec2) {
    let s = TIE_SLOTS[part.min(1)];
    let at = post.transform_point(Vec3::new(s.x, 0.0, s.y)).xz();
    (at, (post.translation.xz() - at).normalize_or(Vec2::Y))
}

/// A free place at a post, of the parts taken.
pub fn free_part(taken: &[usize]) -> Option<usize> {
    (0..2).find(|p| !taken.contains(p))
}

/// What a tied horse does next, by its needs: eat (hunger), drink (thirst), break free for what
/// the post can't give it, or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtPost {
    Eat,
    Drink,
    Break,
    Stand,
}

pub fn at_post(m: &PetMotives, grown: bool) -> AtPost {
    if [BLADDER, ENERGY, SOCIAL, URGE].iter().any(|i| m.0[*i] < BREAK_AT) {
        return AtPost::Break;
    }
    if m.0[HUNGER] < PECKISH {
        AtPost::Eat
    } else if grown && m.0[THIRST] < PECKISH {
        AtPost::Drink
    } else {
        AtPost::Stand
    }
}

/// A post's menu for a Sim: up on a horse, "Tie Up" (greyed with both places taken); on foot,
/// "Tie Horse to Post" with the horses about that are free, and "Untie Hitched Horse" with those
/// tied there.
pub fn post_options(post: Entity, part: usize, riding: bool, taken: usize, free_horses: &[(Entity, String)], tied_here: &[(Entity, String, Vec2)], pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>, submenu: impl Fn(usize) -> ActionKind) -> Vec<(String, ActionKind)> {
    let mut out = Vec::new();
    if riding {
        out.push((TIE_UP.to_string(), if taken >= 2 { ActionKind::Greyed(POST_FULL.into()) } else { ActionKind::TieUp { post } }));
        return out;
    }
    if taken >= 2 {
        out.push((TIE_TO_POST.to_string(), ActionKind::Greyed(POST_FULL.into())));
    } else if free_horses.is_empty() {
        out.push((TIE_TO_POST.to_string(), ActionKind::Greyed(ALL_IN_USE.into())));
    } else {
        let list: Vec<(String, ActionKind)> = free_horses.iter().map(|(h, name)| (name.clone(), ActionKind::TieHorse { post, horse: *h, part })).collect();
        out.push((format!("{TIE_TO_POST} ›"), submenu(pie_submenus.len())));
        pie_submenus.push((TIE_TO_POST.to_string(), list));
    }
    if !tied_here.is_empty() {
        let list: Vec<(String, ActionKind)> = tied_here.iter().map(|(h, name, at)| (name.clone(), ActionKind::Untie { horse: *h, at: *at })).collect();
        out.push((format!("{UNTIE_HITCHED} ›"), submenu(pie_submenus.len())));
        pie_submenus.push((UNTIE_HITCHED.to_string(), list));
    }
    out
}

/// Called over to a post, a horse that's there is tied.
#[allow(clippy::type_complexity)]
fn tie_arrivals(mut commands: Commands, horses: Query<(Entity, &Pet, &TieWhenThere, &Transform), Without<crate::petneeds::PetTask>>, posts: Query<&Transform, (With<GameObject>, Without<Pet>)>) {
    for (e, pet, w, tf) in &horses {
        commands.entity(e).remove::<TieWhenThere>();
        let Ok(ptf) = posts.get(w.post) else { continue };
        let (at, _) = horse_spot(ptf, w.part);
        if tf.translation.xz().distance(at) < 1.0 {
            tie(&mut commands, e, w.post, w.part);
            info!("hitching: {} tied up", pet.name);
        }
    }
}

/// A horse tied at a post's place: held there, off its own wanderings.
pub fn tie(commands: &mut Commands, horse: Entity, post: Entity, part: usize) {
    commands.entity(horse).remove::<(crate::petneeds::PetTask, crate::petcompany::Greeting)>().insert((Tied::new(post, part), crate::petlitters::Held));
}

/// Untied: free again, its rope gone.
pub fn untie(commands: &mut Commands, horse: Entity, tied: Option<&Tied>) {
    if let Some(r) = tied.and_then(|t| t.rope) {
        commands.entity(r).try_despawn();
    }
    commands.entity(horse).remove::<(Tied, crate::petlitters::Held)>();
}

/// Tied horses: stood at their place facing the post, eating its hay and drinking from its
/// trough as they need, breaking the rope for what it can't give them.
#[allow(clippy::type_complexity)]
fn tied_horses(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    mut horses: Query<(Entity, &Pet, &mut Tied, &mut Transform, &mut PetAnim, Option<&mut PetMotives>, Has<crate::pets::PetBusy>, Has<crate::riding::Ridden>)>,
    posts: Query<&Transform, (With<GameObject>, Without<Pet>)>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let hours = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed] / 60.0;
    let mut rng = rand::rng();
    for (e, pet, mut t, mut tf, mut anim, motives, busy, ridden) in &mut horses {
        let Ok(ptf) = posts.get(t.post) else {
            untie(&mut commands, e, Some(&t));
            continue;
        };
        // (Someone at it, being ridden or groomed: theirs for now; ridden off, not tied.)
        if ridden {
            untie(&mut commands, e, Some(&t));
            continue;
        }
        if busy {
            continue;
        }
        let (at, face) = horse_spot(ptf, t.part);
        tf.translation = Vec3::new(at.x, tf.translation.y, at.y);
        tf.rotation = face;
        // Breaking free.
        if let Some(b) = t.breaking.as_mut() {
            *b += dt;
            let len = lib.get(&data, BREAK_CLIP).map_or(BREAK_SECS, |c| c.duration);
            if *b >= len {
                info!("hitching: {} broke the rope", pet.name);
                anim.play(crate::pets::idle_clip(&pet.kind, false, &mut rng));
                untie(&mut commands, e, Some(&t));
            }
            continue;
        }
        let Some(mut m) = motives else { continue };
        match (t.doing, at_post(&m, !pet.kind.starts_with('c'))) {
            (_, AtPost::Break) => {
                t.breaking = Some(0.0);
                t.doing = None;
                anim.replay(BREAK_CLIP.to_string());
            }
            // At the hay or the water till full.
            (Some((need, since)), _) => {
                let rate = if need == HUNGER { HAY_RATE } else { DRINK_RATE };
                m.add(need, rate * hours);
                let step = (since / 4.0) as u32;
                let clip = if need == HUNGER { format!("ah2o_hayPile_eat_loop{}_x", 1 + step % 4) } else { format!("ah2o_waterTrough_drink_loop{}_x", 1 + step % 3) };
                t.doing = Some((need, since + dt));
                anim.play(clip);
                if m.0[need] >= FULL {
                    t.doing = None;
                    anim.play(format!("{}_idle_stand_breathe_x", pet.kind));
                }
            }
            (None, AtPost::Eat) => t.doing = Some((HUNGER, 0.0)),
            (None, AtPost::Drink) => t.doing = Some((THIRST, 0.0)),
            (None, AtPost::Stand) => {
                if anim.clip.is_empty() || anim.clip.contains("walk") || anim.clip.contains("trot") || anim.clip.contains("run") {
                    anim.play(format!("{}_idle_stand_breathe_x", pet.kind));
                }
            }
        }
    }
}

/// A tied horse's rope: from its halter to its place's ring on the post, a thin cord.
#[allow(clippy::type_complexity)]
fn ropes(
    mut commands: Commands,
    mut horses: Query<(&mut Tied, &crate::pets::PetSkeleton)>,
    joints: Query<&GlobalTransform>,
    posts: Query<&GlobalTransform, With<GameObject>>,
    mut ropes: Query<&mut Transform>,
    (mut meshes, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    mut look: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    for (mut t, skel) in &mut horses {
        let Ok(ptf) = posts.get(t.post) else { continue };
        let ring = ptf.transform_point(ROPE_SLOTS[t.part.min(1)]);
        let Some(halter) = skel.bone("b__NoseBase__", &joints).or_else(|| skel.bone("b__Head__", &joints)) else { continue };
        let along = ring - halter;
        let len = along.length();
        if len < 0.05 {
            continue;
        }
        let tf = Transform::from_translation((ring + halter) * 0.5).with_rotation(Quat::from_rotation_arc(Vec3::Y, along / len)).with_scale(Vec3::new(1.0, len, 1.0));
        match t.rope.and_then(|r| ropes.get_mut(r).ok()) {
            Some(mut r) => *r = tf,
            None => {
                let (mesh, mat) = look
                    .get_or_insert_with(|| {
                        (meshes.add(Cylinder::new(0.012, 1.0)), mats.add(StandardMaterial { base_color: Color::srgb(0.55, 0.45, 0.3), perceptual_roughness: 0.9, ..default() }))
                    })
                    .clone();
                t.rope = Some(commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), tf, DespawnOnExit(crate::AppState::InGame))).id());
            }
        }
    }
}

/// `HITCH_TEST=<horse>` (tests): ten seconds in, that horse tied to the nearest hitching post
/// (put down by it if there's none), the camera on it; `HITCH_NEEDS=hungry|break` its hunger low,
/// or its energy so low it breaks free.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn hitching_test(
    mut commands: Commands,
    time: Res<Time>,
    mut horses: Query<(Entity, &Pet, &mut Transform, Option<&mut PetMotives>), With<HomePet>>,
    posts: Query<(Entity, &GameObject, &Transform), Without<Pet>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut riders: Query<(&mut crate::interact::ActionQueue, Option<&crate::riding::Mounted>), With<crate::sim::Selected>>,
    tied: Query<(), With<Tied>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut stage: Local<u8>,
) {
    // (HITCH_RIDE=1, with RIDE_TEST=<horse>: once up on it, "Tie Up" at the post.)
    if std::env::var_os("HITCH_RIDE").is_some()
        && *stage == 1
        && let Ok(mut q) = riders.single_mut()
        && q.1.is_some_and(|m| m.up())
        && let Some(post) = posts.iter().find(|p| p.1.kind == ObjectKind::HitchingPost).map(|p| p.0)
    {
        *stage = 2;
        q.0.push_player(crate::interact::Action::new(TIE_UP, ActionKind::TieUp { post }, false));
        info!("hitching test: tie up");
        return;
    }
    let Ok(name) = std::env::var("HITCH_TEST") else { return };
    if time.elapsed_secs() < 10.0 {
        return;
    }
    // (HITCH_CALL=1: the selected Sim ties it up on foot; then unties it.)
    if std::env::var_os("HITCH_CALL").is_some()
        && *stage >= 1
        && let Ok(mut q) = riders.single_mut()
        && let Some((post, _, ptf)) = posts.iter().find(|p| p.1.kind == ObjectKind::HitchingPost)
        && let Some((horse, ..)) = horses.iter().find(|h| h.1.name.eq_ignore_ascii_case(&name))
    {
        if *stage == 1 {
            *stage = 3;
            q.0.push_player(crate::interact::Action::new(TIE_TO_POST, ActionKind::TieHorse { post, horse, part: 1 }, false));
            info!("hitching test: tie on foot");
        } else if *stage == 3 && tied.contains(horse) && !q.0.0.iter().any(|a| matches!(a.kind, ActionKind::TieHorse { .. })) {
            *stage = 4;
            q.0.push_player(crate::interact::Action::new(UNTIE, ActionKind::Untie { horse, at: tie_spot(ptf, 1).0 }, false));
            info!("hitching test: untie");
        }
        return;
    }
    let Some((e, pet, mut tf, motives)) = horses.iter_mut().find(|h| h.1.name.eq_ignore_ascii_case(&name)) else { return };
    if std::env::var_os("PET_CAM").is_none()
        && let Ok(mut c) = cams.single_mut()
    {
        c.look_at(tf.translation);
        c.distance = std::env::var("HITCH_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(6.0);
    }
    match *stage {
        0 => {
            *stage = 1;
            if posts.iter().any(|p| p.1.kind == ObjectKind::HitchingPost) {
                return;
            }
            if let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("hitchingPostWestern")).map(|c| c.objd) {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
                let p = tf.translation + Vec3::new(4.0, 0.0, 0.0);
                crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, p, 0.0);
                info!("hitching test: a post at {p}");
            }
        }
        1 if std::env::var_os("HITCH_RIDE").is_some() || std::env::var_os("HITCH_CALL").is_some() => {}
        1 => {
            let Some((post, _, ptf)) = posts.iter().filter(|p| p.1.kind == ObjectKind::HitchingPost).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation))) else { return };
            *stage = 2;
            let (at, face) = horse_spot(ptf, 0);
            tf.translation = Vec3::new(at.x, tf.translation.y, at.y);
            tf.rotation = face;
            tie(&mut commands, e, post, 0);
            if let Some(mut m) = motives {
                match std::env::var("HITCH_NEEDS").as_deref() {
                    Ok("hungry") => m.0[HUNGER] = -50.0,
                    Ok("break") => m.0[ENERGY] = -70.0,
                    _ => {}
                }
            }
            info!("hitching test: {} tied", pet.name);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tied_horse_eats_drinks_or_breaks_free() {
        let mut m = PetMotives([50.0; 8]);
        assert_eq!(at_post(&m, true), AtPost::Stand);
        m.0[THIRST] = -30.0;
        assert_eq!(at_post(&m, true), AtPost::Drink);
        assert_eq!(at_post(&m, false), AtPost::Stand, "a foal's no thirst");
        m.0[HUNGER] = -30.0;
        assert_eq!(at_post(&m, true), AtPost::Eat, "hay first");
        m.0[ENERGY] = -70.0;
        assert_eq!(at_post(&m, true), AtPost::Break, "it can't sleep tied");
        assert_eq!(free_part(&[0]), Some(1));
        assert_eq!(free_part(&[1, 0]), None);
    }

    #[test]
    fn horses_stand_at_the_posts_places_facing_it() {
        let post = Transform::from_xyz(10.0, 0.0, 5.0);
        let (at, face) = horse_spot(&post, 1);
        assert!((at - Vec2::new(10.64, 6.529)).length() < 1e-3);
        let fwd = face * Vec3::Z;
        assert!(fwd.z < -0.99, "facing the post");
        let (sim, to) = tie_spot(&post, 0);
        assert!((sim - Vec2::new(8.25, 5.75)).length() < 1e-3);
        assert!(to.x > 0.0 && to.y < 0.0);
    }
}
