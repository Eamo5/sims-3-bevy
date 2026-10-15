//! Skill classes at the town's rabbit holes, as the game's (`AttendClassInRabbitHole`): an hour's
//! class for §400 (the game's `kClass<Skill>Cost` and `Duration`), from eight in the morning to
//! four in the afternoon, once per skill. A class gives the skill's first level's worth of
//! points (`SkillManager.SetTakenClass`). Cooking at the bistro, Athletic at the stadium,
//! Gardening and Logic at the science lab, Writing at the business office, Guitar at the theatre,
//! Charisma at city hall, Painting at the school, Handiness at the military base, Fishing at the
//! grocery. One class a day is a fifth off (the game's discounted class of the day).

use bevy::prelude::*;

use crate::PlayMode;
use crate::rabbitholes::Activity;

pub struct ClassesPlugin;

impl Plugin for ClassesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, class_over.run_if(in_state(PlayMode::Live)));
    }
}

/// Visit activity numbers for classes: this plus the class's index in `CLASSES`.
pub const CLASS_TASK: usize = 3000;
/// A class's price, length (game minutes) and hours, and the day's discount.
const PRICE: i64 = 400;
const DISCOUNT: f64 = 0.8;
const OPEN: f32 = 8.0;
const CLOSE: f32 = 16.0;

/// A class: where it's taught (a word in the lot's name) and its skill.
pub struct Class {
    pub venue: &'static [&'static str],
    pub skill: &'static str,
    pub activity: Activity,
}

/// In class: bored a little, sociable a little.
const IN_CLASS: [f32; 6] = [-6.0, -5.0, -6.0, 8.0, -3.0, 4.0];

const fn class(venue: &'static [&'static str], skill: &'static str, name: &'static str) -> Class {
    Class { venue, skill, activity: Activity { name, minutes: 60.0, cost: PRICE, per_hour: IN_CLASS, skill: None, open: OPEN, close: CLOSE } }
}

pub static CLASSES: [Class; 10] = [
    class(&["bistro", "restaurant"], "Cooking", "Attend Cooking Class"),
    class(&["stadium"], "Athletic", "Attend Athletic Class"),
    class(&["science"], "Gardening", "Attend Gardening Class"),
    class(&["science"], "Logic", "Attend Logic Class"),
    class(&["business", "journalism"], "Writing", "Attend Writing Class"),
    class(&["theater", "theatre"], "Guitar", "Attend Guitar Class"),
    class(&["cityhall"], "Charisma", "Attend Charisma Class"),
    class(&["school"], "Painting", "Attend Painting Class"),
    class(&["military"], "Handiness", "Attend Handiness Class"),
    class(&["grocery"], "Fishing", "Attend Fishing Class"),
];

/// The skills whose classes a Sim has taken.
#[derive(Component, Default, Clone, Debug)]
pub struct ClassesTaken(pub Vec<String>);

/// Back from a class.
#[derive(Component)]
pub struct ClassOver(pub &'static str);

/// The class an activity number stands for.
pub fn class_task(activity: usize) -> Option<&'static Activity> {
    activity.checked_sub(CLASS_TASK).and_then(|i| CLASSES.get(i)).map(|c| &c.activity)
}

/// The skill a rabbit hole activity is the class of.
pub fn class_of(a: &Activity) -> Option<&'static str> {
    CLASSES.iter().find(|c| std::ptr::eq(&c.activity, a)).map(|c| c.skill)
}

/// The day's discounted class (a different one each day).
pub fn discounted(day: u32) -> usize {
    (day as usize * 7 + 3) % CLASSES.len()
}

/// What a class costs today.
pub fn price(i: usize, day: u32) -> i64 {
    if discounted(day) == i { (PRICE as f64 * DISCOUNT).round() as i64 } else { PRICE }
}

/// The classes a lot offers a Sim now: those of its kind, in class hours, not taken already, for
/// children and up (the game's classes are for C, T, Y, A, E). Labelled as the game's ("Attend
/// Cooking Class (§400)"), with their visit activity numbers.
pub fn lot_classes(lot: &s3formats::world::LotInfo, hour: f32, day: u32, age: crate::sim::Age, taken: Option<&ClassesTaken>, ui: Option<&crate::layout::UiAssets>) -> Vec<(String, usize)> {
    if !(OPEN..CLOSE).contains(&hour) || age.is_little() {
        return Vec::new();
    }
    let name = lot.internal_name.to_ascii_lowercase();
    CLASSES
        .iter()
        .enumerate()
        .filter(|(_, c)| c.venue.iter().any(|v| name.contains(v)))
        .filter(|(_, c)| !taken.is_some_and(|t| t.0.iter().any(|s| s == c.skill)))
        .map(|(i, c)| {
            let words = ui.and_then(|u| u.localize("Gameplay/Abstracts/RabbitHole/AttendClassInRabbitHole:InteractionName")).unwrap_or_else(|| "Attend {0.String} Class ({1.Money})".into());
            (words.replace("{0.String}", c.skill).replace("{1.Money}", &format!("§{}", price(i, day))), CLASS_TASK + i)
        })
        .collect()
}

/// Back from a class: the skill's first level's worth of points (or the skill itself, new), and
/// the class taken.
fn class_over(
    mut commands: Commands,
    mut sims: Query<(Entity, &crate::sim::Sim, &ClassOver, &mut crate::interact::Skills, Option<&mut ClassesTaken>)>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut life: MessageWriter<crate::life::LifeEvent>,
) {
    for (e, sim, c, mut skills, taken) in &mut sims {
        commands.entity(e).remove::<ClassOver>();
        let first = crate::interact::skill_points(c.0).first().copied().unwrap_or(1800) as f32;
        let level = skills.add_points(c.0, first);
        match taken {
            Some(mut t) => t.0.push(c.0.to_string()),
            None => {
                commands.entity(e).insert(ClassesTaken(vec![c.0.to_string()]));
            }
        }
        notes.push(format!("{} finished the {} class.", sim.first, c.0));
        if let Some(level) = level {
            notes.push(format!("{} reached level {level} in {}!", sim.first, c.0));
            life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: c.0, level }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_are_had_where_and_when_the_game_has_them() {
        let lot = |n: &str| s3formats::world::LotInfo { id: 0, internal_name: n.into(), corner: [0.0; 3], rotation: 0.0, width: 20, depth: 20, string_keys: Vec::new() };
        let adult = crate::sim::Age::Adult;
        // (The science lab: gardening and logic; in class hours only; once.)
        let at_lab = lot_classes(&lot("Com_Science_40x24"), 10.0, 0, adult, None, None);
        assert_eq!(at_lab.len(), 2);
        assert!(at_lab[0].0.starts_with("Attend Gardening Class"));
        assert!(lot_classes(&lot("Com_Science_40x24"), 17.0, 0, adult, None, None).is_empty());
        let taken = ClassesTaken(vec!["Gardening".into()]);
        assert_eq!(lot_classes(&lot("Com_Science_40x24"), 10.0, 0, adult, Some(&taken), None).len(), 1);
        assert!(lot_classes(&lot("Com_Bistro_18x22"), 10.0, 0, crate::sim::Age::Toddler, None, None).is_empty());
        // (A fifth off the day's class.)
        let d = discounted(4);
        assert_eq!(price(d, 4), 320);
        assert_eq!(price((d + 1) % CLASSES.len(), 4), 400);
        assert_eq!(class_task(CLASS_TASK + 1).map(|a| a.name), Some("Attend Athletic Class"));
        assert_eq!(class_of(&CLASSES[6].activity), Some("Charisma"));
    }
}
