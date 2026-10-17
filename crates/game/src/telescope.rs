//! Searching the galaxy through a telescope, as the game's `Telescope.SearchGalaxyInteraction`:
//! every one to three hours (its tuning's `kMinTimeToFindInGalaxy`, `kMaxTimeToFindInGalaxy`)
//! something's seen. The first time, and three times in four after (`kChanceOfFindingSomethingNew`),
//! it's something new: a star mostly, its colour, kind and size the likelier the nearer their
//! difficulty to the Sim's Logic skill; now and then a satellite, comet, moon, planet or galaxy,
//! and one time in a thousand a black hole (`kChanceOfCreating`). The player names it, the
//! Holmberg-Weidler Astronomical Society pays for it (the tuned values: a black hole §10,000;
//! half as much again to a Celestial Explorer, `kValueModifier`), and
//! the search is over. Otherwise one found before is spotted again, and the search goes on.
//! What's been discovered is the town's (the game's `DiscoveredObjects`), kept in saves, and
//! counted in the finder's skill journal.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{ActionKind, ActionQueue, GameObject, Notifications, Phase, Special};
use crate::sim::Sim;

pub struct TelescopePlugin;

impl Plugin for TelescopePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Discoveries>().init_resource::<Naming>().add_systems(Update, (searching, named).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The kinds of discovery a telescope makes (the game's `TelescopeDiscoveryType`, its first seven).
const KINDS: [&str; 7] = ["Star", "Satellite", "Comet", "Moon", "Planet", "Aurora", "Galaxy"];
/// How likely each is (`kChanceOfCreating`, tuned).
const KIND_CHANCE: [f64; 7] = [0.75, 0.1, 0.03, 0.04, 0.069, 0.001, 0.01];
/// What each but a star is worth (`kTypeValueMin`, `kTypeValueMax`, tuned; from the satellite).
const VALUE_MIN: [i64; 6] = [25, 300, 250, 150, 10_000, 500];
const VALUE_MAX: [i64; 6] = [75, 500, 450, 350, 10_000, 1_500];
/// A star's colour, kind and size: their keys, weights, difficulties (a Logic level) and worth
/// (`kStarColorWeights` etc., tuned).
const COLORS: [&str; 5] = ["Blue", "White", "Yellow", "Orange", "Red"];
const COLOR_WEIGHT: [f32; 5] = [12.0, 8.0, 6.0, 3.0, 1.0];
const COLOR_DIFFICULTY: [f32; 5] = [3.0, 4.0, 5.0, 8.0, 11.0];
const COLOR_VALUE: [i64; 5] = [5, 20, 100, 350, 750];
const STAR_TYPES: [&str; 6] = ["Dwarf", "SubGiant", "Giant", "SuperGiant", "Neutron", "Pulsar"];
const TYPE_WEIGHT: [f32; 6] = [60.0, 60.0, 40.0, 15.0, 3.0, 1.0];
const TYPE_DIFFICULTY: [f32; 6] = [3.0, 4.0, 6.0, 11.0, 11.0, 11.0];
const TYPE_VALUE: [i64; 6] = [10, 25, 100, 600, 2_000, 5_000];
const MASSES: [&str; 6] = ["Tiny", "Small", "Medium", "Large", "Huge", "Ginormus"];
const MASS_WEIGHT: [f32; 6] = [26.0, 25.0, 16.0, 9.0, 6.0, 2.0];
const MASS_DIFFICULTY: [f32; 6] = [3.0, 3.0, 6.0, 7.0, 8.0, 11.0];
const MASS_VALUE: [i64; 6] = [10, 15, 50, 200, 500, 900];
/// How long between sightings (game minutes), and the chance of something new.
const FIND_EVERY: std::ops::Range<f64> = 60.0..180.0;
const NEW_CHANCE: f64 = 0.75;
/// What a Celestial Explorer's discoveries are worth more (`kValueModifier`, tuned).
const EXPLORER_BONUS: f64 = 1.5;
/// How long a discovery's name may be (`kMaxLengthOfNamedDiscovery`).
pub const NAME_LENGTH: usize = 25;
const KEY: &str = "Gameplay/Objects/HobbiesSkills/Telescope";

/// Something discovered: its name, kind (an index into the game's types), who found it, what
/// it was worth; a star's colour, kind and size.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Discovery {
    pub name: String,
    pub kind: u8,
    pub by: String,
    pub value: i64,
    #[serde(default)]
    pub star: Option<[u8; 3]>,
}

/// The town's discoveries (kept in saves).
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct Discoveries(pub Vec<Discovery>);

/// A discovery waiting for its name (and who found it).
#[derive(Resource, Default)]
struct Naming(Vec<(Entity, Discovery)>);

/// Searching the galaxy: when something's next seen.
#[derive(Component)]
pub struct Searching {
    next: f64,
}

/// A weighted choice by how near each one's difficulty is to the Sim's skill (the game's
/// `GetWeightedChance`: its weight over the distance, at least one).
fn pick_weighted(level: f32, weights: &[f32], difficulty: &[f32], rng: &mut impl Rng) -> usize {
    let w: Vec<f32> = weights.iter().zip(difficulty).map(|(w, d)| w / (d - level).abs().max(1.0)).collect();
    let total: f32 = w.iter().sum();
    let mut r = rng.random_range(0.0..total.max(f32::EPSILON));
    for (i, x) in w.iter().enumerate() {
        if r < *x {
            return i;
        }
        r -= x;
    }
    w.len() - 1
}

/// A new discovery (unnamed): its kind by the tuned chances, and what it's worth (more to a
/// Celestial Explorer).
fn discover(by: &str, logic: f32, explorer: bool, rng: &mut impl Rng) -> Discovery {
    let mut d = discovery(by, logic, rng);
    if explorer {
        d.value = (d.value as f64 * EXPLORER_BONUS) as i64;
    }
    d
}

fn discovery(by: &str, logic: f32, rng: &mut impl Rng) -> Discovery {
    let mut r: f64 = rng.random();
    let kind = KIND_CHANCE.iter().position(|c| {
        r -= c;
        r < 0.0
    });
    let kind = kind.unwrap_or(KINDS.len() - 1);
    if kind == 0 {
        let c = pick_weighted(logic, &COLOR_WEIGHT, &COLOR_DIFFICULTY, rng);
        let t = pick_weighted(logic, &TYPE_WEIGHT, &TYPE_DIFFICULTY, rng);
        let m = pick_weighted(logic, &MASS_WEIGHT, &MASS_DIFFICULTY, rng);
        return Discovery { name: String::new(), kind: 0, by: by.to_string(), value: COLOR_VALUE[c] + TYPE_VALUE[t] + MASS_VALUE[m], star: Some([c as u8, t as u8, m as u8]) };
    }
    let value = rng.random_range(VALUE_MIN[kind - 1]..=VALUE_MAX[kind - 1]);
    Discovery { name: String::new(), kind: kind as u8, by: by.to_string(), value, star: None }
}

/// The game's words for a star's colour, kind and size.
fn star_words(ui: Option<&crate::layout::UiAssets>, s: [u8; 3]) -> [String; 3] {
    let word = |k: String, or: &str| ui.and_then(|u| u.localize(&format!("{KEY}:{k}"))).unwrap_or_else(|| or.to_string());
    let (c, t, m) = (COLORS[s[0] as usize], STAR_TYPES[s[1] as usize], MASSES[s[2] as usize]);
    [word(format!("StarColor{c}"), c), word(format!("StarType{t}"), t), word(format!("StarMass{m}"), m)]
}

/// A kind's name ("Star", the game's "black hole" for its sixth).
pub fn kind_name(ui: Option<&crate::layout::UiAssets>, kind: u8) -> String {
    let k = KINDS[(kind as usize).min(KINDS.len() - 1)];
    ui.and_then(|u| u.localize(&format!("{KEY}:Type{k}"))).unwrap_or_else(|| if k == "Aurora" { "black hole".into() } else { k.to_string() })
}

/// Searching: a sighting every hour or few; something new named and paid for (and the search
/// over), or one found before spotted again.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn searching(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, Option<&mut Searching>, &crate::interact::Skills, Option<&crate::journal::SkillJournal>)>,
    objects: Query<&GameObject>,
    discoveries: Res<Discoveries>,
    mut naming: ResMut<Naming>,
    mut questions: ResMut<crate::dialog::Questions>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<Notifications>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    for (e, sim, mut queue, search, skills, journal) in &mut sims {
        let at = queue.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).is_some_and(|a| match a.kind {
            ActionKind::Object { target, def } => objects.get(target).ok().and_then(|o| crate::interact::interactions_for(o.kind).get(def)).is_some_and(|d| d.special == Special::SearchGalaxy),
            _ => false,
        });
        match (at, search) {
            (true, None) => {
                commands.entity(e).insert(Searching { next: now + rng.random_range(FIND_EVERY) });
            }
            (true, Some(mut s)) if now >= s.next => {
                let known: Vec<&Discovery> = discoveries.0.iter().chain(naming.0.iter().map(|(_, d)| d)).collect();
                if known.is_empty() || rng.random_bool(NEW_CHANCE) {
                    let d = discover(&sim.full_name(), skills.level("Logic") as f32, crate::journal::earned(journal, "Celestial Explorer"), &mut rng);
                    let ui = ui.as_deref();
                    let first = sim.first.clone();
                    let kind = KINDS[d.kind as usize];
                    let prompt = match d.star {
                        Some(s) => {
                            let [c, t, m] = star_words(ui, s);
                            ui.and_then(|u| u.localize(&format!("{KEY}:NameDiscoveryPromptStar")))
                                .map(|p| p.replace("{1.String}", &c).replace("{2.String}", &t).replace("{3.String}", &m).replace("{4.Number}", &d.value.to_string()))
                                .unwrap_or_else(|| format!("{first} has discovered a new {m} {c} {t}! {first} now gets to name the star, and will be awarded {} Simoleons.", d.value))
                        }
                        None => ui
                            .and_then(|u| u.localize(&format!("{KEY}:NameDiscoveryPrompt{kind}")))
                            .map(|p| p.replace("{1.Number}", &d.value.to_string()))
                            .unwrap_or_else(|| format!("{first} has discovered a new {}! {first} gets to name it, and will be awarded {} Simoleons.", kind_name(ui, d.kind), d.value)),
                    }
                    .replace("{0.SimFirstName}", &first);
                    let suggested = ui.and_then(|u| u.localize(&format!("{KEY}:DefaultDiscoveryName"))).unwrap_or_else(|| "New Name".into());
                    questions.ask(crate::dialog::Ask {
                        about: crate::dialog::Question::NameDiscovery { sim: e, suggested },
                        icon: String::new(),
                        heading: sim.full_name(),
                        title: ui.and_then(|u| u.localize(&format!("{KEY}:NameDiscoveryWindowText"))).unwrap_or_else(|| "New Object Discovered!".into()),
                        text: prompt,
                        answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
                    });
                    naming.0.push((e, d));
                    // (Something new found: the search is over.)
                    if let Some(a) = queue.0.front_mut() {
                        a.cancel = true;
                    }
                } else {
                    s.next = now + rng.random_range(FIND_EVERY);
                    let d = known[rng.random_range(0..known.len())];
                    notes.push(format!("{} has spotted {}, the {} discovered by {}.", sim.first, d.name, kind_name(ui.as_deref(), d.kind), d.by));
                }
            }
            (false, Some(_)) => {
                commands.entity(e).remove::<Searching>();
            }
            _ => {}
        }
    }
}

/// A discovery named: the town's, paid for, and in the finder's journal.
fn named(
    mut answers: MessageReader<crate::dialog::Answered>,
    typed: Res<crate::dialog::Typed>,
    mut naming: ResMut<Naming>,
    mut discoveries: ResMut<Discoveries>,
    mut household: Option<ResMut<crate::interact::Household>>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    for a in answers.read() {
        let crate::dialog::Question::NameDiscovery { sim, .. } = &a.about else { continue };
        let Some(i) = naming.0.iter().position(|(e, _)| e == sim) else { continue };
        let (e, mut d) = naming.0.remove(i);
        let t = typed.0.trim();
        d.name = if t.is_empty() { kind_name(ui.as_deref(), d.kind) } else { t.chars().take(NAME_LENGTH).collect() };
        if let Some(h) = household.as_mut() {
            h.funds += d.value;
        }
        did.write(crate::journal::Did::count(e, crate::journal::Stat::StarsFound, 1.0));
        info!("telescope: {} discovered {} ({}, §{})", d.by, d.name, KINDS[d.kind as usize], d.value);
        discoveries.0.push(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mostly_stars_their_kinds_by_skill_and_the_tuned_values() {
        let mut rng = rand::rng();
        let finds: Vec<Discovery> = (0..4000).map(|_| discover("A", 3.0, false, &mut rng)).collect();
        let stars = finds.iter().filter(|d| d.kind == 0).count() as f64 / finds.len() as f64;
        assert!((0.7..0.8).contains(&stars), "{stars}");
        for d in &finds {
            match d.star {
                Some([c, t, m]) => assert_eq!(d.value, COLOR_VALUE[c as usize] + TYPE_VALUE[t as usize] + MASS_VALUE[m as usize]),
                None => assert!((VALUE_MIN[d.kind as usize - 1]..=VALUE_MAX[d.kind as usize - 1]).contains(&d.value)),
            }
        }
        // (A beginner sees blue stars most, an expert the rarer red ones more often.)
        let red = |level: f32, rng: &mut rand::rngs::ThreadRng| (0..4000).filter(|_| pick_weighted(level, &COLOR_WEIGHT, &COLOR_DIFFICULTY, rng) == 4).count();
        assert!(red(10.0, &mut rng) > red(0.0, &mut rng) * 2);
    }
}
