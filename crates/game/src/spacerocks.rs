//! Space rocks analyzed, as the game's `Rock.Analyze` and `AnalyzeAll`: a space rock found (small,
//! medium or large) is only a space rock till it's looked over from the inventory ("Analyze Space
//! Rock", or "Analyze All" of a stack), when it turns out to be one of the game's `RockNames`, a
//! Howardite to a Mesosiderite, worth its multiplier times what it was. With half the kinds
//! analyzed (the Collecting skill's Amateur Rock Collector) a kind not yet found is tried for five
//! times over; with all of them (Rock Collector), one worth two and a half times or more. The
//! game's words say how it came out, common to rare (a Gatherer's taken ten times rarer: the
//! game's `kGathererTraitSpaceRocksAnalyzeBetter`, which changes only the words).

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::ActionClip;
use crate::interact::{Action, ActionKind, ActionQueue, Phase};
use crate::inventory::{Inventory, ItemKind, Stack};
use crate::sim::Sim;

pub struct SpaceRocksPlugin;

impl Plugin for SpaceRocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (analyzing, analyze_test).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words for it.
pub const ANALYZE: &str = "Analyze Space Rock";
pub const ANALYZE_ALL: &str = "Analyze All";

/// `Analyze`'s tuning: tries for a kind not yet found, for one worth the most, and that worth;
/// the words' thresholds (`kCommonThreshold`, `kUncommonThreshold`, `kRareThreshold`); a
/// Gatherer's multiplier for them.
const AMATEUR_TRIES: usize = 5;
const COLLECTOR_TRIES: usize = 5;
const COLLECTOR_MIN: f32 = 2.5;
const COMMON: f32 = 1.0;
const UNCOMMON: f32 = 2.0;
const RARE: f32 = 5.0;
const GATHERER: f32 = 10.0;

/// Looking it over (the game's `viewobjectinteraction`): a curious look, a child's look up and
/// over.
const LOOK: &str = "a_react_view_curious_x";
const LOOK_CHILD: &str = "c_idle_neutral_look_upAndOver_x";

/// Whether a stack is space rocks not yet analyzed (found ones keep their size's key, the
/// tables' `SpaceRockSmall`...; analyzed ones are told apart by their kind, one more than its
/// place in `RockNames`, as their quality).
pub fn unanalyzed(s: &Stack) -> bool {
    s.kind == ItemKind::Find && s.key.starts_with("SpaceRock") && s.quality == 0 && s.count > 0
}

/// Which kind it turns out to be: any (`roll` picking), unless half the kinds have been found
/// (then a new one tried for), or all of them (then a precious one).
pub fn pick(names: &[s3bake::gamedata::RockName], found: &std::collections::BTreeSet<String>, mut roll: impl FnMut() -> usize) -> Option<usize> {
    if names.is_empty() {
        return None;
    }
    let mut i = roll() % names.len();
    let known = names.iter().filter(|n| found.contains(&n.key)).count();
    if known == names.len() {
        for _ in 0..COLLECTOR_TRIES {
            if names[i].multiplier >= COLLECTOR_MIN {
                break;
            }
            i = roll() % names.len();
        }
    } else if known as f32 >= names.len() as f32 / 2.0 {
        for _ in 0..AMATEUR_TRIES {
            if !found.contains(&names[i].key) {
                break;
            }
            i = roll() % names.len();
        }
    }
    Some(i)
}

/// The game's words for how it came out (`Analyze:TNSCommon`, `TNS`, `TNSUncommon`, `TNSRare`).
pub fn words(name: &str, worth: i64, multiplier: f32, gatherer: bool) -> String {
    let m = multiplier * if gatherer { GATHERER } else { 1.0 };
    if m < COMMON {
        format!("This space rock is {name}. Unfortunately, it's actually quite common, and worth less than the initial appraisal. Its value is §{worth}.")
    } else if m < UNCOMMON {
        format!("This space rock looks to be {name}, worth around §{worth}")
    } else if m < RARE {
        format!("Wow, {name}! This was a great find, and is worth quite a bit more than the initial appraisal. It's valued at §{worth}!")
    } else {
        format!("Astounding! It's actually {name}, one of the rarest space rocks in existence, and far more precious than originally thought. It's valued at §{worth}!")
    }
}

/// Space rocks being looked over: which stack, and all of it or one.
#[derive(Component, Clone, Debug)]
pub struct Analyzing {
    key: String,
    all: bool,
    begun: bool,
}

/// "Analyze Space Rock" or "Analyze All" chosen from the inventory: looked over where they stand.
pub fn start(commands: &mut Commands, sim: Entity, who: &Sim, queue: &mut ActionQueue, stack: &Stack, all: bool) {
    let look: &'static [&'static str] = if who.age == crate::sim::Age::Child { &[LOOK_CHILD] } else { &[LOOK] };
    commands.entity(sim).insert(Analyzing { key: stack.key.clone(), all, begun: false });
    queue.push_player(Action::new(if all { ANALYZE_ALL } else { ANALYZE }, ActionKind::Outro { clips: look, then: None, secs: 6.0, stand_at: None, target: sim }, false));
}

/// One space rock of the stack analyzed: what it is, worth its multiplier more, kept as that kind.
fn analyze_one(inv: &mut Inventory, key: &str, kind: usize, rock: &s3bake::gamedata::RockName, size: &str) -> Option<i64> {
    let i = inv.0.iter().position(|s| unanalyzed(s) && s.key == key)?;
    let each = inv.take_one(i)?;
    let worth = (each as f32 * rock.multiplier).round() as i64;
    inv.add(ItemKind::Find, key, &format!("{} ({size})", rock.name), kind as u8 + 1, worth);
    Some(worth)
}

#[allow(clippy::type_complexity)]
fn analyzing(
    mut commands: Commands,
    (data, mut lib, ui): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>, Option<Res<crate::icons::GameUi>>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &mut Analyzing, &mut Inventory)>,
    mut collection: ResMut<crate::collecting::Collection>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let mut rng = rand::rng();
    for (e, sim, mut q, mut a, mut inv) in &mut sims {
        let now = q.0.front().filter(|x| (x.label == ANALYZE || x.label == ANALYZE_ALL) && matches!(x.kind, ActionKind::Outro { target, .. } if target == e));
        match now {
            Some(x) if !a.begun => {
                if !matches!(x.phase, Phase::Running(_)) {
                    continue;
                }
                a.begun = true;
                // (The rock in a thought balloon as they look it over, its look's length.)
                let clip = if sim.age == crate::sim::Age::Child { LOOK_CHILD } else { LOOK };
                let secs = lib.get(&data, clip).map_or(6.0, |c| c.duration);
                commands.entity(e).insert(ActionClip::new(Some(clip), &[]));
                if let Some(objd) = ui.as_ref().and_then(|u| u.data.collectibles.iter().find(|c| c.key == a.key)).and_then(|c| data.0.catalog.iter().find(|o| o.instance_name.eq_ignore_ascii_case(&c.model))).map(|o| o.objd) {
                    commands.entity(e).insert(crate::balloons::BalloonRequest::thought(&s3bake::gamedata::thumb_name(objd.2)));
                }
                if let Some(x) = q.0.front_mut()
                    && let ActionKind::Outro { secs: s, .. } = &mut x.kind
                {
                    *s = secs;
                }
            }
            Some(_) => {}
            None if a.begun => {
                commands.entity(e).remove::<Analyzing>();
                let Some(ui) = ui.as_ref() else { continue };
                let names = &ui.data.rock_names;
                let size = ui.data.collectibles.iter().find(|c| c.key == a.key).map_or("Space Rock".to_string(), |c| c.name.clone());
                let n = if a.all { inv.0.iter().filter(|s| unanalyzed(s) && s.key == a.key).map(|s| s.count).sum() } else { 1 };
                let gatherer = sim.traits.contains(&crate::life::Trait::Gatherer);
                for _ in 0..n {
                    let Some(kind) = pick(names, &collection.rocks, || rng.random_range(0..usize::MAX)) else { break };
                    let rock = &names[kind];
                    let Some(worth) = analyze_one(&mut inv, &a.key, kind, rock, &size) else { break };
                    collection.rocks.insert(rock.key.clone());
                    info!("space rocks: {} analyzed a {} ({})", sim.first, rock.name, rock.multiplier);
                    notes.push(words(&rock.name, worth, rock.multiplier, gatherer));
                }
            }
            None => {
                commands.entity(e).remove::<Analyzing>();
            }
        }
    }
}

/// `ANALYZE_TEST=1|all|menu` (tests): eight seconds in, the selected Sim has three large space
/// rocks in their inventory and analyzes one (or all of them, or the stack's menu is up); the
/// camera on them.
fn analyze_test(
    mut commands: Commands,
    time: Res<Time>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut sel: Query<(Entity, &Sim, &mut ActionQueue, Option<&Inventory>, &Transform), With<crate::sim::Selected>>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<u8>,
) {
    let Ok(what) = std::env::var("ANALYZE_TEST") else { return };
    let t = time.elapsed_secs();
    let (Some(ui), Ok((e, sim, mut q, inv, tf))) = (ui, sel.single_mut()) else { return };
    if t > 7.0
        && let Ok(mut cam) = cams.single_mut()
    {
        cam.look_at(tf.translation);
        cam.distance = 7.0;
    }
    match *stage {
        0 if t > 8.0 => {
            let Some(c) = ui.data.collectibles.iter().find(|c| c.key == "SpaceRockLarge") else { return };
            crate::inventory::give(&mut commands, e, ItemKind::Find, c.key.clone(), c.name.clone(), 0, c.max_price as i64, 3);
            info!("analyze test: {} space rocks in hand ({} kinds known)", c.name, ui.data.rock_names.len());
            *stage = 1;
        }
        1 if t > 10.0 => {
            let Some((i, s)) = inv.and_then(|i| i.0.iter().enumerate().find(|(_, s)| unanalyzed(s))).map(|(i, s)| (i, s.clone())) else { return };
            *stage = 2;
            // (=menu: the inventory tab, then the stack's menu on it.)
            if what == "menu" {
                commands.insert_resource(crate::livehud::InfoPanel::Inventory);
                *stage = 3 + i as u8;
                return;
            }
            start(&mut commands, e, sim, &mut q, &s, what == "all");
        }
        n if n >= 3 && t > 12.0 => {
            commands.insert_resource(crate::hudpanels::StackPie((n - 3) as usize));
            *stage = 2;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rock(key: &str, m: f32) -> s3bake::gamedata::RockName {
        s3bake::gamedata::RockName { key: key.into(), name: key.into(), multiplier: m }
    }

    #[test]
    fn collectors_find_new_kinds_and_then_precious_ones() {
        let names = [rock("Howardite", 0.7), rock("Eucrite", 0.8), rock("Pallasite", 4.0), rock("Mesosiderite", 10.0)];
        let mut found = std::collections::BTreeSet::new();
        // (None found: any, as rolled.)
        assert_eq!(pick(&names, &found, || 0), Some(0));
        // (Half found: one not yet found tried for.)
        found.insert("Howardite".to_string());
        found.insert("Eucrite".to_string());
        let mut rolls = [0, 1, 3].into_iter();
        assert_eq!(pick(&names, &found, || rolls.next().unwrap_or(0)), Some(3));
        // (All found: a precious one tried for.)
        found.extend(["Pallasite".to_string(), "Mesosiderite".to_string()]);
        let mut rolls = [1, 0, 2].into_iter();
        assert_eq!(pick(&names, &found, || rolls.next().unwrap_or(0)), Some(2));
        assert_eq!(pick(&[], &found, || 0), None);
    }

    #[test]
    fn an_analyzed_rock_is_worth_its_kind_and_kept_apart() {
        let mut inv = Inventory::default();
        inv.add(ItemKind::Find, "SpaceRockLarge", "Large Space Rock", 0, 100);
        inv.add(ItemKind::Find, "SpaceRockLarge", "Large Space Rock", 0, 100);
        let worth = analyze_one(&mut inv, "SpaceRockLarge", 2, &rock("Pallasite", 4.0), "Large Space Rock");
        assert_eq!(worth, Some(400));
        assert_eq!(inv.0.iter().filter(|s| unanalyzed(s)).map(|s| s.count).sum::<u32>(), 1);
        let done = inv.0.iter().find(|s| s.quality == 3).unwrap();
        assert_eq!((done.name.as_str(), done.worth), ("Pallasite (Large Space Rock)", 400));
        assert!(words("Pallasite", 400, 4.0, false).starts_with("Wow, Pallasite!"));
        assert!(words("Howardite", 70, 0.7, false).contains("quite common"));
        assert!(words("Howardite", 70, 0.7, true).starts_with("Astounding!"), "a Gatherer's words are rosier");
        assert!(words("Mesosiderite", 1000, 10.0, false).starts_with("Astounding!"));
    }
}
