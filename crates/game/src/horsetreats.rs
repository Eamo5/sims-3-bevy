//! Horses fed produce by hand, as the Pets pack has it ("Feed Produce", the game's
//! `FeedHarvestable`): a Sim with produce a horse eats (the ingredients' `PetSpeciesThatCanEatOffGround`
//! naming horses: apples, carrots, lettuce, grapes, horse granola...) holds one out to a horse of
//! theirs and it eats it from their hand (`a2ah_feedHarvestable`), its hunger filling
//! (`kHungerDelta` 10, a carrot a fifth again: `kEatingCarrotMod`) and the horse liking them the
//! better; not while it's ridden or tied up. A horse begs for them ("Beg For Carrots", the pack's
//! `BegForHarvestable`, `petcompany`).

use bevy::prelude::*;

use crate::anim::{ActionClip, ClipLibrary};
use crate::baked::Baked;
use crate::interact::{ActionKind, ActionQueue, Phase};
use crate::inventory::{Inventory, ItemKind, Stack};
use crate::pets::{Pet, PetAnim, PetBonds, PetBusy};
use crate::sim::Sim;

pub struct HorseTreatsPlugin;

impl Plugin for HorseTreatsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (feeding, treats_test).chain().after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The game's words (`Gameplay/Excel/Socializing/Action:FeedHarvestable`).
pub const FEED: &str = "Feed Produce";
/// What it does for the horse: hunger, a carrot the more, and its liking for them.
const HUNGER: f32 = 10.0;
const CARROT: f32 = 1.2;
const LIKING: f32 = 3.0;
const CLIP: &str = "a2ah_feedHarvestable";

/// Whether an ingredient's one a horse eats (by its key: `Apple`, `Carrot`).
pub fn horse_eats(data: &s3bake::gamedata::GameDataBaked, key: &str) -> bool {
    data.ingredients.iter().find(|i| i.key.eq_ignore_ascii_case(key)).is_some_and(|i| i.pets_eat.iter().any(|s| s == "Horses" || s == "AllAvailable"))
}

/// The produce a Sim carries that a horse eats.
pub fn treats<'a>(data: &s3bake::gamedata::GameDataBaked, inv: Option<&'a Inventory>) -> Vec<&'a Stack> {
    inv.map_or(Vec::new(), |inv| inv.0.iter().filter(|s| s.kind == ItemKind::Produce && s.count > 0 && horse_eats(data, &s.key)).collect())
}

/// A horse's menu for a Sim on foot: "Feed Produce ›" what they carry that it eats.
pub fn options(horse: Entity, at: Vec2, stacks: &[&Stack], pie_submenus: &mut Vec<(String, Vec<(String, ActionKind)>)>, submenu: impl Fn(usize) -> ActionKind) -> Vec<(String, ActionKind)> {
    if stacks.is_empty() {
        return Vec::new();
    }
    let list: Vec<(String, ActionKind)> = stacks.iter().map(|s| (s.name.clone(), ActionKind::FeedProduce { horse, at, key: s.key.clone(), quality: s.quality })).collect();
    let out = vec![(format!("{FEED} ›"), submenu(pie_submenus.len()))];
    pie_submenus.push((FEED.to_string(), list));
    out
}

/// A horse eating from a Sim's hand (theirs to see to).
#[derive(Component)]
pub struct FedByHand;

/// A Sim feeding a horse: which, how far in, how long it takes, what's fed.
#[derive(Component)]
struct Feeding {
    horse: Entity,
    time: f32,
    length: f32,
    carrot: bool,
}

/// The produce held out and eaten: both sides' clips, the horse's hunger and liking.
#[allow(clippy::type_complexity)]
fn feeding(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<Baked>, ResMut<ClipLibrary>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &Transform, Option<&mut Inventory>, Option<&mut Feeding>), Without<Pet>>,
    mut pets: Query<(&Pet, &mut Transform, &mut PetAnim, &mut crate::petneeds::PetMotives, Option<&mut PetBonds>), Without<Sim>>,
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed].min(3.0);
    let mut rng = rand::rng();
    for (e, sim, mut queue, stf, inv, session) in &mut sims {
        let running = queue.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).and_then(|a| match &a.kind {
            ActionKind::FeedProduce { horse, key, quality, .. } => Some((*horse, key.clone(), *quality)),
            _ => None,
        });
        let Some(mut s) = session else {
            let Some((horse, key, quality)) = running else { continue };
            let Ok((pet, mut ptf, mut anim, ..)) = pets.get_mut(horse) else {
                queue.0.pop_front();
                continue;
            };
            // (What's fed taken from them now: none left, nothing to feed.)
            if !inv.is_some_and(|mut i| i.consume_produce(&key, quality)) {
                queue.0.pop_front();
                continue;
            }
            let (x, y) = (format!("{CLIP}_x"), format!("{CLIP}_y"));
            let length = lib.get(&data, &x).map_or(3.0, |c| c.duration).min(lib.get(&data, &y).map_or(f32::MAX, |c| c.duration)).max(0.3) - 0.05;
            // (Turned to them.)
            let to = stf.translation.xz() - ptf.translation.xz();
            if to.length() > 0.05 {
                ptf.rotation = Quat::from_rotation_y(to.x.atan2(to.y));
            }
            commands.entity(horse).insert((PetBusy, FedByHand)).remove::<crate::petneeds::PetTask>();
            anim.replay(y);
            let clips: &'static [&'static str] = Box::leak(Box::new([crate::pets::intern(x)]));
            commands.entity(e).insert((ActionClip::new(None, clips), Feeding { horse, time: 0.0, length, carrot: key.eq_ignore_ascii_case("Carrot") }));
            info!("horse treats: {} feeds {} a {key}", sim.first, pet.name);
            continue;
        };
        let Ok((pet, _, mut anim, mut pm, bonds)) = pets.get_mut(s.horse) else {
            commands.entity(e).remove::<(Feeding, ActionClip)>();
            continue;
        };
        let horse = s.horse;
        let done = |commands: &mut Commands, anim: &mut PetAnim, rng: &mut rand::rngs::ThreadRng| {
            commands.entity(e).remove::<(Feeding, ActionClip)>();
            commands.entity(horse).remove::<(PetBusy, FedByHand)>();
            anim.play(crate::pets::idle_clip(&pet.kind, false, rng));
        };
        if running.is_none_or(|r| r.0 != horse) {
            done(&mut commands, &mut anim, &mut rng);
            continue;
        }
        s.time += dt;
        if s.time < s.length {
            continue;
        }
        pm.add(crate::petneeds::HUNGER, HUNGER * if s.carrot { CARROT } else { 1.0 });
        if let Some(mut b) = bonds {
            b.add(sim.id, true, LIKING);
        }
        queue.0.pop_front();
        done(&mut commands, &mut anim, &mut rng);
    }
}

/// `TREATS_TEST=<horse>` (tests): eight seconds in, the selected Sim's given two apples and a
/// carrot and feeds that horse one of each in turn, the camera on them.
#[allow(clippy::type_complexity)]
fn treats_test(
    mut commands: Commands,
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), Without<Sim>>,
    mut sel: Query<(Entity, &Transform, &mut ActionQueue), (With<crate::sim::Selected>, Without<Pet>)>,
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
) {
    let Ok(name) = std::env::var("TREATS_TEST") else { return };
    let now = time.elapsed_secs();
    if now < 8.0 {
        return;
    }
    let Some((horse, _, htf)) = pets.iter().find(|p| p.1.name.eq_ignore_ascii_case(&name)) else { return };
    let Ok((me, stf, mut q)) = sel.single_mut() else { return };
    if let Ok(mut c) = cams.single_mut() {
        let mid = (stf.translation + htf.translation) / 2.0;
        let across = (htf.translation - stf.translation).xz().normalize_or(Vec2::Y).perp();
        c.look_at(mid + Vec3::Y * 1.0);
        c.yaw = across.x.atan2(across.y);
        c.pitch = 0.3;
        c.distance = 6.0;
    }
    match stage.0 {
        0 => {
            *stage = (1, now);
            crate::inventory::give(&mut commands, me, ItemKind::Produce, "Apple".into(), "Normal Apple".into(), 3, 4, 1);
            crate::inventory::give(&mut commands, me, ItemKind::Produce, "Carrot".into(), "Normal Carrot".into(), 3, 4, 1);
        }
        1 if now - stage.1 > 1.0 => {
            *stage = (2, now);
            let at = htf.translation.xz();
            q.push_player(crate::interact::Action::new(FEED, ActionKind::FeedProduce { horse, at, key: "Apple".into(), quality: 3 }, false));
            q.push_player(crate::interact::Action::new(FEED, ActionKind::FeedProduce { horse, at, key: "Carrot".into(), quality: 3 }, false));
            info!("horse treats test: feeding {name}");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horses_eat_what_the_game_says_they_do() {
        let ing = |key: &str, eat: &[&str]| s3bake::gamedata::IngredientInfo { key: key.into(), name: key.into(), pets_eat: eat.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let data = s3bake::gamedata::GameDataBaked { ingredients: vec![ing("Apple", &["AllDogs", "Horses", "AB"]), ing("Steak", &["AllDogs", "Cats"])], ..Default::default() };
        assert!(horse_eats(&data, "apple"));
        assert!(!horse_eats(&data, "Steak"));
        let inv = Inventory(vec![
            Stack { kind: ItemKind::Produce, key: "Apple".into(), name: "Perfect Apple".into(), quality: 9, count: 2, worth: 30 },
            Stack { kind: ItemKind::Produce, key: "Steak".into(), name: "Steak".into(), quality: 3, count: 1, worth: 10 },
        ]);
        let t = treats(&data, Some(&inv));
        assert_eq!(t.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Perfect Apple"]);
    }
}
