//! The cheat console, as the game's (Ctrl+Shift+C): a line at the top of the screen to type its
//! cheats into, Enter to run one, Escape (or Ctrl+Shift+C again) to close it; while it's open the
//! keys are its alone. The game's own (`Sims3.Gameplay.Core.Commands`): "kaching" (§1,000),
//! "motherlode" (§50,000), "familyFunds <household> <amount>", "speed <0-4>", "resetSim <first>
//! <last>", "testingCheatsEnabled true|false" (the mailbox's "Make All Happy" then), and the
//! engine's "fps on|off" and "help".

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::AppState;

pub struct CheatsPlugin;

impl Plugin for CheatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Console>()
            .add_systems(PreUpdate, console_keys.after(bevy::input::InputSystems).run_if(in_state(AppState::InGame)))
            .add_systems(Update, (console_line, test_console).run_if(in_state(AppState::InGame)))
            .add_systems(OnExit(AppState::InGame), |mut c: ResMut<Console>| c.open = false);
    }
}

/// The console: open or not, what's typed, what's been run (for Up), and its last answer.
#[derive(Resource, Default)]
pub struct Console {
    pub open: bool,
    typed: String,
    history: Vec<String>,
    back: usize,
    reply: String,
}

/// "testingCheatsEnabled": the testing cheats on (the mailbox's "Make All Happy").
pub static TESTING: AtomicBool = AtomicBool::new(false);

pub fn testing() -> bool {
    TESTING.load(Ordering::Relaxed)
}

/// `moveobjects on`: objects put down anywhere, over walls and one another (the game's build and
/// buy cheat).
pub static MOVE_OBJECTS: AtomicBool = AtomicBool::new(false);
/// `freerealestate on`: homes bought for nothing.
pub static FREE_REAL_ESTATE: AtomicBool = AtomicBool::new(false);
/// `hideHeadlineEffects on`: no thought or speech balloons over the Sims.
pub static HIDE_HEADLINES: AtomicBool = AtomicBool::new(false);
/// `mapTags off`: no map tags in map view.
pub static NO_MAP_TAGS: AtomicBool = AtomicBool::new(false);

pub fn move_objects() -> bool {
    MOVE_OBJECTS.load(Ordering::Relaxed)
}
pub fn free_real_estate() -> bool {
    FREE_REAL_ESTATE.load(Ordering::Relaxed)
}
pub fn hide_headlines() -> bool {
    HIDE_HEADLINES.load(Ordering::Relaxed)
}
pub fn no_map_tags() -> bool {
    NO_MAP_TAGS.load(Ordering::Relaxed)
}

/// How the game names the mailbox's testing cheat.
pub const MAKE_ALL_HAPPY: &str = "Make All Happy";
/// The mailbox's other testing cheats (the game's `Cheats`): five strangers about made friends,
/// everyone known, the needs left as they are, a visitor come by now.
pub const MAKE_FRIENDS: &str = "Make Friends For Me";
pub const KNOW_EVERYONE: &str = "Make Me Know Everyone";
pub const MOTIVE_DECAY: &str = "Toggle Motive Decay";
pub const FORCE_VISITOR: &str = "Force Visitor";
/// A Sim's, Shift-clicked with the testing cheats on: grown up now, one of the household, needs
/// full.
pub const TRIGGER_AGE: &str = "Trigger Age Transition";
pub const ADD_TO_FAMILY: &str = "Add to Family";
pub const MAKE_HAPPY: &str = "Make Happy";
/// How many strangers "Make Friends For Me" befriends (the game's `kNumFriendsForMe`).
const FRIENDS_FOR_ME: usize = 5;

/// `Toggle Motive Decay`: needs left as they are.
pub static NO_DECAY: AtomicBool = AtomicBool::new(false);

pub fn no_decay() -> bool {
    NO_DECAY.load(Ordering::Relaxed)
}

/// The console's line on screen.
#[derive(Component)]
struct ConsoleLine;

fn console_keys(mut console: ResMut<Console>, mut keys: ResMut<ButtonInput<KeyCode>>, mut typed: MessageReader<KeyboardInput>, mut commands: Commands) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if ctrl && shift && keys.just_pressed(KeyCode::KeyC) {
        console.open = !console.open;
        console.typed.clear();
        typed.clear();
        keys.reset_all();
        return;
    }
    if !console.open {
        typed.clear();
        return;
    }
    let mut run = None;
    for ev in typed.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Character(c) if console.typed.chars().count() < 80 => console.typed.push_str(c.as_str()),
            Key::Space => console.typed.push(' '),
            Key::Backspace => {
                console.typed.pop();
            }
            Key::Escape => console.open = false,
            // (Up: what was run before, and before that.)
            Key::ArrowUp if console.back < console.history.len() => {
                console.back += 1;
                console.typed = console.history[console.history.len() - console.back].clone();
            }
            Key::Enter => run = Some(std::mem::take(&mut console.typed)),
            _ => {}
        }
    }
    // (Nothing else sees the keys while it's open.)
    keys.reset_all();
    if let Some(line) = run.filter(|l| !l.trim().is_empty()) {
        console.history.push(line.clone());
        console.back = 0;
        info!("cheat: {line}");
        commands.queue(move |w: &mut World| {
            let reply = run_cheat(w, &line);
            w.resource_mut::<Console>().reply = reply;
        });
    }
}

/// Runs a cheat (its name in any case, as the game's), with what it says back.
pub fn run_cheat(w: &mut World, line: &str) -> String {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some(cmd) = words.first().map(|c| c.to_ascii_lowercase()) else { return String::new() };
    let arg = |i: usize| words.get(i).copied().unwrap_or("");
    let on = |s: &str| matches!(s.to_ascii_lowercase().as_str(), "on" | "true" | "1");
    match cmd.as_str() {
        "kaching" => funds(w, |f| f + 1_000),
        "motherlode" => funds(w, |f| f + 50_000),
        "familyfunds" => {
            let Ok(amount) = words.last().unwrap_or(&"").parse::<i64>() else { return "Usage: familyFunds <household_name> <amount>".to_string() };
            let name = words[1..words.len() - 1].join(" ");
            let ours = w.get_resource::<crate::interact::Household>().is_some_and(|h| h.name.eq_ignore_ascii_case(&name) || h.name.to_ascii_lowercase().ends_with(&name.to_ascii_lowercase()));
            if ours { funds(w, |_| amount) } else { format!("No household named {name}.") }
        }
        "speed" => match arg(1).parse::<usize>() {
            Ok(s) if s <= crate::clock::SKIP => {
                w.resource_mut::<crate::clock::GameClock>().set_speed(s);
                String::new()
            }
            _ => "Usage: speed <0-4>".to_string(),
        },
        "testingcheatsenabled" => {
            TESTING.store(on(arg(1)), Ordering::Relaxed);
            String::new()
        }
        "fps" => {
            w.resource_mut::<crate::options::Settings>().show_fps = on(arg(1));
            String::new()
        }
        "resetsim" => reset_sim(w, arg(1), arg(2)),
        "moveobjects" => {
            MOVE_OBJECTS.store(on(arg(1)), Ordering::Relaxed);
            format!("moveObjects {}", if on(arg(1)) { "on" } else { "off" })
        }
        "freerealestate" => {
            FREE_REAL_ESTATE.store(on(arg(1)), Ordering::Relaxed);
            format!("freeRealEstate {}", if on(arg(1)) { "on" } else { "off" })
        }
        "hideheadlineeffects" => {
            HIDE_HEADLINES.store(on(arg(1)), Ordering::Relaxed);
            String::new()
        }
        "maptags" => {
            NO_MAP_TAGS.store(!on(arg(1)), Ordering::Relaxed);
            String::new()
        }
        "quit" => {
            w.write_message(AppExit::Success);
            String::new()
        }
        "help" => "kaching, motherlode, familyFunds <household> <amount>, speed <0-4>, resetSim <first> <last>, testingCheatsEnabled true|false, moveObjects on|off, freeRealEstate on|off, hideHeadlineEffects on|off, mapTags on|off, fps on|off, quit".to_string(),
        _ => format!("Unknown command: {}", words[0]),
    }
}

/// The household's funds changed (never below nothing).
fn funds(w: &mut World, to: impl FnOnce(i64) -> i64) -> String {
    match w.get_resource_mut::<crate::interact::Household>() {
        Some(mut h) => {
            h.funds = to(h.funds).max(0);
            String::new()
        }
        None => "There's no household to give it to.".to_string(),
    }
}

/// "resetSim": the Sim back to a safe, neutral state: whatever they were doing dropped (and
/// what they had to themselves let go), on their feet where they can walk.
fn reset_sim(w: &mut World, first: &str, last: &str) -> String {
    let mut sims = w.query::<(Entity, &crate::sim::Sim)>();
    let Some(e) = sims.iter(w).find(|(_, s)| s.first.eq_ignore_ascii_case(first) && s.last.eq_ignore_ascii_case(last)).map(|(e, _)| e) else {
        return format!("No Sim named {first} {last}.");
    };
    if let Some(mut q) = w.get_mut::<crate::interact::ActionQueue>(e) {
        q.0.clear();
    }
    let mut used = w.query::<&mut crate::interact::UsedBy>();
    for mut u in used.iter_mut(w) {
        if u.0 == Some(e) {
            u.0 = None;
        }
    }
    w.entity_mut(e).remove::<(crate::anim::ActionClip, crate::nav::PathFollow, crate::fire::Panicking)>();
    if let Some(mut a) = w.get_mut::<crate::sim::SimAnim>(e) {
        a.pose = crate::sim::Pose::Stand;
        a.seat_height = 0.0;
    }
    let at = w.get::<Transform>(e).map(|t| t.translation);
    if let (Some(at), Some(grid)) = (at, w.get_resource::<crate::nav::NavGrid>())
        && let Some((x, z)) = grid.nearest_free(at.xz())
    {
        let p = grid.center_of(x, z);
        if let Some(mut tf) = w.get_mut::<Transform>(e) {
            tf.translation.x = p.x;
            tf.translation.z = p.y;
        }
    }
    String::new()
}

/// The mailbox's "Make All Happy" (testing cheats on): the household's needs full and their bad
/// moodlets gone.
pub fn make_all_happy(w: &mut World) {
    let mut q = w.query_filtered::<(&mut crate::sim::Motives, Option<&mut crate::life::Moodlets>), With<crate::sim::HouseholdMember>>();
    for (mut m, moodlets) in q.iter_mut(w) {
        for v in m.0.iter_mut() {
            *v = 100.0;
        }
        if let Some(mut ml) = moodlets {
            ml.0.retain(|x| x.value >= 0);
        }
    }
}

/// "Make Friends For Me": five Sims of the town the selected Sim doesn't know become friends,
/// each to the other.
pub fn make_friends(w: &mut World) {
    let mut sel = w.query_filtered::<(Entity, Option<&crate::social::Relationships>), With<crate::sim::Selected>>();
    let Some((me, known)) = sel.iter(w).next().map(|(e, r)| (e, r.map_or_else(Vec::new, |r| r.0.keys().copied().collect::<Vec<_>>()))) else { return };
    let mut others = w.query_filtered::<(Entity, &crate::sim::Sim), Without<crate::sim::HouseholdMember>>();
    let mut strangers: Vec<Entity> = others.iter(w).filter(|(e, s)| *e != me && !s.age.is_little() && !known.contains(e)).map(|(e, _)| e).collect();
    use rand::seq::SliceRandom;
    strangers.shuffle(&mut rand::rng());
    strangers.truncate(FRIENDS_FOR_ME);
    for e in strangers {
        debug!("make friends: {}", w.get::<crate::sim::Sim>(e).map(|s| s.full_name()).unwrap_or_default());
        befriend(w, me, e, crate::social::FRIEND_THRESHOLD + 5.0);
    }
}

/// "Make Me Know Everyone": everyone in town an acquaintance of the selected Sim (each to the
/// other), those they know already as they are.
pub fn know_everyone(w: &mut World) {
    let mut sel = w.query_filtered::<(Entity, Option<&crate::social::Relationships>), With<crate::sim::Selected>>();
    let Some((me, known)) = sel.iter(w).next().map(|(e, r)| (e, r.map_or_else(Vec::new, |r| r.0.keys().copied().collect::<Vec<_>>()))) else { return };
    let mut all = w.query_filtered::<Entity, With<crate::sim::Sim>>();
    let strangers: Vec<Entity> = all.iter(w).filter(|e| *e != me && !known.contains(e)).collect();
    for e in strangers {
        befriend(w, me, e, 1.0);
    }
}

/// Two Sims knowing each other, at least this well (those who knew nobody now knowing someone).
fn befriend(w: &mut World, a: Entity, b: Entity, friendship: f32) {
    for (x, y) in [(a, b), (b, a)] {
        if w.get::<crate::social::Relationships>(x).is_none()
            && let Ok(mut e) = w.get_entity_mut(x)
        {
            e.insert(crate::social::Relationships::default());
        }
        if let Some(mut r) = w.get_mut::<crate::social::Relationships>(x) {
            let rel = r.0.entry(y).or_default();
            rel.friendship = rel.friendship.max(friendship);
        }
    }
}

/// "Toggle Motive Decay": the needs left as they are (or not again).
pub fn toggle_decay(w: &mut World) {
    let off = !NO_DECAY.load(Ordering::Relaxed);
    NO_DECAY.store(off, Ordering::Relaxed);
    w.resource_mut::<crate::interact::Notifications>().push(if off { "Motive decay is off." } else { "Motive decay is on." });
}

/// The console on screen while it's open: what's typed, and its last answer under it.
fn console_line(mut commands: Commands, console: Res<Console>, mut lines: Query<(Entity, &Children), With<ConsoleLine>>, mut texts: Query<&mut Text>) {
    match (console.open, lines.single_mut()) {
        (false, Ok((e, _))) => commands.entity(e).despawn(),
        (true, Err(_)) => {
            commands
                .spawn((
                    ConsoleLine,
                    DespawnOnExit(AppState::InGame),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(8.0),
                        top: Val::Px(8.0),
                        width: Val::Px(560.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.04, 0.09, 0.22, 0.88)),
                    BorderColor::all(Color::srgba(0.55, 0.75, 1.0, 0.8)),
                    GlobalZIndex(60),
                ))
                .with_children(|c| {
                    c.spawn(crate::menu::text("|", 15.0, Color::WHITE));
                    c.spawn(crate::menu::text("", 13.0, Color::srgb(0.75, 0.85, 1.0)));
                });
        }
        (true, Ok((_, kids))) => {
            for (k, want) in kids.iter().zip([format!("{}|", console.typed), console.reply.clone()]) {
                if let Ok(mut t) = texts.get_mut(k)
                    && t.0 != want
                {
                    t.0 = want;
                }
            }
        }
        _ => {}
    }
}

/// CHEAT_CONSOLE=<cheat>@<seconds> (tests): the console opened then with the cheat typed in,
/// run two seconds on (as Enter does), and closed two more after.
fn test_console(time: Res<Time>, mut console: ResMut<Console>, mut step: Local<u8>, mut commands: Commands) {
    let Some((line, at)) = std::env::var("CHEAT_CONSOLE").ok().and_then(|v| v.rsplit_once('@').and_then(|(l, s)| Some((l.to_string(), s.parse::<f32>().ok()?)))) else { return };
    let t = time.elapsed_secs();
    match *step {
        0 if t >= at => {
            console.open = true;
            console.typed = line;
            *step = 1;
        }
        1 if t >= at + 2.0 => {
            let line = std::mem::take(&mut console.typed);
            info!("cheat: {line}");
            commands.queue(move |w: &mut World| {
                let reply = run_cheat(w, &line);
                info!("cheat answered {reply:?}");
                w.resource_mut::<Console>().reply = reply;
            });
            *step = 2;
        }
        2 if t >= at + 4.0 => {
            console.open = false;
            *step = 3;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        w.insert_resource(crate::interact::Household { name: "Goth".into(), funds: 100, lot_index: 0, last_bill_day: 0, bills: Vec::new() });
        w.insert_resource(crate::clock::GameClock::default());
        w
    }

    #[test]
    fn the_money_cheats_add_and_set_the_households_funds() {
        let mut w = world();
        run_cheat(&mut w, "kaching");
        run_cheat(&mut w, "MotherLode");
        assert_eq!(w.resource::<crate::interact::Household>().funds, 51_100);
        run_cheat(&mut w, "familyfunds Goth 250");
        assert_eq!(w.resource::<crate::interact::Household>().funds, 250);
        assert_eq!(run_cheat(&mut w, "familyfunds Landgraab 9"), "No household named Landgraab.");
        assert_eq!(w.resource::<crate::interact::Household>().funds, 250);
    }

    #[test]
    fn speed_sets_the_clocks_and_unknown_cheats_say_so() {
        let mut w = world();
        run_cheat(&mut w, "speed 3");
        assert_eq!(w.resource::<crate::clock::GameClock>().speed, 3);
        assert_eq!(run_cheat(&mut w, "speed 9"), "Usage: speed <0-4>");
        assert_eq!(run_cheat(&mut w, "rosebud"), "Unknown command: rosebud");
    }

    #[test]
    fn make_friends_befriends_five_strangers_even_for_a_sim_who_knew_nobody() {
        let mut w = world();
        let sim = || crate::sim::random_sim(&mut rand::rng(), "T", Some(true), crate::sim::Age::Adult);
        let me = w.spawn((sim(), crate::sim::Selected, crate::sim::HouseholdMember)).id();
        let others: Vec<Entity> = (0..7).map(|_| w.spawn(sim()).id()).collect();
        make_friends(&mut w);
        let mine = &w.get::<crate::social::Relationships>(me).expect("knows people now").0;
        assert_eq!(mine.len(), FRIENDS_FOR_ME);
        for (o, rel) in mine {
            assert!(others.contains(o) && rel.friendship > crate::social::FRIEND_THRESHOLD);
            assert!(w.get::<crate::social::Relationships>(*o).is_some_and(|r| r.0.contains_key(&me)));
        }
    }

    #[test]
    fn free_real_estate_turns_on_and_off() {
        let mut w = world();
        assert_eq!(run_cheat(&mut w, "FreeRealEstate on"), "freeRealEstate on");
        assert!(free_real_estate());
        assert_eq!(run_cheat(&mut w, "freerealestate off"), "freeRealEstate off");
        assert!(!free_real_estate());
    }
}
