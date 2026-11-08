//! Playing a household's pets, as the Pets pack lets you: a pet clicked on the pet skewer is
//! selected as a Sim is (the game's `OnHouseholdSimButtonMouseDown` for both), its needs on the
//! motives panel in its species' own words (`SimOccultInfo`: a cat's Scratch, a dog's
//! Destruction, a horse's Exercise and Thirst in place of Hygiene and Fun), its picture on the
//! bust. With it selected, the world's clicks are its: the ground, "Go Here"; its bowl, "Eat";
//! its bed or stall, "Sleep" (a horse's stall, "Eat Hay" too); a litter box, a scratching post, a
//! chew toy, a water trough, a toy of its kind's, each what it does there. It goes and does it
//! (the pack's own animations, the need filling as when it sees to itself), then is left to its
//! own free will again.

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::{ActionKind, GameObject, ObjectKind};
use crate::petneeds::{BowlFood, PetDo};
use crate::pets::{HomePet, Pet};

pub struct PetControlPlugin;

impl Plugin for PetControlPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (pet_control_test, obey).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// What a pet's told to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PetCommand {
    Go(Vec2),
    Do(PetDo),
}

/// An order for a pet, to be seen to.
#[derive(Component)]
pub struct PetOrder(pub PetCommand);

/// What a selected pet can be told to do at an object, by its kind and the pet's.
pub fn object_options(pet: &Pet, target: Entity, o: &GameObject, food: Option<&BowlFood>) -> Vec<(String, ActionKind)> {
    let species = pet.kind.chars().nth(1).unwrap_or('d');
    let horse = species == 'h';
    let order = |label: &str, what: PetDo| (label.to_string(), ActionKind::PetCommand(PetCommand::Do(what)));
    let mut out = Vec::new();
    match o.kind {
        ObjectKind::PetBowl if !horse => {
            if food.is_some_and(|f| f.0 > 0) {
                out.push(order("Eat", PetDo::Eat(target)));
            } else {
                out.push(("Eat".to_string(), ActionKind::Greyed("The bowl is empty.".into())));
            }
        }
        ObjectKind::PetBed if !horse => out.push(order("Sleep", PetDo::Sleep(Some(target)))),
        ObjectKind::BoxStall if horse => {
            out.push(order("Eat Hay", PetDo::Hay(target)));
            out.push(order("Sleep", PetDo::Sleep(Some(target))));
        }
        ObjectKind::LitterBox if species == 'c' => out.push(order("Use Litter Box", PetDo::Litter(target))),
        ObjectKind::ScratchingPost if species == 'c' => out.push(order("Scratch", PetDo::Scratch(target))),
        ObjectKind::ChewToy if matches!(species, 'd' | 'l') => out.push(order("Chew", PetDo::Chew(target))),
        ObjectKind::WaterTrough if horse => out.push(order("Drink", PetDo::Drink(target))),
        _ => {}
    }
    if let Some(toy) = crate::pettoys::toy_for(&pet.kind, o.kind) {
        out.push(order("Play", PetDo::Play(target, toy)));
    }
    out
}

/// Ground clicked with a pet selected: there.
pub fn ground_options(at: Vec2) -> Vec<(String, ActionKind)> {
    vec![("Go Here".to_string(), ActionKind::PetCommand(PetCommand::Go(at)))]
}

/// A pet's orders seen to: what it was at left, off to what it's told.
#[allow(clippy::type_complexity)]
fn obey(
    mut commands: Commands,
    pets: Query<(Entity, &Pet, &PetOrder, &Transform), With<HomePet>>,
    objects: Query<(&GameObject, &Transform), Without<Pet>>,
    (grid, building): (Option<Res<crate::nav::NavGrid>>, Option<Res<crate::building::ActiveBuilding>>),
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let Some(grid) = grid else { return };
    let mut rng = rand::rng();
    for (e, pet, order, tf) in &pets {
        commands.entity(e).remove::<PetOrder>();
        let here = tf.translation.xz();
        let (what, to, otf) = match order.0 {
            PetCommand::Go(at) => (PetDo::Go, Some(at), None),
            PetCommand::Do(what) => {
                let Some((o, otf)) = what.object().and_then(|t| objects.get(t).ok()) else { continue };
                (what, crate::petneeds::spot_for(what, o, otf, here, &pet.kind), Some(*otf))
            }
        };
        match crate::petneeds::plan(pet, here, what, to, otf, &grid, building.as_deref(), &mut rng) {
            Some(task) => {
                info!("pet control: {} told to {what:?}", pet.name);
                commands.entity(e).insert(task);
            }
            None => notes.push(format!("{} can't get there.", pet.name)),
        }
    }
}

/// `PET_CONTROL_TEST=<name>:<x>,<z>` (tests): ten seconds in, the household's pet of that name
/// selected and sent so far from where it stands.
fn pet_control_test(
    mut commands: Commands,
    time: Res<Time>,
    pets: Query<(Entity, &Pet, &Transform), With<HomePet>>,
    selected: Query<Entity, With<crate::sim::Selected>>,
    mut done: Local<bool>,
) {
    let Ok(v) = std::env::var("PET_CONTROL_TEST") else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let (name, off) = v.split_once(':').unwrap_or((&v, "3,0"));
    let Some((e, _, tf)) = pets.iter().find(|(_, p, _)| p.name.eq_ignore_ascii_case(name)) else { return };
    *done = true;
    for s in &selected {
        commands.entity(s).remove::<crate::sim::Selected>();
    }
    let d: Vec<f32> = off.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let to = tf.translation.xz() + Vec2::new(d.first().copied().unwrap_or(3.0), d.get(1).copied().unwrap_or(0.0));
    commands.entity(e).insert((crate::sim::Selected, PetOrder(PetCommand::Go(to))));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pet(kind: &str) -> Pet {
        Pet { kind: kind.into(), breed: 0, name: "Rex".into(), female: false, look: Default::default(), elder: false }
    }

    fn object(kind: ObjectKind) -> GameObject {
        GameObject { kind, name: "Test".into(), objd: (0, 0, 0), price: 0, center: Vec2::ZERO, half: Vec2::splat(0.3), height: 0.5, route: None }
    }

    fn labels(o: &[(String, ActionKind)]) -> Vec<&str> {
        o.iter().map(|(l, _)| l.as_str()).collect()
    }

    #[test]
    fn a_pet_is_offered_what_its_kind_does_there() {
        let t = Entity::from_raw_u32(7).unwrap();
        let bowl = object(ObjectKind::PetBowl);
        let full = object_options(&pet("ad"), t, &bowl, Some(&BowlFood(3)));
        assert!(matches!(full.first().map(|o| &o.1), Some(ActionKind::PetCommand(PetCommand::Do(PetDo::Eat(b)))) if *b == t));
        assert!(matches!(object_options(&pet("ad"), t, &bowl, Some(&BowlFood(0)))[0].1, ActionKind::Greyed(_)), "an empty bowl's greyed");
        assert!(object_options(&pet("ah"), t, &bowl, None).is_empty(), "no bowls for horses");
        assert_eq!(labels(&object_options(&pet("ah"), t, &object(ObjectKind::BoxStall), None)), ["Eat Hay", "Sleep"]);
        assert_eq!(labels(&object_options(&pet("ac"), t, &object(ObjectKind::ScratchingPost), None)), ["Scratch"]);
        assert!(object_options(&pet("ad"), t, &object(ObjectKind::ScratchingPost), None).is_empty(), "posts are cats'");
        assert_eq!(labels(&object_options(&pet("al"), t, &object(ObjectKind::DogBall), None)), ["Play"]);
        assert_eq!(labels(&ground_options(Vec2::ZERO)), ["Go Here"]);
    }
}
