//! Thought and speech balloons over Sims' heads, drawn with the game's own balloon pictures and
//! chosen from its balloon table (`s3bake::gamedata::BalloonTable`): needs running low, new
//! moodlets and trait quirks as thoughts, dreams while asleep, and the topics of conversation as
//! speech balloons during socials (with the game's like / dislike marks).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;
use s3bake::gamedata::{BalloonEntry, BalloonTable};

use crate::PlayMode;
use crate::camera::SimsCamera;
use crate::interact::{ActionKind, ActionQueue, Phase, SOCIALS};
use crate::sim::{Age, Motives, Sim};

pub struct BalloonsPlugin;

impl Plugin for BalloonsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BalloonState>()
            .add_systems(Update, (balloon_triggers, place_balloons).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), clear_balloons);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BalloonKind {
    Thought,
    Speech,
    Dream,
}

/// Asks for a balloon over this Sim (from any system).
#[derive(Component, Clone)]
pub struct BalloonRequest {
    pub kind: BalloonKind,
    pub icon: String,
    /// 0 neutral, 1 like, 2 dislike.
    pub axis: u8,
}

impl BalloonRequest {
    pub fn thought(icon: &str) -> Self {
        Self { kind: BalloonKind::Thought, icon: icon.to_string(), axis: 0 }
    }
}

/// A balloon on screen.
#[derive(Component)]
struct BalloonUi {
    sim: Entity,
    kind: BalloonKind,
    born: f32,
    until: f32,
}

/// The pictures of a balloon (faded together).
#[derive(Component)]
struct BalloonPart;

/// What each Sim is thinking about next (real-time seconds).
#[derive(Default)]
struct SimBalloons {
    showing: Option<Entity>,
    until: f32,
    next_need: f32,
    next_idle: f32,
    next_talk: f32,
    talk_turn: u32,
    moodlets: Vec<crate::life::MoodletKind>,
    known: bool,
    /// The object the Sim last decided on by themselves (thought of once).
    decided: Option<Entity>,
}

#[derive(Resource, Default)]
struct BalloonState {
    sims: HashMap<Entity, SimBalloons>,
}

/// How long a balloon stays up (seconds).
const SHOW_SECS: f32 = 2.8;

/// The game's names for the needs' thought balloons, in need order.
const NEED_KEYS: [&str; 6] = ["MotiveHunger", "MotiveBladder", "MotiveEnergy", "MotiveSocial", "MotiveHygiene", "MotiveFun"];

/// Base-game dream sets (good dreams for a Sim in a good mood, bad ones otherwise).
const DREAMS: [&str; 12] = [
    "DreamEnergy",
    "DreamHunger",
    "DreamSocial",
    "DreamHygiene",
    "DreamBladder",
    "DreamRomanceTraits",
    "DreamFinanceTraits",
    "DreamFamilyTraits",
    "DreamHumorTraits",
    "DreamWealth",
    "DreamCareerAndProfession",
    "DreamLifetimeHappiness",
];

/// Things Sims chat about (topics from the game's table), besides their work and traits.
const CHAT_TOPICS: [&str; 11] = ["Weather", "Art", "Books", "Movies", "Great Outdoors", "Music", "Party", "Money", "Computers", "Workout", "RandomGood"];

/// Socials where both Sims take turns talking.
const CONVERSATIONS: [&str; 6] = ["Chat", "Get to Know", "Talk About Hobbies", "Argue", "Flirt", "Propose Marriage"];

/// What a social's balloons are, in the game's table: its own row, or the nearest the game has.
fn social_list(table: &BalloonTable, name: &str) -> Option<Vec<BalloonEntry>> {
    let entry = |icon: &str, refkey: &str| BalloonEntry { icon: icon.into(), refkey: refkey.into(), axis: 0, weight: 1.0 };
    let key = match name {
        "Get to Know" | "Talk About Hobbies" => "Share Interests",
        "Do Funny Impression" => "Tell Joke",
        "Ask to Go Steady" => "Propose Going Steady",
        "Put to Bed" => "Request Go to Bed",
        // The little ones' care, with the game's pictures for it.
        "Feed" => return Some(vec![entry("balloon_babybottle", "")]),
        "Change Diaper" => return Some(vec![entry("balloon_smelly", "")]),
        "Play With" => return Some(vec![entry("", "RandomToddler")]),
        "Read to" => return Some(vec![entry("", "Books")]),
        n => n,
    };
    table.social.get(key).cloned()
}

/// The game's conversation topic for a career.
fn career_topic(table: &BalloonTable, career: &str) -> Option<String> {
    let direct = format!("Career {career}");
    if table.topic.contains_key(&direct) {
        return Some(direct);
    }
    let c = career.to_ascii_lowercase();
    let key = [
        ("sport", "Career Sports"),
        ("law", "Career Law Enforcement"),
        ("police", "Career Law Enforcement"),
        ("book", "PT Bookstore"),
        ("spa", "PT Day Spa"),
        ("grocer", "PT Grocery Store"),
        ("mausoleum", "PT Mausoleum"),
        ("medic", "Career Medical"),
        ("politic", "Career Political"),
        ("crimin", "Career Criminal"),
        ("culin", "Career Culinary"),
        ("journal", "Career Journalism"),
        ("milit", "Career Military"),
        ("music", "Career Music"),
        ("scien", "Career Science"),
        ("business", "Career Business"),
        ("school", "Career School"),
    ]
    .iter()
    .find(|(k, _)| c.contains(k))?
    .1;
    table.topic.contains_key(key).then(|| key.to_string())
}

/// Who a balloon is about.
struct Who<'a> {
    entity: Entity,
    sim: &'a Sim,
    career: Option<&'a str>,
}

/// The icon name standing for a Sim's portrait.
const PORTRAIT: &str = "@portrait:";

struct Picker<'a> {
    table: &'a BalloonTable,
    actor: Who<'a>,
    target: Option<Who<'a>>,
}

impl Picker<'_> {
    /// A weighted pick from a list, followed through references and the game's special pickers
    /// to an icon name and its like / dislike mark.
    fn resolve(&self, list: &[BalloonEntry], depth: u32) -> Option<(String, u8)> {
        if depth > 4 || list.is_empty() {
            return None;
        }
        let total: f32 = list.iter().map(|e| e.weight.max(0.01)).sum();
        let mut r = rand::rng().random_range(0.0..total);
        let e = list.iter().find(|e| {
            r -= e.weight.max(0.01);
            r <= 0.0
        })?;
        let with_axis = |(icon, axis): (String, u8)| (icon, if e.axis != 0 { e.axis } else { axis });
        if !e.refkey.is_empty() {
            return self.resolve(self.table.list(&e.refkey)?, depth + 1).map(with_axis);
        }
        match e.icon.as_str() {
            "GetSpeechBalloonImageForChat" => self.chat(&self.actor, depth).map(with_axis),
            "Thumbnail Target" => self.about(self.target.as_ref().unwrap_or(&self.actor), depth).map(with_axis),
            "Thumbnail Actor" => self.about(&self.actor, depth).map(with_axis),
            "Actor Career Topic" | "GetSpeechBalloonIconForCareer" => self.work(&self.actor, depth).map(with_axis),
            "Target Career Topic" | "GetSpeechBalloonIconForTargetCareer" => self.work(self.target.as_ref().unwrap_or(&self.actor), depth).map(with_axis),
            icon => Some((icon.to_string(), e.axis)),
        }
    }

    fn key(&self, key: &str, depth: u32) -> Option<(String, u8)> {
        self.resolve(self.table.list(key)?, depth + 1)
    }

    /// Small talk: the weather, interests, the Sim's work or one of their traits.
    fn chat(&self, who: &Who, depth: u32) -> Option<(String, u8)> {
        let mut keys: Vec<String> = CHAT_TOPICS.iter().map(|s| s.to_string()).collect();
        if let Some(t) = who.career.and_then(|c| career_topic(self.table, c)) {
            keys.extend([t.clone(), t]);
        }
        keys.extend(who.sim.traits.iter().map(|t| format!("Trait{}", t.game_id())).filter(|k| self.table.idle.contains_key(k)));
        let k = &keys[rand::rng().random_range(0..keys.len())];
        self.key(k, depth)
    }

    /// A Sim's picture.
    fn about(&self, who: &Who, _depth: u32) -> Option<(String, u8)> {
        Some((format!("{PORTRAIT}{}", who.entity.to_bits()), 0))
    }

    fn work(&self, who: &Who, depth: u32) -> Option<(String, u8)> {
        match who.career.and_then(|c| career_topic(self.table, c)) {
            Some(t) => self.key(&t, depth),
            None => self.key("Random", depth),
        }
    }
}

/// How high above a Sim's feet their balloons start (lower when sitting or lying down).
fn head_height(age: Age, pose: crate::sim::Pose) -> f32 {
    let standing = match age {
        Age::Baby => 0.7,
        Age::Toddler => 0.95,
        Age::Child => 1.45,
        Age::Teen => 1.9,
        _ => 2.05,
    };
    match pose {
        crate::sim::Pose::Lie if !age.is_little() => 1.15,
        crate::sim::Pose::Sit => standing * 0.75,
        _ => standing,
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn balloon_triggers(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<crate::clock::GameClock>,
    mut state: ResMut<BalloonState>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    (mut images, mut portraits): (ResMut<Assets<Image>>, ResMut<crate::portraits::Portraits>),
    sims: Query<(
        Entity,
        &Sim,
        &Motives,
        &crate::life::Moodlets,
        &crate::life::Mood,
        &ActionQueue,
        Option<&crate::careers::Job>,
        &InheritedVisibility,
    )>,
    requests: Query<(Entity, &BalloonRequest)>,
    objects: Query<&crate::interact::GameObject>,
) {
    let Some(mut ui) = ui else { return };
    let now = time.elapsed_secs();
    let data = ui.data.clone();
    let table = &data.balloons;
    state.sims.retain(|e, _| sims.contains(*e));
    let careers: HashMap<Entity, &str> = sims.iter().filter_map(|q| Some((q.0, q.6?.career().name))).collect();
    let by_entity: HashMap<Entity, &Sim> = sims.iter().map(|q| (q.0, q.1)).collect();
    // (sim, kind, icon, axis, replaces a balloon already up)
    let mut show: Vec<(Entity, BalloonKind, String, u8, bool)> = Vec::new();

    for (e, req) in &requests {
        commands.entity(e).remove::<BalloonRequest>();
        show.push((e, req.kind, req.icon.clone(), req.axis, true));
    }

    if clock.speed > 0 {
        let mut rng = rand::rng();
        for (me, sim, motives, moodlets, mood, queue, job, vis) in &sims {
            let st = state.sims.entry(me).or_default();
            let kinds: Vec<_> = moodlets.0.iter().map(|m| m.kind).collect();
            if !st.known {
                // (Moodlets a Sim already has when play starts don't count as news.)
                st.known = true;
                st.moodlets = kinds;
                st.next_need = now + rng.random_range(2.0..8.0);
                st.next_idle = now + rng.random_range(10.0..30.0);
                continue;
            }
            if !vis.get() {
                st.moodlets = kinds;
                continue;
            }
            let busy = st.showing.is_some() && now < st.until;
            let picker = Picker { table, actor: Who { entity: me, sim, career: job.map(|j| j.career().name) }, target: None };
            let front = queue.0.front();

            // Socials: the topic of conversation, both Sims taking turns.
            if let Some(a) = front
                && let (ActionKind::Social { target, social }, Phase::Running(_)) = (&a.kind, a.phase)
            {
                if now >= st.next_talk {
                    let s = &SOCIALS[*social];
                    let talks_back = CONVERSATIONS.contains(&s.name);
                    let their_turn = talks_back && st.talk_turn % 2 == 1;
                    st.talk_turn += 1;
                    st.next_talk = now + SHOW_SECS + 0.5;
                    if let (Some(list), Some(tsim)) = (social_list(table, s.name), by_entity.get(target)) {
                        let (speaker, listener) = if their_turn { (*target, me) } else { (me, *target) };
                        let p = Picker {
                            table,
                            actor: Who { entity: speaker, sim: if their_turn { tsim } else { sim }, career: careers.get(&speaker).copied() },
                            target: Some(Who { entity: listener, sim: if their_turn { sim } else { tsim }, career: careers.get(&listener).copied() }),
                        };
                        if let Some((icon, axis)) = p.resolve(&list, 0) {
                            show.push((speaker, BalloonKind::Speech, icon, axis, true));
                        }
                    }
                }
                st.moodlets = kinds;
                continue;
            }
            st.talk_turn = 0;
            st.next_talk = now + 0.6;

            // Asleep: dreams.
            let asleep = front.is_some_and(|a| matches!(a.phase, Phase::Running(_)) && (a.label.contains("Sleep") || a.label.contains("Nap")));
            if asleep {
                if !busy && now >= st.next_idle {
                    st.next_idle = now + rng.random_range(7.0..13.0);
                    let set = format!("{}{}Balloons", DREAMS[rng.random_range(0..DREAMS.len())], if mood.0 >= 0.0 { "Good" } else { "Bad" });
                    if let Some((icon, axis)) = picker.key(&set, 0) {
                        show.push((me, BalloonKind::Dream, icon, axis, false));
                    }
                }
                st.moodlets = kinds;
                continue;
            }

            // Deciding on something by themselves: a thought of it (the game's picture of the
            // object), now and then.
            if let Some(a) = front
                && let (ActionKind::Object { target, .. }, true) = (&a.kind, a.autonomous)
                && st.decided != Some(*target)
            {
                st.decided = Some(*target);
                if !busy
                    && rng.random_bool(0.7)
                    && let Ok(obj) = objects.get(*target)
                {
                    show.push((me, BalloonKind::Thought, s3bake::gamedata::thumb_name(obj.objd.2), 0, false));
                    st.moodlets = kinds;
                    continue;
                }
            }

            // A new moodlet.
            let fresh: Vec<_> = kinds.iter().filter(|k| !st.moodlets.contains(k)).copied().collect();
            st.moodlets = kinds;
            if !busy && let Some(k) = fresh.iter().find(|k| table.idle.contains_key(&format!("Buff{}", k.buff().0))) {
                if let Some((icon, axis)) = picker.key(&format!("Buff{}", k.buff().0), 0) {
                    show.push((me, BalloonKind::Thought, icon, axis, false));
                    st.next_need = now + rng.random_range(6.0..10.0);
                    continue;
                }
            }

            // A need running low, while the Sim isn't seeing to anything yet.
            let idle = front.is_none_or(|a| a.autonomous && !matches!(a.phase, Phase::Running(_)));
            if !busy && idle && now >= st.next_need {
                st.next_need = now + rng.random_range(7.0..12.0);
                let (low, value) = motives.0.iter().enumerate().fold((0, f32::MAX), |b, (i, v)| if *v < b.1 { (i, *v) } else { b });
                if value < 30.0 && !(sim.age == Age::Baby && low == crate::sim::SOCIAL) {
                    if let Some((icon, axis)) = picker.key(NEED_KEYS[low], 0) {
                        show.push((me, BalloonKind::Thought, icon, axis, false));
                        continue;
                    }
                }
            }

            // Now and then, a thought that goes with one of their traits.
            if !busy && front.is_none() && now >= st.next_idle {
                st.next_idle = now + rng.random_range(18.0..40.0);
                let keys: Vec<String> = sim.traits.iter().map(|t| format!("Trait{}", t.game_id())).filter(|k| table.idle.contains_key(k)).collect();
                if !keys.is_empty() && rng.random_bool(0.6) {
                    if let Some((icon, axis)) = picker.key(&keys[rng.random_range(0..keys.len())], 0) {
                        show.push((me, BalloonKind::Thought, icon, axis, false));
                    }
                }
            }
        }
    }

    for (sim, kind, icon, axis, replace) in show {
        let st = state.sims.entry(sim).or_default();
        if let Some(old) = st.showing {
            if !replace && now < st.until {
                continue;
            }
            commands.entity(old).try_despawn();
        }
        let dur = if kind == BalloonKind::Dream { SHOW_SECS + 1.0 } else { SHOW_SECS };
        st.showing = spawn_balloon(&mut commands, &mut ui, &mut images, &mut portraits, sim, kind, &icon, axis, now, dur);
        st.until = now + dur;
    }
}

/// A box in balloon units (the balloon's width = 1), as percentages of a root `h` tall.
fn rect(x: f32, y: f32, w: f32, hh: f32, h: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(x * 100.0),
        top: Val::Percent(y / h * 100.0),
        width: Val::Percent(w * 100.0),
        height: Val::Percent(hh / h * 100.0),
        ..default()
    }
}

/// The root's height in balloon widths: thought clouds have their lead bubbles below.
fn root_height(kind: BalloonKind) -> f32 {
    if kind == BalloonKind::Speech { 1.0 } else { 1.3 }
}

/// Where the balloon points (the Sim's head), in balloon units from its root's top left.
fn anchor(kind: BalloonKind) -> Vec2 {
    if kind == BalloonKind::Speech { Vec2::new(0.625, 0.91) } else { Vec2::new(0.33, 1.27) }
}

#[allow(clippy::too_many_arguments)]
fn spawn_balloon(
    commands: &mut Commands,
    ui: &mut crate::icons::GameUi,
    images: &mut Assets<Image>,
    portraits: &mut crate::portraits::Portraits,
    sim: Entity,
    kind: BalloonKind,
    icon: &str,
    axis: u8,
    now: f32,
    dur: f32,
) -> Option<Entity> {
    // A Sim's picture (rounded into the balloon), or an icon.
    let picture = icon.strip_prefix(PORTRAIT).and_then(|b| b.parse::<u64>().ok()).and_then(Entity::try_from_bits);
    let icon_h = match picture {
        Some(e) => portraits.portrait(images, e),
        None => ui.icon(images, icon)?,
    };
    let frame = ui.icon(
        images,
        match kind {
            BalloonKind::Thought => "thought_balloon",
            BalloonKind::Dream => "thought_balloon2",
            BalloonKind::Speech => "speech_balloon",
        },
    )?;
    let lead = ui.icon(images, "thought_balloonLead");
    let mark = match axis {
        1 => ui.icon(images, "sb_like"),
        2 => ui.icon(images, "sb_dislike"),
        _ => None,
    };
    let h = root_height(kind);
    let part = |img: Handle<Image>, node: Node| (ImageNode::new(img), node, BalloonPart, Pickable::IGNORE);
    let root = commands
        .spawn((
            BalloonUi { sim, kind, born: now, until: now + dur },
            Node { position_type: PositionType::Absolute, display: Display::None, ..default() },
            GlobalZIndex(-5),
            Pickable::IGNORE,
        ))
        .with_children(|r| {
            let icon_y = if kind == BalloonKind::Speech { 0.15 } else { 0.24 };
            if kind != BalloonKind::Speech
                && let Some(l) = lead
            {
                r.spawn(part(l.clone(), rect(0.315, 0.895, 0.17, 0.17, h)));
                r.spawn(part(l, rect(0.28, 1.12, 0.1, 0.1, h)));
            }
            r.spawn(part(frame, rect(0.0, 0.0, 1.0, 1.0, h)));
            if picture.is_some() {
                let mut n = rect(0.27, icon_y + 0.02, 0.46, 0.46, h);
                n.border_radius = BorderRadius::all(Val::Percent(50.0));
                r.spawn((ImageNode::new(icon_h), n, BalloonPart, Pickable::IGNORE));
            } else {
                r.spawn(part(icon_h, rect(0.25, icon_y, 0.5, 0.5, h)));
            }
            match (axis, mark) {
                (2, Some(m)) => {
                    r.spawn(part(m, rect(0.22, icon_y - 0.03, 0.56, 0.56, h)));
                }
                (1, Some(m)) => {
                    r.spawn(part(m, rect(0.6, icon_y + 0.32, 0.22, 0.22, h)));
                }
                _ => {}
            }
        })
        .id();
    Some(root)
}

/// Keeps each balloon over its Sim's head, sized for the camera's distance, popping in and
/// fading out.
#[allow(clippy::type_complexity)]
fn place_balloons(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<BalloonState>,
    cams: Query<(&Camera, &GlobalTransform), With<SimsCamera>>,
    sims: Query<(&Sim, &crate::sim::SimAnim, &GlobalTransform, &InheritedVisibility, Has<crate::sim::Selected>)>,
    mut balloons: Query<(Entity, &BalloonUi, &mut Node)>,
    mut parts: Query<(&ChildOf, &mut ImageNode), With<BalloonPart>>,
) {
    let now = time.elapsed_secs();
    let Ok((cam, cam_tf)) = cams.single() else { return };
    let right = cam_tf.right().as_vec3();
    let mut alpha: HashMap<Entity, f32> = HashMap::new();
    for (e, b, mut node) in &mut balloons {
        if now >= b.until {
            commands.entity(e).despawn();
            if let Some(st) = state.sims.get_mut(&b.sim)
                && st.showing == Some(e)
            {
                st.showing = None;
            }
            continue;
        }
        let shown = sims.get(b.sim).ok().filter(|(_, _, _, v, _)| v.get()).and_then(|(sim, anim, tf, _, selected)| {
            // (Above the plumbob of the Sim being played.)
            let head = tf.translation() + Vec3::Y * (head_height(sim.age, anim.pose) + if selected { 0.55 } else { 0.0 });
            let p = cam.world_to_viewport(cam_tf, head).ok()?;
            let q = cam.world_to_viewport(cam_tf, head + right).ok()?;
            // In front of the camera only.
            ((head - cam_tf.translation()).dot(cam_tf.forward().as_vec3()) > 0.3).then_some((p, p.distance(q)))
        });
        let Some((at, per_metre)) = shown else {
            node.display = Display::None;
            continue;
        };
        let t = now - b.born;
        let pop = (t / 0.18).min(1.0);
        let pop = 0.55 + 0.45 * (1.0 - (1.0 - pop) * (1.0 - pop));
        let size = (per_metre * 0.85).clamp(46.0, 118.0) * pop;
        let a = anchor(b.kind) * size;
        node.display = Display::Flex;
        node.width = Val::Px(size);
        node.height = Val::Px(size * root_height(b.kind));
        node.left = Val::Px(at.x - a.x);
        node.top = Val::Px(at.y - a.y);
        alpha.insert(e, (t / 0.12).min(1.0).min((b.until - now) / 0.35).clamp(0.0, 1.0));
    }
    for (parent, mut img) in &mut parts {
        if let Some(a) = alpha.get(&parent.parent()) {
            img.color = Color::srgba(1.0, 1.0, 1.0, *a);
        }
    }
}

fn clear_balloons(mut commands: Commands, balloons: Query<Entity, With<BalloonUi>>, mut state: ResMut<BalloonState>) {
    for e in &balloons {
        commands.entity(e).despawn();
    }
    state.sims.clear();
}
