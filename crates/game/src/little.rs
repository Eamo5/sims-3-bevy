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
    /// Potty training (see `potty`).
    #[serde(default)]
    pub potty: f32,
}

impl ToddlerSkills {
    pub fn walks(&self) -> bool {
        self.walk >= 1.0
    }
    pub fn talks(&self) -> bool {
        self.talk >= 1.0
    }
    pub fn potty_trained(&self) -> bool {
        self.potty >= 1.0
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
            .init_resource::<NewbornPicks>()
            .add_systems(Update, (conceive, pregnancy, name_babies, newborn_traits_picker, little_life, put_down).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(Update, kids_radio.run_if(in_state(PlayMode::Live)))
            .add_systems(PostUpdate, carry_follow.after(bevy::transform::TransformSystems::Propagate));
    }
}

/// "Try for Baby" succeeded between these two Sims.
#[derive(Message, Clone, Copy)]
pub struct Conceive {
    pub a: Entity,
    pub b: Entity,
}

/// A pregnant Sim: when it started (game minutes), the other parent, how far along (`stage`:
/// 1 sick in the mornings, 2 showing, 3 the walk, 4 in labour), her mood over it all (mood ×
/// game minutes, as the game's `PregnancyScore`), and the last hour gone by.
#[derive(Component, Clone, Copy, Debug)]
pub struct Pregnancy {
    pub since: f64,
    pub other_parent: Option<Entity>,
    pub stage: u8,
    pub mood: f64,
    pub hour: u32,
    /// How much likelier a girl than evens (the game's `mBabySexOffset`: watermelon a girl, apples
    /// a boy, a twentieth a piece, at most three twentieths either way).
    pub girl_bias: f32,
    /// Hours of children's TV and radio she's had (the game's `mMultipleBabiesMultiplier`, one
    /// more for each: twins and triplets likelier).
    pub kids_hours: f64,
}

/// How near a stereo playing the children's station she's in its hearing (the game's room's
/// listeners).
const KIDS_RADIO_RANGE: f32 = 8.0;

/// A pregnant Sim in hearing of a stereo on the children's station: the hours counted, as the
/// game's `StartListeningKidsRadio` (twins likelier).
fn kids_radio(delta: Res<crate::clock::SimDelta>, mut pregnant: Query<(&GlobalTransform, &mut Pregnancy)>, stereos: Query<(&GlobalTransform, &crate::music::StereoOn)>) {
    let hours = delta.0 as f64 / 60.0;
    if hours <= 0.0 {
        return;
    }
    let playing: Vec<Vec3> = stereos.iter().filter(|(_, on)| on.0.as_deref() == Some("stereo_kids")).map(|(tf, _)| tf.translation()).collect();
    if playing.is_empty() {
        return;
    }
    for (tf, mut p) in &mut pregnant {
        if playing.iter().any(|s| s.distance(tf.translation()) < KIDS_RADIO_RANGE) {
            p.kids_hours += hours;
        }
    }
}

/// The game's `kGenderOffsetPerFoodEaten` and `kMaxGenderOffset`.
pub const GENDER_OFFSET_PER_FOOD: f32 = 0.05;
pub const MAX_GENDER_OFFSET: f32 = 0.15;

impl Pregnancy {
    pub fn new(since: f64, other_parent: Option<Entity>) -> Self {
        Self { since, other_parent, stage: 0, mood: 0.0, hour: 0, girl_bias: 0.0, kids_hours: 0.0 }
    }

    /// Something eaten that sways the baby's sex (the game's `Ingredient` keys: `Watermelon` a
    /// girl, `Apple` a boy).
    pub fn ate(&mut self, key: &str) {
        let d = match key {
            "Watermelon" => GENDER_OFFSET_PER_FOOD,
            "Apple" => -GENDER_OFFSET_PER_FOOD,
            _ => return,
        };
        self.girl_bias = (self.girl_bias + d).clamp(-MAX_GENDER_OFFSET, MAX_GENDER_OFFSET);
    }

    /// Her mood on average over the pregnancy so far.
    pub fn average_mood(&self, now: f64) -> f32 {
        let minutes = (now - self.since).max(1.0);
        (self.mood / minutes) as f32
    }
}

/// The game's pregnancy, by the hour (`Pregnancy` tuning): sick from hour 8 (and now and then
/// till 24, one hour in ten), showing at 24, the pregnant walk from 32, the waters breaking and
/// contractions at 70, the baby at 72.
const HOUR_SICK: u32 = 8;
const HOUR_SHOWING: u32 = 24;
const HOUR_WALK: u32 = 32;
const HOUR_LABOUR: u32 = 70;
pub const HOURS_OF_PREGNANCY: u32 = 72;
const CHANCE_OF_NAUSEA: f64 = 0.1;
/// A contraction every five minutes in labour; four puddles when the waters break.
const CONTRACTION_MINUTES: f64 = 5.0;
const WATER_PUDDLES: usize = 4;
/// Twins one time in twenty, triplets one in a hundred; three and a half times as likely with a
/// Fertility Treatment. A hospital birth's 10 more mood towards the baby's traits.
const CHANCE_OF_TWINS: f64 = 0.05;
const CHANCE_OF_TRIPLETS: f64 = 0.01;
const FERTILITY_MULTIPLIER: f64 = 3.5;
const HOSPITAL_MOOD_BONUS: f32 = 10.0;
/// The most Sims a household can have.
const HOUSEHOLD_MAX: usize = 8;

/// In labour: when the next contraction comes.
#[derive(Component, Clone, Copy, Debug)]
pub struct Labour {
    pub next: f64,
}

/// Off to the hospital to have the baby (a rabbit hole's "activity", lasting till the birth).
pub const BIRTH_TASK: usize = 997;
pub static HAVE_BABY: crate::rabbitholes::Activity = crate::rabbitholes::act("Have the Baby", 24.0 * 60.0, 0, [0.0, 0.0, -2.0, 0.0, 0.0, 0.0], None);

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
        commands.entity(mother).insert(Pregnancy::new(clock.minutes, Some(father)));
        play.write(PlaySound::ui("sting_baby_conception").with_volume(0.6));
    }
}

/// How the babies' traits come out of the mother's mood over the pregnancy (the game's
/// `TraitOutcome`, by `kMoodLevel*`: under -49, under 0, under 31, under 66, and above).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TraitOutcome {
    Horrible,
    Bad,
    Average,
    Good,
    Excellent,
}

impl TraitOutcome {
    pub fn of(mood: f32) -> Self {
        if mood < -49.0 {
            TraitOutcome::Horrible
        } else if mood < 0.0 {
            TraitOutcome::Bad
        } else if mood < 31.0 {
            TraitOutcome::Average
        } else if mood < 66.0 {
            TraitOutcome::Good
        } else {
            TraitOutcome::Excellent
        }
    }
}

/// How many babies come (the game's `GetNumForBirth`): one, twins one time in twenty, triplets
/// one in a hundred (more likely with a Fertility Treatment, and for each hour of children's TV
/// and radio in the pregnancy), no more than the household has room for.
pub fn babies_born(roll: f64, fertility: bool, kids_hours: f64, room: usize) -> usize {
    let m = (if fertility { FERTILITY_MULTIPLIER } else { 1.0 }) * (1.0 + kids_hours.max(0.0));
    let n = if roll < CHANCE_OF_TRIPLETS * m { 3 } else if roll < CHANCE_OF_TWINS * m { 2 } else { 1 };
    n.min(room.max(1))
}

/// A newborn's traits as the game's `Genetics.InheritTraits` draws them: in an unhappy
/// pregnancy, the bad ones (horrible and negative ones, by their weights); in an ordinary one,
/// two from the parents (each half the time one of theirs, else one at random); a happy one,
/// one; a very happy one, none, the rest the player's to choose (`chosen`: how many of the
/// two are left to them). `score` and `weight` are a trait's (from the game's tables).
pub fn newborn_traits(
    outcome: TraitOutcome,
    parents: &[crate::life::Trait],
    score: &dyn Fn(crate::life::Trait) -> (i32, f32, bool),
    rng: &mut impl Rng,
) -> (Vec<crate::life::Trait>, usize) {
    use crate::life::Trait;
    let slots = crate::life::trait_slots(Age::Baby);
    let pool: Vec<Trait> = Trait::ALL.iter().copied().filter(|t| t.allowed_at(Age::Baby) && score(*t).2).collect();
    let mut out: Vec<Trait> = Vec::new();
    // (Weighted from a list, fitting with what's drawn.)
    let draw = |from: &[Trait], out: &Vec<Trait>, rng: &mut dyn rand::RngCore| -> Option<Trait> {
        let fits: Vec<Trait> = from.iter().copied().filter(|t| t.compatible(out)).collect();
        let total: f32 = fits.iter().map(|t| score(*t).1.max(0.1)).sum();
        if fits.is_empty() || total <= 0.0 {
            return None;
        }
        let mut x = rng.random::<f32>() * total;
        for t in &fits {
            x -= score(*t).1.max(0.1);
            if x <= 0.0 {
                return Some(*t);
            }
        }
        fits.last().copied()
    };
    let bad: Vec<Trait> = pool.iter().copied().filter(|t| score(*t).0 < 0).collect();
    let good: Vec<Trait> = pool.iter().copied().filter(|t| score(*t).0 >= 0).collect();
    let inherited: Vec<Trait> = parents.iter().copied().filter(|t| t.allowed_at(Age::Baby)).collect();
    // (One the parents have half the time, else one from all there are.)
    let genetic = |out: &Vec<Trait>, rng: &mut dyn rand::RngCore| -> Option<Trait> {
        if rng.random_bool(0.5)
            && let Some(t) = draw(&inherited, out, rng)
        {
            return Some(t);
        }
        draw(&pool, out, rng)
    };
    let push = |t: Option<Trait>, out: &mut Vec<Trait>| {
        if let Some(t) = t
            && out.len() < slots
        {
            out.push(t);
        }
    };
    match outcome {
        TraitOutcome::Horrible => {
            for _ in 0..slots {
                let t = draw(&bad, &out, rng);
                push(t, &mut out);
            }
        }
        TraitOutcome::Bad => {
            let t = draw(&bad, &out, rng);
            push(t, &mut out);
            let either: Vec<Trait> = bad.iter().chain(&good).copied().collect();
            let t = draw(&either, &out, rng);
            push(t, &mut out);
        }
        TraitOutcome::Average => {
            for _ in 0..slots {
                let t = genetic(&out, rng);
                push(t, &mut out);
            }
        }
        TraitOutcome::Good => {
            let t = genetic(&out, rng);
            push(t, &mut out);
        }
        TraitOutcome::Excellent => {}
    }
    let chosen = slots.saturating_sub(out.len());
    (out, chosen)
}

/// Newborns whose last traits are the player's to choose, one at a time (after their names).
#[derive(Resource, Default)]
pub struct NewbornPicks(pub std::collections::VecDeque<Entity>);

/// The game's pregnancy hour by hour (sick in the mornings, the bump, the walk, labour with its
/// waters and contractions, "Go to the Hospital" there to be had), and the birth at hour 72:
/// at home, or at the hospital in a little better spirits.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pregnancy(
    mut commands: Commands,
    clock: Res<GameClock>,
    delta: Res<crate::clock::SimDelta>,
    mut mothers: Query<(
        Entity,
        &Sim,
        &mut Pregnancy,
        &mut Moodlets,
        &Transform,
        Has<HouseholdMember>,
        &crate::life::Mood,
        (Option<&mut Labour>, Option<&mut crate::rabbitholes::AtRabbitHole>, Option<&mut crate::interact::ActionQueue>, &crate::nav::Floor, Option<&crate::wishes::Wishes>),
    )>,
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
    (mut family, fathers, members): (ResMut<crate::family::Genealogy>, Query<(&Sim, Option<&crate::wishes::Wishes>), Without<Pregnancy>>, Query<(), With<HouseholdMember>>),
    (mut questions, mut picks, ui, words, mut puddles, mut topics): (
        ResMut<crate::dialog::Questions>,
        ResMut<NewbornPicks>,
        Option<Res<crate::icons::GameUi>>,
        Option<Res<crate::layout::UiAssets>>,
        MessageWriter<crate::puddles::AddPuddle>,
        MessageWriter<crate::topics::AddTopic>,
    ),
) {
    let mut rng = rand::rng();
    for (mother, sim, mut p, mut moodlets, tf, member, mood, (labour, at, mut queue, floor, wishes)) in &mut mothers {
        // (Her mood all along, for the baby's traits.)
        p.mood += mood.0 as f64 * delta.0 as f64;
        let hours = ((clock.minutes - p.since) / 60.0).max(0.0) as u32;
        while p.hour < hours.min(HOURS_OF_PREGNANCY) {
            p.hour += 1;
            let h = p.hour;
            if h == HOUR_SICK || (h > HOUR_SICK && h < HOUR_SHOWING && rng.random_bool(CHANCE_OF_NAUSEA)) {
                moodlets.add(MoodletKind::Nauseous, clock.minutes);
                if p.stage == 0 {
                    p.stage = 1;
                    if member {
                        notes.push(format!("{} isn't feeling well this morning…", sim.first));
                    }
                }
            }
            if h == HOUR_SHOWING {
                p.stage = 2;
                moodlets.add(MoodletKind::Pregnant, clock.minutes);
                if member {
                    notes.push(format!("{} is pregnant!", sim.first));
                }
            }
            if h == HOUR_WALK {
                p.stage = 3;
                topics.write(crate::topics::AddTopic::new(mother, "Pregnant"));
            }
            if h == HOUR_LABOUR {
                // The waters break; the contractions begin.
                p.stage = 4;
                for _ in 0..WATER_PUDDLES {
                    puddles.write(crate::puddles::AddPuddle { at: tf.translation, level: floor.0, lasts: None });
                }
                moodlets.remove(MoodletKind::Pregnant);
                moodlets.add(MoodletKind::BabyIsComing, clock.minutes);
                commands.entity(mother).insert(Labour { next: clock.minutes });
                if member {
                    notes.push(format!("{}'s baby is coming! She can go to the hospital, or have the baby at home.", sim.first));
                    play.write(PlaySound::ui("sting_baby_conception").with_volume(0.6));
                }
            }
        }
        // A contraction every few minutes, when she isn't busy with one.
        if let (Some(mut l), Some(q)) = (labour, queue.as_mut())
            && at.is_none()
            && clock.minutes >= l.next
        {
            l.next = clock.minutes + CONTRACTION_MINUTES;
            if q.0.iter().all(|a| !a.label.starts_with("Contraction")) {
                let clips: &'static [&'static str] = if rng.random_bool(0.5) { &["a_pregnancy_contractions_x"] } else { &["a_pregnancy_contractions2_x"] };
                q.0.push_front(crate::interact::Action::new(
                    "Contraction",
                    crate::interact::ActionKind::Outro { clips, then: None, secs: 4.0, stand_at: None, target: mother },
                    false,
                ));
            }
        }
        if p.hour < HOURS_OF_PREGNANCY {
            continue;
        }
        // The birth: at the hospital, or at home.
        let hospital = at.as_ref().is_some_and(|a| std::ptr::eq(a.activity, &HAVE_BABY));
        if let Some(mut a) = at {
            if hospital {
                // (Home again with the babies.)
                a.until = a.until.min(clock.minutes + 25.0);
            } else {
                // (Out at a rabbit hole otherwise: the baby waits till she's home.)
                continue;
            }
        }
        commands.entity(mother).remove::<(Pregnancy, Labour)>();
        moodlets.remove(MoodletKind::Pregnant);
        moodlets.remove(MoodletKind::BabyIsComing);
        if !member {
            continue;
        }
        if !hospital
            && let Some(mut q) = queue
        {
            q.0.retain(|a| !a.label.starts_with("Contraction"));
            q.0.push_front(crate::interact::Action::new(
                "Have Baby",
                crate::interact::ActionKind::Outro { clips: &["a2b_pregnancy_homeBirth_x"], then: None, secs: 8.0, stand_at: None, target: mother },
                false,
            ));
        }
        let father = p.other_parent.and_then(|f| fathers.get(f).ok());
        let fertility = crate::wishes::has(wishes, "FertilityTreatment") || father.is_some_and(|(_, w)| crate::wishes::has(w, "FertilityTreatment"));
        let n = babies_born(rng.random::<f64>(), fertility, p.kids_hours, HOUSEHOLD_MAX.saturating_sub(members.iter().count()));
        let average = p.average_mood(clock.minutes) + if hospital { HOSPITAL_MOOD_BONUS } else { 0.0 };
        let outcome = TraitOutcome::of(average);
        info!("birth: {} has {n} at {} (mood {average:.0}: {outcome:?})", sim.first, if hospital { "the hospital" } else { "home" });
        let parent_traits: Vec<crate::life::Trait> = sim.traits.iter().chain(father.map(|f| f.0.traits.iter()).into_iter().flatten()).copied().collect();
        let score = |t: crate::life::Trait| ui.as_ref().and_then(|u| u.trait_info(t)).map_or((10, 10.0, true), |i| (i.score, i.weight, i.random));
        // The cribs: the household's own, nearest first, and new ones beside her for the rest.
        let mut free: Vec<(Vec3, Quat)> = cribs.iter().filter(|(o, _)| o.kind == ObjectKind::Crib).map(|(_, c)| (c.translation, c.rotation)).collect();
        free.sort_by(|a, b| a.0.distance(tf.translation).total_cmp(&b.0.distance(tf.translation)));
        let mut girls = 0;
        for k in 0..n {
            // (Evens, as swayed by what she ate.)
            let female = rng.random_bool((0.5 + p.girl_bias.clamp(-MAX_GENDER_OFFSET, MAX_GENDER_OFFSET)) as f64);
            girls += female as usize;
            let mut baby = random_sim(&mut rng, &sim.last, Some(female), Age::Baby);
            baby.skin = sim.skin;
            baby.hair = if rng.random_bool(0.5) { sim.hair } else { baby.hair };
            // Eyes like the mother's or the father's; a face between theirs.
            baby.eyes = match father {
                Some((f, _)) if rng.random_bool(0.5) => f.eyes,
                _ if rng.random_bool(0.85) || father.is_some() => sim.eyes,
                _ => baby.eyes,
            };
            baby.face = crate::simbody::inherited_face(sim, father.map(|f| f.0), &mut rng);
            baby.face_presets = crate::simbody::inherited_presets(sim, father.map(|f| f.0), &mut rng);
            // (The rest the player's, in the traits picker after the name.)
            let (traits, chosen) = newborn_traits(outcome, &parent_traits, &score, &mut rng);
            baby.traits = traits;
            let (crib_pos, crib_rot) = match free.get(k) {
                Some(&c) => c,
                None => {
                    let pos = tf.translation + tf.rotation * Vec3::new(1.2 + k as f32 * 1.4, 0.0, 0.6);
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
            // The family tree: the mother, and the father if he's known.
            family.note(sim.id, &sim.full_name(), sim.female);
            family.note(baby.id, &baby.full_name(), baby.female);
            family.add_parent(baby.id, sim.id);
            if let Some((f, _)) = father {
                family.note(f.id, &f.full_name(), f.female);
                family.add_parent(baby.id, f.id);
            }
            let prompt = birth_prompt(ui.as_deref(), words.as_deref(), outcome, sim, &baby);
            let title = if baby.female { "It's a Girl!" } else { "It's a Boy!" };
            let suggested = baby.first.clone();
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
            // Named as the game's: a box for each, the traits it came with said.
            questions.ask(crate::dialog::Ask {
                about: crate::dialog::Question::NameBaby { sim: e, suggested },
                icon: String::new(),
                heading: format!("{}'s baby", sim.first),
                title: title.to_string(),
                text: prompt,
                answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
            });
            if chosen > 0 {
                picks.0.push_back(e);
            }
        }
        // It's a Boy, a Girl, Twins or Triplets, for both parents.
        let kind = match (n, girls) {
            (3.., _) => MoodletKind::ItsTriplets,
            (2, _) => MoodletKind::ItsTwins,
            (_, 1) => MoodletKind::ItsAGirl,
            _ => MoodletKind::NewBaby,
        };
        moodlets.add(kind, clock.minutes);
        if let Some(f) = p.other_parent {
            let now = clock.minutes;
            commands.entity(f).queue(move |mut w: EntityWorldMut| {
                if let Some(mut m) = w.get_mut::<Moodlets>() {
                    m.add(kind, now);
                }
            });
        }
        play.write(PlaySound::ui("sting_good_event").with_volume(0.7));
        let said = match n {
            3 => "triplets".to_string(),
            2 => "twins".to_string(),
            _ => format!("a baby {}", if girls == 1 { "girl" } else { "boy" }),
        };
        notes.push(format!("{} had {said}{}!", sim.first, if hospital { " at the hospital" } else { "" }));
    }
}

/// The game's words for a newborn as it's named, by how the traits came out
/// (`Gameplay/CAS/Genetics:MakeBabyDesc...`).
fn birth_prompt(ui: Option<&crate::icons::GameUi>, words: Option<&crate::layout::UiAssets>, outcome: TraitOutcome, mother: &Sim, baby: &Sim) -> String {
    let name = |i: usize| baby.traits.get(i).map_or(String::new(), |t| ui.and_then(|u| u.trait_info(*t)).map_or(t.name().to_string(), |i| i.name));
    let (key, n) = match outcome {
        TraitOutcome::Horrible => ("MakeBabyDescTwoTraitsHorrible", 2),
        TraitOutcome::Bad => ("MakeBabyDescTwoTraitsBad", 2),
        TraitOutcome::Average => ("MakeBabyDescTwoTraits", 2),
        TraitOutcome::Good => ("MakeBabyDescOneTraits", 1),
        TraitOutcome::Excellent => ("MakeBabyDesc", 0),
    };
    let text = words.and_then(|w| {
        let key = format!("Gameplay/CAS/Genetics:{key}");
        (if baby.female { w.localize(&format!("{key}_Female")) } else { None }).or_else(|| w.localize(&key))
    });
    match text {
        Some(t) => t.replace("{0.SimFirstName}", &mother.first).replace("{1.String}", &name(0)).replace("{2.String}", &name(1)),
        None => match n {
            2 => format!("{} gave birth! The baby is {} and {}. What should the baby be named?", mother.first, name(0), name(1)),
            1 => format!("{} gave birth! The baby is {}. What should the baby be named?", mother.first, name(0)),
            _ => format!("{} gave birth! What should the baby be named?", mother.first),
        },
    }
}

/// A newborn's name, as the player typed it (or the one suggested).
fn name_babies(mut answers: MessageReader<crate::dialog::Answered>, typed: Res<crate::dialog::Typed>, mut sims: Query<&mut Sim>, mut family: ResMut<crate::family::Genealogy>) {
    for a in answers.read() {
        if let crate::dialog::Question::NameBaby { sim, suggested } = &a.about
            && let Ok(mut s) = sims.get_mut(*sim)
        {
            let name = typed.0.trim();
            s.first = if name.is_empty() { suggested.clone() } else { name.chars().take(20).collect() };
            family.note(s.id, &s.full_name(), s.female);
        }
    }
}

/// Once named, a newborn whose last traits are the player's: the traits picker for them.
fn newborn_traits_picker(mut commands: Commands, mut picks: ResMut<NewbornPicks>, questions: Res<crate::dialog::Questions>, open: Option<Res<crate::midlife::TraitPicker>>, sims: Query<&Sim>) {
    if open.is_some() || !questions.queue.is_empty() {
        return;
    }
    while let Some(e) = picks.0.pop_front() {
        if let Ok(s) = sims.get(e) {
            commands.insert_resource(crate::midlife::TraitPicker::newborn(e, &s.traits));
            return;
        }
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
        (Has<Carried>, Has<crate::highchair::InHighChair>),
    )>,
    cribs: Query<(&GameObject, &Transform), Without<Sim>>,
    mut notes: ResMut<Notifications>,
) {
    let h = clock.hour_f();
    for (e, sim, mut motives, mut anim, crib, last_cry, mut tf, clip, bedtime, (carried, in_chair)) in &mut littles {
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
            // (In a high chair, they fuss there as its own clips have them: `highchair`.)
            if !crying && anim.pose != Pose::Lie && !in_chair {
                let cry = if sim.age == Age::Baby { CRY_BABY } else { CRY_TODDLER };
                commands.entity(e).insert(cry);
            }
            if last_cry.is_none_or(|c| clock.minutes - c.0 > 180.0) {
                notes.push(format!("{} {what}.", sim.first));
                commands.entity(e).insert(LastCry(clock.minutes));
            }
        } else if !in_chair && clip.is_some_and(|c| c.loops.first().is_some_and(|l| l.contains("cry"))) {
            commands.entity(e).remove::<crate::anim::ActionClip>();
        }
    }
}

const CRY_BABY: crate::anim::ActionClip = crate::anim::ActionClip::new(Some("b2o_crib_cry_start_y"), &["b2o_crib_cry_loop"]);
const CRY_TODDLER: crate::anim::ActionClip = crate::anim::ActionClip::new(Some("p2o_crib_cry_start_y"), &["p2o_crib_cry_loop"]);

#[cfg(test)]
mod birth_tests {
    use super::*;
    use crate::life::Trait;

    #[test]
    fn twins_and_triplets_come_as_the_games_odds_have_them() {
        assert_eq!(babies_born(0.5, false, 0.0, 6), 1);
        assert_eq!(babies_born(0.03, false, 0.0, 6), 2);
        assert_eq!(babies_born(0.005, false, 0.0, 6), 3);
        // (Two hours of children's TV: three times as likely.)
        assert_eq!(babies_born(0.12, false, 2.0, 6), 2);
        assert_eq!(babies_born(0.12, false, 0.0, 6), 1);
        let mut p = Pregnancy::new(0.0, None);
        for _ in 0..5 {
            p.ate("Watermelon");
        }
        assert!((p.girl_bias - MAX_GENDER_OFFSET).abs() < 1e-6);
        p.ate("Apple");
        p.ate("Tomato");
        assert!((p.girl_bias - 0.10).abs() < 1e-6);
        // (A Fertility Treatment: three and a half times as likely.)
        assert_eq!(babies_born(0.15, true, 0.0, 6), 2);
        assert_eq!(babies_born(0.03, true, 0.0, 6), 3);
        // (No more than the household has room for.)
        assert_eq!(babies_born(0.005, false, 0.0, 1), 1);
    }

    #[test]
    fn a_newborns_traits_come_of_the_mothers_mood() {
        assert_eq!(TraitOutcome::of(-60.0), TraitOutcome::Horrible);
        assert_eq!(TraitOutcome::of(-10.0), TraitOutcome::Bad);
        assert_eq!(TraitOutcome::of(20.0), TraitOutcome::Average);
        assert_eq!(TraitOutcome::of(40.0), TraitOutcome::Good);
        assert_eq!(TraitOutcome::of(80.0), TraitOutcome::Excellent);
        let bad = [Trait::Grumpy, Trait::Loser, Trait::Slob, Trait::Unlucky];
        let score = |t: Trait| (if bad.contains(&t) { -10 } else { 10 }, 10.0, true);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(3);
        // (Unhappy: both bad.)
        for _ in 0..20 {
            let (t, chosen) = newborn_traits(TraitOutcome::Horrible, &[], &score, &mut rng);
            assert_eq!((t.len(), chosen), (2, 0));
            assert!(t.iter().all(|x| bad.contains(x)), "{t:?}");
        }
        // (Happy: one drawn, one the player's; very happy: both the player's.)
        let (t, chosen) = newborn_traits(TraitOutcome::Good, &[Trait::Friendly], &score, &mut rng);
        assert_eq!((t.len(), chosen), (1, 1));
        let (t, chosen) = newborn_traits(TraitOutcome::Excellent, &[Trait::Friendly], &score, &mut rng);
        assert_eq!((t.len(), chosen), (0, 2));
        // (Ordinary: two, a parent's often enough.)
        let mut parents = 0;
        for _ in 0..200 {
            let (t, _) = newborn_traits(TraitOutcome::Average, &[Trait::Friendly], &score, &mut rng);
            assert_eq!(t.len(), 2);
            parents += t.contains(&Trait::Friendly) as usize;
        }
        assert!(parents > 60, "{parents}");
    }
}
