//! Babies, toddlers and how they arrive: "Try for Baby" may start a pregnancy, which after
//! three days brings a baby home to a crib. Babies live in their crib and need feeding,
//! changing, cuddles and sleep from the grown-ups; toddlers toddle about, play with toys and
//! nap, and still need feeding and changing. Little ones in distress cry.

use bevy::prelude::*;
use rand::Rng;

use crate::clock::GameClock;
use crate::interact::{GameObject, Notifications, ObjectKind};
use crate::life::{MoodletKind, Moodlets};
use crate::sim::*;
use crate::sound::PlaySound;
use crate::{AppState, PlayMode};

/// A toddler's skills, as the game's: walking and talking, each learned (at 1) over a few
/// lessons from a grown-up. Until they can walk, toddlers crawl.
#[derive(Component, Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ToddlerSkills {
    pub walk: f32,
    pub talk: f32,
}

impl ToddlerSkills {
    pub fn walks(&self) -> bool {
        self.walk >= 1.0
    }
    pub fn talks(&self) -> bool {
        self.talk >= 1.0
    }
}

/// How far one lesson takes a toddler (three to learn).
const LESSON: f32 = 0.34;

/// A lesson finished: what was taught, to whom, by whom.
#[derive(Message, Clone, Copy)]
pub struct Lesson {
    pub toddler: Entity,
    pub teacher: Entity,
    pub walk: bool,
}

/// Lessons count towards the toddler's skill; untaught toddlers crawl (slowly).
fn lessons(
    mut lessons: MessageReader<Lesson>,
    mut commands: Commands,
    mut toddlers: Query<(&Sim, Option<&mut ToddlerSkills>)>,
    teachers: Query<&Sim>,
    mut notes: ResMut<Notifications>,
) {
    for l in lessons.read() {
        let Ok((sim, skills)) = toddlers.get_mut(l.toddler) else { continue };
        let teacher = teachers.get(l.teacher).map(|t| t.first.clone()).unwrap_or_default();
        let mut s = skills.as_deref().copied().unwrap_or_default();
        let v = if l.walk { &mut s.walk } else { &mut s.talk };
        let before = *v;
        *v = (*v + LESSON).min(1.0);
        info!("{} had a lesson in {}: {:.0}%", sim.first, if l.walk { "walking" } else { "talking" }, *v * 100.0);
        if before < 1.0 && *v >= 1.0 {
            notes.push(format!("{} learned to {} with {}'s help!", sim.first, if l.walk { "walk" } else { "talk" }, teacher));
        } else {
            notes.push(format!("{} is learning to {} ({:.0}%).", sim.first, if l.walk { "walk" } else { "talk" }, *v * 100.0));
        }
        match skills {
            Some(mut k) => *k = s,
            None => {
                commands.entity(l.toddler).insert(s);
            }
        }
    }
}

fn crawl_speed(mut q: Query<(&Sim, Option<&ToddlerSkills>, &mut crate::nav::PathFollow), Added<crate::nav::PathFollow>>) {
    for (sim, skills, mut pf) in &mut q {
        if sim.age == Age::Toddler && !skills.is_some_and(|s| s.walks()) {
            pf.speed = 0.55;
        }
    }
}

pub struct LittlePlugin;

impl Plugin for LittlePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Lesson>()
            .add_systems(Update, (lessons, crawl_speed).run_if(in_state(PlayMode::Live)))
            .add_message::<Conceive>()
            .add_systems(Update, (conceive, pregnancy, little_life, put_down).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(PostUpdate, carry_follow.after(bevy::transform::TransformSystems::Propagate));
    }
}

/// "Try for Baby" succeeded between these two Sims.
#[derive(Message, Clone, Copy)]
pub struct Conceive {
    pub a: Entity,
    pub b: Entity,
}

/// A pregnant Sim: when it started (game minutes), the other parent, and what's been shown.
#[derive(Component, Clone, Copy, Debug)]
pub struct Pregnancy {
    pub since: f64,
    pub other_parent: Option<Entity>,
    pub stage: u8,
}

/// Days from conception to birth (the game's normal pregnancy).
const PREGNANCY_DAYS: f64 = 3.0;

fn conceive(
    mut commands: Commands,
    mut events: MessageReader<Conceive>,
    clock: Res<GameClock>,
    sims: Query<(&Sim, Has<Pregnancy>, Option<&crate::wishes::Wishes>)>,
    mut play: MessageWriter<PlaySound>,
) {
    for ev in events.read() {
        let (Ok((a, a_preg, aw)), Ok((b, b_preg, bw))) = (sims.get(ev.a), sims.get(ev.b)) else { continue };
        if a.female == b.female || a_preg || b_preg {
            continue;
        }
        // Not every try succeeds (most do, with a Fertility Treatment).
        let chance = if crate::wishes::has(aw, "FertilityTreatment") || crate::wishes::has(bw, "FertilityTreatment") { 0.9 } else { 0.6 };
        if !rand::rng().random_bool(chance) {
            info!("conceive: {} and {} tried for a baby, no luck this time", a.first, b.first);
            continue;
        }
        info!("conceive: {} and {} are expecting", a.first, b.first);
        let (mother, father) = if a.female { (ev.a, ev.b) } else { (ev.b, ev.a) };
        commands.entity(mother).insert(Pregnancy { since: clock.minutes, other_parent: Some(father), stage: 0 });
        play.write(PlaySound::ui("sting_baby_conception").with_volume(0.6));
    }
}

/// Morning sickness, the bump, then the birth.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pregnancy(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut mothers: Query<(Entity, &Sim, &mut Pregnancy, &mut Moodlets, &Transform, Has<HouseholdMember>)>,
    cribs: Query<(&GameObject, &Transform)>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<PlaySound>,
    (catalog, data, cas, mut assets): (
        Res<crate::loading::Catalog>,
        Res<crate::baked::Baked>,
        Option<Res<crate::simbody::CasData>>,
        ResMut<crate::objects::ObjectAssets>,
    ),
    (sim_assets, mut meshes, mut images, mut mats): (Res<SimAssets>, ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    (mut skin_mats, mut bindposes, mut textures): (
        ResMut<Assets<crate::simbody::SimSkinMaterial>>,
        ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
        ResMut<crate::simbody::SimTextures>,
    ),
    (mut family, fathers): (ResMut<crate::family::Genealogy>, Query<&Sim, Without<Pregnancy>>),
) {
    for (mother, sim, mut p, mut moodlets, tf, member) in &mut mothers {
        let days = (clock.minutes - p.since) / 1440.0;
        if p.stage == 0 && days > 0.25 {
            p.stage = 1;
            moodlets.add(MoodletKind::Nauseous, clock.minutes);
            if member {
                notes.push(format!("{} isn't feeling well this morning…", sim.first));
            }
        }
        if p.stage == 1 && days > 1.0 {
            p.stage = 2;
            moodlets.add(MoodletKind::Pregnant, clock.minutes);
            if member {
                notes.push(format!("{} is pregnant!", sim.first));
            }
        }
        if days < PREGNANCY_DAYS {
            continue;
        }
        commands.entity(mother).remove::<Pregnancy>();
        moodlets.remove(MoodletKind::Pregnant);
        if !member {
            continue;
        }
        // The baby: a boy or a girl, with two traits, named after nobody in particular.
        let mut rng = rand::rng();
        let female = rng.random_bool(0.5);
        let mut baby = random_sim(&mut rng, &sim.last, Some(female), Age::Baby);
        baby.skin = sim.skin;
        baby.hair = if rng.random_bool(0.5) { sim.hair } else { baby.hair };
        // Eyes like the mother's or the father's.
        let father = p.other_parent.and_then(|f| fathers.get(f).ok());
        baby.eyes = match father {
            Some(f) if rng.random_bool(0.5) => f.eyes,
            _ if rng.random_bool(0.85) || father.is_some() => sim.eyes,
            _ => baby.eyes,
        };
        // The crib: the household's own, or a new one beside the mother.
        let crib = cribs.iter().filter(|(o, _)| o.kind == ObjectKind::Crib).min_by(|a, b| {
            a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation))
        });
        let (crib_pos, crib_rot) = match crib {
            Some((_, ctf)) => (ctf.translation, ctf.rotation),
            None => {
                let pos = tf.translation + tf.rotation * Vec3::new(1.2, 0.0, 0.6);
                let key = catalog.entries.iter().filter(|e| e.kind == ObjectKind::Crib && e.price > 0).min_by_key(|e| e.price).map(|e| e.key);
                if let Some(key) = key {
                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                    crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, pos, 0.0);
                }
                (pos, Quat::IDENTITY)
            }
        };
        let model = cas.as_ref().and_then(|cas| {
            let outfit = crate::simbody::pick_outfit(cas, &baby, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(baby.look));
            crate::simbody::build_sim_model(&data.0, cas, &baby, &outfit, crate::simbody::tone_of(&baby))
        });
        let name = baby.first.clone();
        // The family tree: the mother, and the father if he's known.
        family.note(sim.id, &sim.full_name(), sim.female);
        family.note(baby.id, &baby.full_name(), baby.female);
        family.add_parent(baby.id, sim.id);
        if let Some(f) = p.other_parent.and_then(|f| fathers.get(f).ok()) {
            family.note(f.id, &f.full_name(), f.female);
            family.add_parent(baby.id, f.id);
        }
        let mut sctx = SimSpawnCtx {
            assets: &sim_assets,
            render: crate::simbody::SimRenderCtx {
                meshes: &mut meshes,
                images: &mut images,
                mats: &mut mats,
                skin_mats: &mut skin_mats,
                bindposes: &mut bindposes,
                textures: &mut textures,
            },
        };
        let e = spawn_sim_full(&mut commands, &mut sctx, baby, crib_pos + Vec3::Y * CRIB_HEIGHT, model);
        commands.entity(e).insert((
            Transform::from_translation(crib_pos + Vec3::Y * CRIB_HEIGHT).with_rotation(crib_rot),
            HouseholdMember,
            crate::interact::ActionQueue::default(),
            crate::interact::Skills::default(),
            InCrib(crib_pos, crib_rot),
            crate::aging::Aging::default(),
            DespawnOnExit(AppState::InGame),
        ));
        moodlets.add(MoodletKind::NewBaby, clock.minutes);
        play.write(PlaySound::ui("sting_good_event").with_volume(0.7));
        notes.push(format!("{} had a baby {}! Welcome to the family, {name}.", sim.first, if female { "girl" } else { "boy" }));
    }
}

/// Height of a crib's mattress above the floor.
pub const CRIB_HEIGHT: f32 = 0.42;

/// A baby lying in the crib at this position and facing.
#[derive(Component, Clone, Copy)]
pub struct InCrib(pub Vec3, pub Quat);

/// A baby held in a grown-up's arms (on their carry slot) during a care social.
#[derive(Component, Clone, Copy)]
pub struct Carried {
    pub by: Entity,
}

/// Keeps carried babies cradled on the grown-up's left forearm, head in the crook of the
/// elbow. (The game slots babies to a carry bone and IK-solves the arms around them; the
/// carry clips hold the left forearm out palm-up, so the forearm makes a steady cradle.)
fn carry_follow(
    mut carried: Query<(&Carried, &mut Transform)>,
    carriers: Query<&crate::simbody::Skeleton>,
    joints: Query<&GlobalTransform>,
) {
    for (c, mut tf) in &mut carried {
        let Ok(skel) = carriers.get(c.by) else { continue };
        let at = |name: &str| {
            let i = skel.rig.bones.iter().position(|b| b.name == name)?;
            Some(joints.get(*skel.joints.get(i)?).ok()?.translation())
        };
        // The cradling arm: whichever forearm lies flatter (the other may hold a bottle).
        let arm = |side: &str| Some((at(&format!("b__{side}_Forearm__"))?, at(&format!("b__{side}_Hand__"))?));
        let level = |(e, h): (Vec3, Vec3)| (e - h).normalize_or(Vec3::Y).y.abs() + (e.y + h.y) * 0.25;
        let Some((elbow, hand)) = [arm("L"), arm("R")].into_iter().flatten().min_by(|a, b| level(*a).total_cmp(&level(*b))) else {
            continue;
        };
        let along = (elbow - hand).normalize_or(Vec3::Z);
        // Lie level along the forearm, face up.
        let flat = Vec3::new(along.x, along.y * 0.3, along.z).normalize_or(Vec3::Z);
        tf.rotation = Transform::IDENTITY.looking_to(flat, Vec3::Y).rotation;
        tf.translation = hand.lerp(elbow, 0.45) + Vec3::Y * 0.02;
    }
}

/// Puts a carried baby back once the grown-up's care social is over.
fn put_down(
    mut commands: Commands,
    carried: Query<(Entity, &Carried)>,
    carriers: Query<&crate::interact::ActionQueue>,
) {
    for (e, c) in &carried {
        let busy = carriers.get(c.by).ok().and_then(|q| q.0.front()).is_some_and(|a| {
            matches!(a.kind, crate::interact::ActionKind::Social { target, .. } if target == e)
                && matches!(a.phase, crate::interact::Phase::Running(_))
        });
        if !busy {
            commands.entity(e).remove::<Carried>();
        }
    }
}

/// Put to bed: sleeps (in the crib) until rested.
#[derive(Component)]
pub struct Bedtime;

/// When a little one last cried for attention (game minutes).
#[derive(Component, Default)]
struct LastCry(f64);

/// Babies sleep and wake in their crib and cry when they need something; toddlers cry too.
#[allow(clippy::type_complexity)]
fn little_life(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut littles: Query<(
        Entity,
        &Sim,
        &mut Motives,
        &mut SimAnim,
        Option<&InCrib>,
        Option<&LastCry>,
        &mut Transform,
        Option<&crate::anim::ActionClip>,
        Option<&Bedtime>,
        Has<Carried>,
    )>,
    cribs: Query<(&GameObject, &Transform), Without<Sim>>,
    mut notes: ResMut<Notifications>,
) {
    let h = clock.hour_f();
    for (e, sim, mut motives, mut anim, crib, last_cry, mut tf, clip, bedtime, carried) in &mut littles {
        if !sim.age.is_little() || carried {
            continue;
        }
        // A toddler put to bed naps in the crib.
        if sim.age == Age::Toddler && bedtime.is_some() {
            if let Some((_, ctf)) = cribs.iter().filter(|(o, _)| o.kind == ObjectKind::Crib).min_by(|a, b| {
                a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation))
            }) {
                tf.translation = ctf.translation + Vec3::Y * CRIB_HEIGHT;
                tf.rotation = ctf.rotation;
            }
            anim.pose = Pose::Lie;
            motives.add(ENERGY, 0.5);
            if motives.0[ENERGY] > 90.0 {
                commands.entity(e).remove::<Bedtime>();
                anim.pose = Pose::Stand;
            }
        }
        if sim.age == Age::Baby {
            // Babies stay in their crib: find one if they haven't got one.
            let (at, facing) = match crib {
                Some(c) => (c.0, c.1),
                None => {
                    let Some((_, ctf)) = cribs.iter().filter(|(o, _)| o.kind == ObjectKind::Crib).min_by(|a, b| {
                        a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation))
                    }) else {
                        continue;
                    };
                    commands.entity(e).insert(InCrib(ctf.translation, ctf.rotation));
                    (ctf.translation, ctf.rotation)
                }
            };
            tf.translation = at + Vec3::Y * CRIB_HEIGHT;
            tf.rotation = facing;
            // Sleep at night, whenever tired, and when put to bed.
            let sleepy = motives.0[ENERGY] < -20.0 || !(7.0..20.0).contains(&h) || bedtime.is_some();
            if bedtime.is_some() && motives.0[ENERGY] > 90.0 {
                commands.entity(e).remove::<Bedtime>();
            }
            anim.pose = if sleepy { Pose::Lie } else { Pose::Stand };
            if sleepy {
                motives.add(ENERGY, 0.5);
            }
        }
        // Crying for food, a clean diaper or company.
        let need = [(HUNGER, "is hungry"), (BLADDER, "needs a diaper change"), (SOCIAL, "wants some attention")]
            .into_iter()
            .find(|(m, _)| motives.0[*m] < -40.0);
        if let Some((_, what)) = need {
            let crying = clip.is_some_and(|c| c.loops.first().is_some_and(|l| l.contains("cry")));
            if !crying && anim.pose != Pose::Lie {
                let cry = if sim.age == Age::Baby { CRY_BABY } else { CRY_TODDLER };
                commands.entity(e).insert(cry);
            }
            if last_cry.is_none_or(|c| clock.minutes - c.0 > 180.0) {
                notes.push(format!("{} {what}.", sim.first));
                commands.entity(e).insert(LastCry(clock.minutes));
            }
        } else if clip.is_some_and(|c| c.loops.first().is_some_and(|l| l.contains("cry"))) {
            commands.entity(e).remove::<crate::anim::ActionClip>();
        }
    }
}

const CRY_BABY: crate::anim::ActionClip = crate::anim::ActionClip::new(Some("b2o_crib_cry_start_y"), &["b2o_crib_cry_loop"]);
const CRY_TODDLER: crate::anim::ActionClip = crate::anim::ActionClip::new(Some("p2o_crib_cry_start_y"), &["p2o_crib_cry_loop"]);

