//! City hall's "Change Name..." (the game's `CityHall.ChangeName`): from nine to six a young adult
//! or older goes in to change their own name or a child's of theirs, a teen or younger (its Sim
//! picker's); inside, the game's "Official Name Change" box asks for the new first name, then
//! the new last (each left as it was if nothing's typed).

use bevy::prelude::*;

use crate::PlayMode;
use crate::interact::ActionKind;
use crate::sim::{Age, Sim};

pub struct CityHallPlugin;

impl Plugin for CityHallPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (renamed, name_test).run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words: the menu (`ChangeName:InteractionName`), the box's title and asks.
pub const CHANGE_NAME: &str = "Change Name...";
const TITLE: &str = "Official Name Change";
/// A name's letters at most (`CASBasics.GetMaxNameLength`).
pub const NAME_LENGTH: usize = 20;
/// The hours it's done in (`kStartHour`, `kEndHour`).
const HOURS: std::ops::Range<f32> = 9.0..18.0;

/// City hall's errand for it, as a rabbit hole's activity (a quarter of an hour inside).
pub const CHANGE_NAME_TASK: usize = 995;
pub static CHANGE_NAME_ACT: crate::rabbitholes::Activity = crate::rabbitholes::Activity {
    name: "Change Name",
    minutes: 15.0,
    cost: 0,
    per_hour: [-4.0, -4.0, -2.0, 5.0, 0.0, 0.0],
    skill: None,
    open: HOURS.start,
    close: HOURS.end,
};

/// Off to city hall to change whose name.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChangingName(pub Entity);

/// Whose names a Sim may change there (the game's picker): their own, and their children's
/// that are teens or younger; none for those under young adulthood, or out of hours.
pub fn renamable<'a>(actor: (Entity, &Sim), hour: f32, children: &[u64], people: impl Iterator<Item = (Entity, &'a Sim)>) -> Vec<(Entity, String)> {
    if !actor.1.age.is_grown() || actor.1.age == Age::Child || !HOURS.contains(&hour) {
        return Vec::new();
    }
    let mut out = vec![(actor.0, actor.1.full_name())];
    out.extend(people.filter(|(e, s)| *e != actor.0 && children.contains(&s.id) && (s.age == Age::Teen || !s.age.is_grown())).map(|(e, s)| (e, s.full_name())));
    out
}

/// City hall's menu entry for it: "Change Name... ›" and whose.
pub fn menu(lot: &s3bake::LotInfo, lot_index: usize, who: &[(Entity, String)]) -> Option<(String, Vec<(String, ActionKind)>)> {
    if !lot.internal_name.to_ascii_lowercase().contains("cityhall") || who.is_empty() {
        return None;
    }
    Some((CHANGE_NAME.to_string(), who.iter().map(|(_, n)| (n.clone(), ActionKind::Visit { lot: lot_index, activity: CHANGE_NAME_TASK })).collect()))
}

/// The box for the next part of the name: the first, then the last.
fn ask(w: &mut World, sim: Entity, last: bool) {
    let Some(s) = w.get::<Sim>(sim) else { return };
    let (suggested, text) = if last {
        (s.last.clone(), format!("Please enter a new last name for {}.", s.full_name()))
    } else {
        (s.first.clone(), format!("Please enter a new first name for {}.", s.full_name()))
    };
    w.resource_mut::<crate::dialog::Questions>().ask(crate::dialog::Ask {
        about: crate::dialog::Question::ChangeName { sim, last, suggested },
        icon: String::new(),
        heading: String::new(),
        title: TITLE.into(),
        text,
        answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
    });
}

/// In at city hall: the box for the first name of whoever's name is being changed.
pub fn begin(w: &mut World, actor: Entity) {
    let Some(ChangingName(sim)) = w.get::<ChangingName>(actor).copied() else { return };
    w.entity_mut(actor).remove::<ChangingName>();
    ask(w, sim, false);
}

/// The new name typed in: the first, then the box for the last; the last, and it's done.
fn renamed(mut commands: Commands, mut answers: MessageReader<crate::dialog::Answered>, typed: Res<crate::dialog::Typed>, mut sims: Query<&mut Sim>, mut family: ResMut<crate::family::Genealogy>) {
    for a in answers.read() {
        let crate::dialog::Question::ChangeName { sim, last, .. } = &a.about else { continue };
        // (NAME_TEST_FIRST, NAME_TEST_LAST: what's typed, for a test.)
        let typed = std::env::var(if *last { "NAME_TEST_LAST" } else { "NAME_TEST_FIRST" }).unwrap_or_else(|_| typed.0.clone());
        let name: String = typed.trim().chars().take(NAME_LENGTH).collect();
        if let Ok(mut s) = sims.get_mut(*sim)
            && !name.is_empty()
        {
            if *last {
                s.last = name;
            } else {
                s.first = name;
            }
            family.note(s.id, &s.full_name(), s.female);
            info!("city hall: now {}", s.full_name());
        }
        if !*last {
            let sim = *sim;
            commands.queue(move |w: &mut World| ask(w, sim, true));
        }
    }
}

/// `NAME_TEST=1` (tests): the selected Sim off to city hall to change their own name (with
/// `ANSWER=0`, `NAME_TEST_FIRST`, `NAME_TEST_LAST` the boxes answered).
fn name_test(
    mut commands: Commands,
    world: Res<crate::loading::CurrentWorld>,
    mut sims: Query<(Entity, &mut crate::interact::ActionQueue), With<crate::sim::Selected>>,
    mut done: Local<bool>,
) {
    if *done || std::env::var_os("NAME_TEST").is_none() {
        return;
    }
    let Ok((e, mut q)) = sims.single_mut() else { return };
    *done = true;
    let Some(lot) = world.data.lots.iter().position(|l| l.internal_name.to_ascii_lowercase().contains("cityhall")) else { return };
    commands.entity(e).insert(ChangingName(e));
    q.push_player(crate::interact::Action::new(CHANGE_NAME, ActionKind::Visit { lot, activity: CHANGE_NAME_TASK }, false));
    info!("name test: off to city hall");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grown_ups_rename_themselves_and_their_young_at_city_hall_by_day() {
        let mut rng = rand::rng();
        let mom = crate::sim::random_sim(&mut rng, "M", Some(true), Age::Adult);
        let mut kid = crate::sim::random_sim(&mut rng, "K", Some(false), Age::Child);
        kid.id = 77;
        let mut grown = crate::sim::random_sim(&mut rng, "G", Some(false), Age::YoungAdult);
        grown.id = 78;
        let (a, k, g) = (Entity::from_bits(1), Entity::from_bits(2), Entity::from_bits(3));
        let people = [(k, &kid), (g, &grown)];
        let who = renamable((a, &mom), 10.0, &[77, 78], people.iter().copied());
        assert_eq!(who.iter().map(|w| w.0).collect::<Vec<_>>(), [a, k], "not a grown child");
        assert!(renamable((a, &mom), 20.0, &[77], people.iter().copied()).is_empty(), "closed at night");
        assert!(renamable((k, &kid), 10.0, &[], people.iter().copied()).is_empty(), "not for children");
        let hall = s3bake::LotInfo { id: 1, internal_name: "Com_CityHall".into(), corner: [0.0; 3], rotation: 0.0, width: 30, depth: 30, string_keys: Vec::new() };
        let (label, list) = menu(&hall, 4, &who).unwrap();
        assert_eq!((label.as_str(), list.len()), (CHANGE_NAME, 2));
    }
}
