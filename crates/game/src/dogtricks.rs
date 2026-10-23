//! Dog tricks, the Pets pack's (`TrickSkill`, the `TeachTrick` and `CommandTrick` socials): a
//! teen or older of the household teaches a grown dog of theirs its tricks ("Teach Trick..." on
//! its menu: Sit, Sit Up, Speak, Shake, Roll Over, Play Dead, each from the dog's Tricks level the
//! game's tuning asks: `kTrickSkillLevelRequired` 0 to 5). Teaching goes round the pack's
//! `DogTricks` clips, both sides together: the trick shown (`_demo`), the dog trying it (getting
//! it, more often the further along it is, `IsPracticeTrySuccessfull`, or not: `_fail`), a
//! breath, again; each minute of it is practice (`kBasePracticePointsPerSimMinute`, more for a
//! happy dog that likes its teacher, a Genius dog, a Dog Person teaching; less for a Clueless
//! one; a tenth more from a best friend: `kTrickBFFMultiplier`) till it has the trick
//! (`kPracticePointsNeededToLearn`: 150 to 600, by trick), when it's learned ("Rover learned to
//! Sit!"). The dog learns the Tricks skill as it goes (the `LearnTrick` social's `SkillTrick`, 100
//! points an hour; 25 doing a trick asked for) by the skill's own levels, and tires of it (the
//! pack's `PetSkillFatigue`: four hours' learning, sooner for a Lazy dog, later a Hyper one) and
//! has to have a break (four hours) before it'll learn more ("Rover needs a break from learning
//! new skills!"). "Tricks..." asks it for one it knows; teaching and asking each make them fonder
//! of each other (`kLTRGain`). What a dog knows is kept in saves.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{ActionKind, ActionQueue, Notifications, Phase};
use crate::pets::{Pet, PetAnim, PetBonds, PetBusy};
use crate::sim::Sim;

pub struct DogTricksPlugin;

impl Plugin for DogTricksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tricks_test, train, rest).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// A trick: its name, the clips' word for it, the Tricks level it takes, and the practice to learn
/// it (the pack's tuning).
pub struct Trick {
    pub name: &'static str,
    state: &'static str,
    pub level: u32,
    pub points: f32,
}

pub const TRICKS: [Trick; 6] = [
    Trick { name: "Sit", state: "sit", level: 0, points: 150.0 },
    Trick { name: "Sit Up", state: "sitUp", level: 1, points: 300.0 },
    Trick { name: "Speak", state: "speak", level: 2, points: 300.0 },
    Trick { name: "Shake", state: "shake", level: 3, points: 450.0 },
    Trick { name: "Roll Over", state: "rollOver", level: 4, points: 450.0 },
    Trick { name: "Play Dead", state: "playDead", level: 5, points: 600.0 },
];

/// The menus' words.
pub const TEACH: &str = "Teach Trick...";
pub const COMMAND: &str = "Tricks...";

/// The Tricks skill's points to each level (its `Skills` row: five levels).
const LEVELS: [f32; 5] = [160.0, 490.0, 820.0, 1320.0, 1820.0];
/// Tricks skill points an hour learning (`LearnTrick`'s `SkillTrick`), and doing one asked for.
const LEARN_POINTS: f32 = 100.0;
const COMMAND_POINTS: f32 = 25.0;
/// The practice tuning: base a minute, at best for mood and for liking, a Genius dog's more, a
/// Clueless one's less, a Dog Person teacher's more, a best friend's tenth (well, twentieth) more.
const BASE: f32 = 1.0;
const MOOD_BONUS: f32 = 0.1;
const LIKING_BONUS: f32 = 0.1;
const GENIUS_BONUS: f32 = 0.1;
const CLUELESS_PENALTY: f32 = 0.2;
const DOG_PERSON_BONUS: f32 = 0.1;
const BFF: f32 = 1.05;
/// Liking from teaching or a trick done (`kLTRGain`).
const LIKING_GAIN: f32 = 10.0;
/// `PetSkillFatigue`: from rested to worn out over four hours' learning (the `Trick` row), back
/// over four hours' rest; worn out from -98, rested from 98; Lazy dogs half as fast again, Hyper
/// (and Fast, Agile) ones three quarters (`TraitTuning`).
const FATIGUE_HOURS: f32 = 4.0;
const REST_HOURS: f32 = 4.0;

/// What a dog knows of its tricks: each one's practice, whether it's learned, how often it's been
/// done; its Tricks skill points; and how fresh it is for learning (100 rested, -100 worn out).
#[derive(Component, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DogTricks {
    pub practice: [f32; 6],
    pub learned: [bool; 6],
    pub performed: [u32; 6],
    pub skill: f32,
    pub fresh: f32,
    pub worn_out: bool,
}

impl Default for DogTricks {
    fn default() -> Self {
        Self { practice: [0.0; 6], learned: [false; 6], performed: [0; 6], skill: 0.0, fresh: 100.0, worn_out: false }
    }
}

impl DogTricks {
    /// Its Tricks level (0 to 5).
    pub fn level(&self) -> u32 {
        LEVELS.iter().take_while(|l| self.skill >= **l).count() as u32
    }

    /// The tricks it could be taught now: not known yet, and its level enough for them.
    pub fn learnable(&self) -> Vec<usize> {
        (0..TRICKS.len()).filter(|i| !self.learned[*i] && TRICKS[*i].level <= self.level()).collect()
    }

    pub fn known(&self) -> Vec<usize> {
        (0..TRICKS.len()).filter(|i| self.learned[*i]).collect()
    }

    /// A try at a trick in practice: more likely the further along it is.
    pub fn tries(&self, i: usize, roll: f32) -> bool {
        roll < self.practice[i] / TRICKS[i].points
    }

    /// Practice at a trick for so many minutes; true once it's learned.
    pub fn practise(&mut self, i: usize, minutes: f32, rate: f32) -> bool {
        self.practice[i] += minutes * rate;
        if self.practice[i] >= TRICKS[i].points {
            self.learned[i] = true;
        }
        self.learned[i]
    }
}

/// How much a minute's practice is worth, for the dog and its teacher: its mood (-100 to 100), how
/// it likes them, its traits, and theirs.
pub fn practice_rate(mood: f32, liking: f32, traits: &[String], dog_person: bool) -> f32 {
    let has = |t: &str| traits.iter().any(|x| x == t);
    let mut r = BASE + (mood / 100.0) * MOOD_BONUS;
    if liking > 0.0 {
        r += liking / 100.0 * LIKING_BONUS;
    }
    if has("CluelessPet") {
        r -= CLUELESS_PENALTY;
    }
    if has("GeniusPet") {
        r += GENIUS_BONUS;
    }
    if dog_person {
        r += DOG_PERSON_BONUS;
    }
    if liking >= 80.0 {
        r *= BFF;
    }
    r.max(0.1)
}

/// How fast a dog wears out learning, by its traits (`TraitTuning`'s pet skill fatigue).
fn wear_rate(traits: &[String]) -> f32 {
    let has = |t: &[&str]| traits.iter().any(|x| t.contains(&x.as_str()));
    match (has(&["LazyPet", "LazyHorse", "HatesJumpingPet"]), has(&["HyperPet", "FastPet", "AgilePet"])) {
        (true, false) => 1.5,
        (false, true) => 0.75,
        _ => 1.0,
    }
}

/// A dog's tricks for its tooltip: its Tricks level and what it knows.
pub fn summary(t: &DogTricks) -> String {
    let known: Vec<&str> = t.known().into_iter().map(|i| TRICKS[i].name).collect();
    if known.is_empty() && t.skill <= 0.0 {
        return String::new();
    }
    let tired = if t.worn_out { " (needs a break)" } else { "" };
    format!("\nTricks level {}{tired}: {}", t.level(), if known.is_empty() { "none yet".to_string() } else { known.join(", ") })
}

/// Whether a pet's a dog grown enough for tricks.
pub fn trick_dog(kind: &str) -> bool {
    matches!(kind, "ad" | "al")
}

/// A clip of the pack's trick set, for a dog of a kind, either side.
fn clip(kind: &str, part: &str, side: char) -> &'static str {
    crate::pets::intern(format!("a2{kind}_soc_dogTricks_{part}_{side}"))
}

/// A teaching or a trick asked for, under way: which part's playing, since when (seconds of its
/// clip), how long it is.
#[derive(Component)]
pub struct TrickSession {
    dog: Entity,
    trick: usize,
    teach: bool,
    part: Part,
    time: f32,
    length: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Part {
    Start,
    Demo,
    Try(bool),
    Breathe,
    Stop,
}

impl TrickSession {
    pub fn new(dog: Entity, trick: usize, teach: bool) -> Self {
        Self { dog, trick, teach, part: Part::Start, time: 0.0, length: 0.0 }
    }
}

/// The clips' words for a part of a trick's set.
fn part_name(trick: usize, part: Part) -> String {
    let s = TRICKS[trick].state;
    let shake = s == "shake";
    let roll = s == "rollOver";
    match part {
        Part::Start if shake => "start2Shake".into(),
        Part::Start if roll => "start2RollOver".into(),
        Part::Start => "start1".into(),
        Part::Stop if shake => "stop2Shake".into(),
        Part::Stop if roll => "stop2RollOver".into(),
        Part::Stop => "stop1".into(),
        Part::Breathe if shake => "shakeBreathe".into(),
        Part::Breathe => "breathe".into(),
        Part::Demo => format!("{s}_demo"),
        Part::Try(true) => format!("{s}_success_command"),
        Part::Try(false) => format!("{s}_fail"),
    }
}

/// Teaching and tricks: both sides of each part played together, practice counted, the trick
/// learned, the dog wearing out, and the end of it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn train(
    mut commands: Commands,
    (time, clock, delta): (Res<Time>, Res<crate::clock::GameClock>, Res<crate::clock::SimDelta>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Transform, Option<&mut TrickSession>), Without<Pet>>,
    mut dogs: Query<(&Pet, &mut Transform, &mut PetAnim, Option<&mut DogTricks>, Option<&mut PetBonds>, Option<&crate::pets::PetTraits>, Option<&crate::petneeds::PetMotives>), Without<Sim>>,
    mut notes: ResMut<Notifications>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let minutes = delta.0;
    let mut rng = rand::rng();
    for (e, sim, mut queue, mut stf, session) in &mut sims {
        let running = queue.0.front().and_then(|a| match a.kind {
            ActionKind::DogTrick { target, trick, teach, .. } if matches!(a.phase, Phase::Running(_)) && !a.cancel => Some((target, trick as usize, teach)),
            _ => None,
        });
        let Some(mut s) = session else {
            // (Arrived: the dog stops and turns to them, and they begin.)
            if let Some((dog, trick, teach)) = running {
                commands.entity(e).insert(TrickSession::new(dog, trick, teach));
                commands.entity(dog).insert(PetBusy).remove::<crate::petneeds::PetTask>();
            }
            continue;
        };
        let Ok((pet, mut dtf, mut anim, tricks, mut bonds, traits, motives)) = dogs.get_mut(s.dog) else {
            commands.entity(e).remove::<TrickSession>();
            continue;
        };
        // (Off to something else, or told to stop: done, the dog freed.)
        if running.is_none_or(|r| r.0 != s.dog) {
            commands.entity(e).remove::<TrickSession>();
            commands.entity(s.dog).remove::<PetBusy>();
            anim.play(format!("{}_idle_stand_breathe_x", pet.kind));
            continue;
        }
        let Some(mut tricks) = tricks else {
            commands.entity(s.dog).insert(DogTricks::default());
            continue;
        };
        let to = stf.translation.xz() - dtf.translation.xz();
        if to.length() > 0.05 {
            dtf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
            stf.rotation = Quat::from_rotation_y((-to).x.atan2((-to).y));
        }
        let traits = traits.map_or(&[][..], |t| &t.0[..]);
        let liking = bonds.as_deref().map_or(crate::pets::FAMILY_LIKING, |b| b.liking(sim.id, true));
        // What the time's worth: practice, the skill, and the dog's freshness.
        if minutes > 0.0 && s.part != Part::Start && s.part != Part::Stop {
            let h = minutes / 60.0;
            let points = if s.teach { LEARN_POINTS } else { COMMAND_POINTS };
            tricks.skill += points * h;
            tricks.fresh = (tricks.fresh - 200.0 / FATIGUE_HOURS * wear_rate(traits) * h).max(-100.0);
            if tricks.fresh <= -98.0 {
                tricks.worn_out = true;
            }
            if s.teach && !tricks.learned[s.trick] {
                let mood = motives.map_or(0.0, crate::petneeds::mood);
                let rate = practice_rate(mood, liking, traits, sim.traits.contains(&crate::life::Trait::DogPerson));
                if tricks.practise(s.trick, minutes, rate) {
                    notes.push(format!("{} learned to {}!", pet.name, TRICKS[s.trick].name));
                    info!("dog tricks: {} learned {}", pet.name, TRICKS[s.trick].name);
                }
            }
        }
        // The next part, once this one's played.
        s.time += dt;
        if s.time < s.length {
            continue;
        }
        let next = match (s.part, s.teach) {
            _ if s.length == 0.0 => Part::Start,
            (Part::Start, true) => Part::Demo,
            (Part::Start, false) => Part::Try(true),
            (Part::Demo, _) => Part::Try(tricks.tries(s.trick, rng.random())),
            (Part::Try(_), false) => Part::Stop,
            (Part::Try(_), true) => Part::Breathe,
            // (Learned, or worn out: that's it.)
            (Part::Breathe, _) if tricks.learned[s.trick] || tricks.worn_out => Part::Stop,
            (Part::Breathe, _) => Part::Demo,
            (Part::Stop, _) => {
                // Over: fonder of each other, a trick done counted.
                if let Some(b) = bonds.as_deref_mut() {
                    b.add(sim.id, true, LIKING_GAIN);
                }
                if !s.teach {
                    tricks.performed[s.trick] += 1;
                } else if tricks.worn_out && !tricks.learned[s.trick] {
                    notes.push(format!("{} needs a break from learning new skills!", pet.name));
                }
                queue.0.pop_front();
                commands.entity(e).remove::<(TrickSession, ActionClip)>();
                commands.entity(s.dog).remove::<PetBusy>();
                anim.play(format!("{}_idle_stand_breathe_x", pet.kind));
                continue;
            }
        };
        let (x, y) = (clip(&pet.kind, &part_name(s.trick, next), 'x'), clip(&pet.kind, &part_name(s.trick, next), 'y'));
        s.part = next;
        s.time = 0.0;
        s.length = lib.get(&data, x).map_or(2.0, |c| c.duration.max(0.3));
        let clips: &'static [&'static str] = Box::leak(Box::new([x]));
        commands.entity(e).insert(ActionClip::new(None, clips));
        anim.replay(y.to_string());
    }
}

/// Worn-out dogs rest: fresh again after four hours away from learning.
fn rest(delta: Res<crate::clock::SimDelta>, mut dogs: Query<(Entity, &mut DogTricks)>, sessions: Query<&TrickSession>) {
    let h = delta.0 / 60.0;
    if h <= 0.0 {
        return;
    }
    let busy: Vec<Entity> = sessions.iter().map(|s| s.dog).collect();
    for (e, mut t) in &mut dogs {
        if busy.contains(&e) {
            continue;
        }
        if t.fresh < 100.0 {
            t.fresh = (t.fresh + 200.0 / REST_HOURS * h).min(100.0);
            if t.fresh >= 98.0 {
                t.worn_out = false;
            }
        }
    }
}

/// `TRICKS_TEST=<dog>:<trick n>[:command]` (tests): eight seconds in, the selected Sim teaches (or
/// asks for) that trick of the household's dog of that name, the camera on them. `TRICKS_KNOWN=<n>`:
/// the dog knowing the first n tricks already.
fn tricks_test(
    mut commands: Commands,
    time: Res<Time>,
    mut done: Local<bool>,
    mut sel: Query<&mut ActionQueue, With<crate::sim::Selected>>,
    dogs: Query<(Entity, &Pet, &Transform), With<crate::pets::HomePet>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
) {
    let Ok(v) = std::env::var("TRICKS_TEST") else { return };
    let p: Vec<&str> = v.split(':').collect();
    let (Some(name), Some(n)) = (p.first(), p.get(1).and_then(|n| n.parse::<u8>().ok())) else { return };
    let Some((dog, pet, tf)) = dogs.iter().find(|(_, d, _)| d.name.eq_ignore_ascii_case(name)) else { return };
    if let Ok(mut c) = cams.single_mut() {
        c.look_at(tf.translation);
        c.distance = 4.0;
    }
    if *done || time.elapsed_secs() < 8.0 {
        return;
    }
    let Ok(mut q) = sel.single_mut() else { return };
    *done = true;
    let teach = p.get(2) != Some(&"command");
    let mut t = DogTricks::default();
    if let Some(k) = std::env::var("TRICKS_KNOWN").ok().and_then(|k| k.parse::<usize>().ok()) {
        for i in 0..k.min(6) {
            t.learned[i] = true;
            t.practice[i] = TRICKS[i].points;
        }
        t.skill = LEVELS[k.clamp(1, 5) - 1];
    }
    commands.entity(dog).insert(t);
    let kind: &'static str = if pet.kind == "al" { "al" } else { "ad" };
    info!("tricks test: {} {} with {}", if teach { "teaching" } else { "asking for" }, TRICKS[n as usize % 6].name, pet.name);
    q.push_player(crate::interact::Action::new(TRICKS[n as usize % 6].name, ActionKind::DogTrick { target: dog, trick: n % 6, teach, at: tf.translation.xz(), kind }, false));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tricks_open_up_with_the_skill() {
        let mut t = DogTricks::default();
        assert_eq!(t.level(), 0);
        assert_eq!(t.learnable(), vec![0], "only Sit to begin with");
        t.skill = 500.0;
        assert_eq!(t.level(), 2);
        assert_eq!(t.learnable(), vec![0, 1, 2]);
        t.skill = 5000.0;
        assert_eq!(t.level(), 5);
        assert_eq!(t.learnable().len(), 6);
    }

    #[test]
    fn practice_teaches_a_trick() {
        let mut t = DogTricks::default();
        assert!(!t.tries(0, 0.01), "no chance at first");
        assert!(!t.practise(0, 100.0, 1.0));
        assert!(t.tries(0, 0.5) && !t.tries(0, 0.9), "two thirds there");
        assert!(t.practise(0, 60.0, 1.0));
        assert_eq!(t.known(), vec![0]);
        // (A genius dog that adores its teacher learns faster; a clueless one slower.)
        let genius = practice_rate(100.0, 90.0, &["GeniusPet".to_string()], true);
        let clueless = practice_rate(0.0, 0.0, &["CluelessPet".to_string()], false);
        assert!(genius > 1.3 && clueless < 0.85, "{genius} {clueless}");
    }

    #[test]
    fn the_sets_clips() {
        assert_eq!(clip("ad", &part_name(3, Part::Start), 'x'), "a2ad_soc_dogTricks_start2Shake_x");
        assert_eq!(clip("al", &part_name(0, Part::Try(false)), 'y'), "a2al_soc_dogTricks_sit_fail_y");
        assert_eq!(part_name(5, Part::Try(true)), "playDead_success_command");
        assert_eq!(wear_rate(&["LazyPet".to_string()]), 1.5);
    }
}
