//! Skill books, as the game's (`BookSkill`): bought at the bookstore ("Handiness Vol. 1: Unplug
//! It First!", §50), shelved for the household, and read at a bookshelf by anyone whose skill is
//! within the book's levels: a page a minute (a Bookworm a quarter more), each page worth the
//! book's points in its skill, read a sitting at a time (the place kept) until it's finished.

use bevy::prelude::*;

use crate::PlayMode;
use crate::sim::Sim;

pub struct SkillBooksPlugin;

impl Plugin for SkillBooksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (book_bought, read_on).run_if(in_state(PlayMode::Live)));
    }
}

/// Visit activity numbers for buying a skill book: this plus its index in the game data's.
pub const BOOK_TASK: usize = 5000;

/// Buying a book at the bookstore: a few minutes' browse, the price the book's.
pub static BUY_BOOK: crate::rabbitholes::Activity = crate::rabbitholes::Activity {
    name: "Buy a Skill Book",
    minutes: 20.0,
    cost: 0,
    per_hour: [-2.0, -2.0, -1.0, 5.0, 0.0, 10.0],
    skill: None,
    open: 9.0,
    close: 21.0,
};

/// The skill books a Sim has: the ones they bought (on the household's shelves), and the pages
/// they've read of each (by the game's key).
#[derive(Component, Default, Clone, Debug)]
pub struct SkillBooks(pub Vec<(String, f32)>);

impl SkillBooks {
    pub fn pages(&self, key: &str) -> Option<f32> {
        self.0.iter().find(|b| b.0 == key).map(|b| b.1)
    }
}

/// Off to buy a book (its index in the game data's).
#[derive(Component)]
pub struct BuyingBook(pub usize);

/// Back from the bookstore with it.
#[derive(Component)]
pub struct BookBought;

/// Reading a skill book (its index), from the shelf to the chair.
#[derive(Component)]
pub struct ReadingBook(pub usize);

/// The household's skill books (owned by any of them).
pub fn shelved<'a>(members: impl Iterator<Item = &'a SkillBooks>) -> Vec<String> {
    let mut keys: Vec<String> = members.flat_map(|b| b.0.iter().map(|x| x.0.clone())).collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The skills here the game's books are for.
fn known_skill(skill: &str) -> bool {
    crate::save::SKILLS.iter().any(|s| *s == skill)
}

/// The books the bookstore sells the household (those it hasn't), with their prices.
pub fn for_sale(books: &[s3bake::gamedata::SkillBookInfo], owned: &[String]) -> Vec<(usize, String)> {
    books
        .iter()
        .enumerate()
        .filter(|(_, b)| known_skill(&b.skill) && !owned.contains(&b.key))
        .map(|(i, b)| (i, format!("{} (§{})", b.name, b.price)))
        .collect()
}

/// The books on the household's shelves a Sim could learn from now (their skill within the
/// book's levels, not finished), with how far they are.
pub fn readable(books: &[s3bake::gamedata::SkillBookInfo], owned: &[String], mine: Option<&SkillBooks>, skills: &crate::interact::Skills) -> Vec<(usize, String)> {
    books
        .iter()
        .enumerate()
        .filter(|(_, b)| owned.contains(&b.key) && known_skill(&b.skill))
        .filter_map(|(i, b)| {
            let level = skills.level(&b.skill);
            let read = mine.and_then(|m| m.pages(&b.key)).unwrap_or(0.0);
            if level < b.min || level >= b.max.max(b.min + 1) || read >= b.pages as f32 {
                return None;
            }
            let done = (read / b.pages.max(1) as f32 * 100.0) as u32;
            Some((i, if done > 0 { format!("{} ({done}%)", b.name) } else { b.name.clone() }))
        })
        .collect()
}

/// Back from the bookstore: the book's the household's.
fn book_bought(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &BuyingBook, Option<&mut SkillBooks>), With<BookBought>>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut dreams: MessageWriter<crate::dreams::DreamEvent>,
) {
    let Some(ui) = ui else { return };
    for (e, sim, buying, books) in &mut sims {
        commands.entity(e).remove::<(BuyingBook, BookBought)>();
        // (Back from the bookstore: the game's `kReceivedRabbitHoleTreatment` there.)
        dreams.write(crate::dreams::DreamEvent { key: Some("Bookstore".into()), ..crate::dreams::DreamEvent::new(e, "kReceivedRabbitHoleTreatment") });
        let Some(b) = ui.data.skill_books.get(buying.0) else { continue };
        let Some(h) = household.as_deref_mut() else { continue };
        if h.funds < b.price {
            notes.push(format!("{} can't afford {} (§{}).", sim.first, b.name, b.price));
            continue;
        }
        h.funds -= b.price;
        match books {
            Some(mut books) if books.pages(&b.key).is_none() => books.0.push((b.key.clone(), 0.0)),
            Some(_) => {}
            None => {
                commands.entity(e).insert(SkillBooks(vec![(b.key.clone(), 0.0)]));
            }
        }
        notes.push(format!("{} bought {} by {} and put it on the shelf.", sim.first, b.name, b.author));
    }
}

/// Reading a skill book in a chair: the pages turned, and the skill learned from them; the place
/// kept, and the book finished at its last page.
#[allow(clippy::type_complexity)]
fn read_on(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    mut sims: Query<(Entity, &Sim, &ReadingBook, &mut crate::interact::ActionQueue, &mut crate::interact::Skills, Option<&mut SkillBooks>, &crate::sim::Motives, Has<crate::surroundings::ReadSomewhere>)>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut life: MessageWriter<crate::life::LifeEvent>,
    mut dreams: MessageWriter<crate::dreams::DreamEvent>,
) {
    let Some(ui) = ui else { return };
    for (e, sim, reading, mut queue, mut skills, books, motives, finding_a_seat) in &mut sims {
        let Some(b) = ui.data.skill_books.get(reading.0) else {
            commands.entity(e).remove::<ReadingBook>();
            continue;
        };
        // (Still at the shelf, or on the way to a chair: not reading yet; anything else, done.)
        let front = queue.0.front_mut();
        let reading_now = front.as_ref().is_some_and(|a| a.label == "Read Book" && matches!(a.phase, crate::interact::Phase::Running(_)));
        let on_the_way = finding_a_seat || front.as_ref().is_some_and(|a| a.label == "Read a Book" || (a.label == "Read Book" && !matches!(a.phase, crate::interact::Phase::Running(_))));
        if !reading_now {
            if !on_the_way {
                commands.entity(e).remove::<ReadingBook>();
            }
            continue;
        }
        let minutes = delta.0;
        if minutes <= 0.0 {
            continue;
        }
        // (Read on till it's finished, the game's: put down only for a pressing need.)
        if let Some(a) = queue.0.front_mut() {
            let pressing = [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|n| motives.0[*n] < -50.0);
            match a.phase {
                _ if pressing => a.cancel = true,
                crate::interact::Phase::Running(t) if t > 45.0 => a.phase = crate::interact::Phase::Running(15.0),
                _ => {}
            }
        }
        let rate = if sim.traits.contains(&crate::life::Trait::Bookworm) { b.bookworm_pages_per_minute } else { b.pages_per_minute };
        let mut books = match books {
            Some(b) => b,
            None => {
                commands.entity(e).insert(SkillBooks(vec![(b.key.clone(), 0.0)]));
                continue;
            }
        };
        if books.pages(&b.key).is_none() {
            books.0.push((b.key.clone(), 0.0));
        }
        let Some(entry) = books.0.iter_mut().find(|x| x.0 == b.key) else { continue };
        let before = entry.1;
        entry.1 = (entry.1 + rate * minutes).min(b.pages as f32);
        let pages = entry.1 - before;
        let skill = crate::save::SKILLS.iter().find(|s| **s == b.skill).copied();
        if let Some(sk) = skill
            && let Some(level) = skills.add_points(sk, pages * b.points_per_page)
        {
            notes.push(format!("{} reached level {level} in {sk}!", sim.first));
            life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: sk, level }));
        }
        if entry.1 >= b.pages as f32 {
            // (No full stop after a title's own "!".)
            let stop = if b.name.ends_with(['!', '?', '.']) { "" } else { "." };
            notes.push(format!("{} finished reading {}{stop}", sim.first, b.name));
            // (The game's `kReadBook`: a skill book, its skill.)
            dreams.write(crate::dreams::DreamEvent { key: Some(b.key.clone()), skill: skill.map(str::to_string), ..crate::dreams::DreamEvent::new(e, "kReadBook") });
            if let Some(a) = queue.0.front_mut() {
                a.cancel = true;
            }
            commands.entity(e).remove::<ReadingBook>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn books_are_sold_and_read_within_their_levels() {
        let book = |key: &str, skill: &str, min: u32, max: u32| s3bake::gamedata::SkillBookInfo { key: key.into(), name: key.into(), skill: skill.into(), min, max, pages: 200, price: 50, points_per_page: 6.0, ..Default::default() };
        let books = vec![book("ImproveHandiness1", "Handiness", 0, 3), book("ImproveHandiness2", "Handiness", 3, 6), book("ImproveBotSkill1", "BotBuilding", 0, 3)];
        // (Robot building's not a skill here: not sold.)
        assert_eq!(for_sale(&books, &[]).len(), 2);
        assert_eq!(for_sale(&books, &["ImproveHandiness1".into()]).len(), 1);
        let owned: Vec<String> = vec!["ImproveHandiness1".into(), "ImproveHandiness2".into()];
        let mut skills = crate::interact::Skills::default();
        // (A beginner: the first volume; half read, its place shown.)
        assert_eq!(readable(&books, &owned, None, &skills).iter().map(|r| r.0).collect::<Vec<_>>(), [0]);
        let mine = SkillBooks(vec![("ImproveHandiness1".into(), 100.0)]);
        assert_eq!(readable(&books, &owned, Some(&mine), &skills)[0].1, "ImproveHandiness1 (50%)");
        // (At level 3, the second.)
        skills.0.insert("Handiness", 3.0);
        assert_eq!(readable(&books, &owned, None, &skills).iter().map(|r| r.0).collect::<Vec<_>>(), [1]);
        assert_eq!(shelved([&mine, &SkillBooks(vec![("ImproveHandiness1".into(), 5.0)])].into_iter()), ["ImproveHandiness1"]);
    }
}
