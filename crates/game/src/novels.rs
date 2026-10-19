//! Books read for their own sake, as the game's `BookGeneral`: novels, biographies, poetry,
//! bought at the bookstore (its shop's "General" books) and kept on the household's shelves; read
//! from a bookshelf by anyone, at the book's pages a minute (a Bookworm's own, faster rate), for
//! fun by the game's sum (`Book.BaseReadPreLoop`: a tenth less each time it's been finished
//! before, a hundredth more for each simoleon it cost, a tenth to twice the usual; twice that
//! for a Bookworm), the place kept between sittings. A career's boss may hand one over to read
//! and report on at work (the game's `kReadBook` opportunities).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::sim::Sim;

pub struct NovelsPlugin;

impl Plugin for NovelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (read_on, novel_test).run_if(in_state(PlayMode::Live)));
    }
}

/// How far a Sim has got with a book, and whether it's one they brought home.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NovelRead {
    pub key: String,
    /// On the household's shelves, theirs.
    #[serde(default)]
    pub owned: bool,
    /// Pages read (of the reading under way).
    #[serde(default)]
    pub pages: f32,
    /// Times read to the end.
    #[serde(default)]
    pub finished: u32,
}

/// A Sim's books: those they brought home, and how far they've got with each they've opened.
#[derive(Component, Default, Clone, Debug)]
pub struct Novels(pub Vec<NovelRead>);

impl Novels {
    pub fn get(&self, key: &str) -> Option<&NovelRead> {
        self.0.iter().find(|n| n.key == key)
    }

    fn entry(&mut self, key: &str) -> &mut NovelRead {
        if let Some(i) = self.0.iter().position(|n| n.key == key) {
            return &mut self.0[i];
        }
        self.0.push(NovelRead { key: key.to_string(), ..default() });
        self.0.last_mut().unwrap()
    }

    /// Theirs, on the shelf.
    pub fn own(&mut self, key: &str) {
        self.entry(key).owned = true;
    }

    /// Read to the end at least once.
    pub fn finished(&self, key: &str) -> bool {
        self.get(key).is_some_and(|n| n.finished > 0)
    }
}

/// Book choices (an action's `book_choice`) from this up are novels: this plus the novel's index.
pub const NOVEL_CHOICE: u16 = 10_000;

/// Reading a novel (its index), from the shelf to the chair.
#[derive(Component)]
pub struct ReadingNovel(pub usize);

/// The household's books (owned by any of them).
pub fn shelved<'a>(members: impl Iterator<Item = &'a Novels>) -> Vec<String> {
    let mut keys: Vec<String> = members.flat_map(|b| b.0.iter().filter(|x| x.owned).map(|x| x.key.clone())).collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The books on the household's shelves, for a Sim to read: each with how far they are into it
/// (or that they've read it), by title.
pub fn readable(novels: &[s3bake::gamedata::NovelInfo], owned: &[String], mine: Option<&Novels>) -> Vec<(usize, String)> {
    let mut v: Vec<(usize, String)> = novels
        .iter()
        .enumerate()
        .filter(|(_, b)| owned.contains(&b.key))
        .map(|(i, b)| {
            let read = mine.and_then(|m| m.get(&b.key));
            let label = match read {
                Some(r) if r.pages > 0.0 => format!("{} ({}%)", b.name, (r.pages / b.pages.max(1) as f32 * 100.0) as u32),
                Some(r) if r.finished > 0 => format!("{} (read)", b.name),
                _ => b.name.clone(),
            };
            (i, label)
        })
        .collect();
    v.sort_by(|a, b| a.1.cmp(&b.1));
    v
}

/// The novel a label names (the title, or the title and how far through).
pub fn by_label(novels: &[s3bake::gamedata::NovelInfo], label: &str) -> Option<usize> {
    novels.iter().position(|b| label == b.name || label.starts_with(&format!("{} (", b.name)))
}

/// The fun of a sitting with a book, against the usual (`Book.BaseReadPreLoop`).
pub fn fun_scale(price: i64, times_finished: u32, bookworm: bool) -> f32 {
    let v = (1.0 - 0.1 * times_finished as f32 + price as f32 * 0.01).clamp(0.1, 2.0);
    if bookworm { v * 2.0 } else { v }
}

/// What reading in a chair does for fun an hour (`interact`'s seated "Read Book").
const READ_FUN_PER_HOUR: f32 = 30.0;

/// Reading a novel in a chair: the pages turned, the fun of it as the book's, the place kept, and
/// the book finished at its last page (and the next reading begun from the first).
#[allow(clippy::type_complexity)]
fn read_on(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    mut sims: Query<(Entity, &Sim, &ReadingNovel, &mut crate::interact::ActionQueue, Option<&mut Novels>, &mut crate::sim::Motives, Has<crate::surroundings::ReadSomewhere>)>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut dreams: MessageWriter<crate::dreams::DreamEvent>,
) {
    let Some(ui) = ui else { return };
    for (e, sim, reading, mut queue, novels, mut motives, finding_a_seat) in &mut sims {
        let Some(b) = ui.data.novels.get(reading.0) else {
            commands.entity(e).remove::<ReadingNovel>();
            continue;
        };
        // (Still at the shelf, or on the way to a chair: not reading yet; anything else, done.)
        let front = queue.0.front();
        let reading_now = front.is_some_and(|a| a.label == "Read Book" && matches!(a.phase, crate::interact::Phase::Running(_)));
        let on_the_way = finding_a_seat || front.is_some_and(|a| a.label == "Read a Book" || (a.label == "Read Book" && !matches!(a.phase, crate::interact::Phase::Running(_))));
        if !reading_now {
            if !on_the_way {
                commands.entity(e).remove::<ReadingNovel>();
            }
            continue;
        }
        let minutes = delta.0;
        if minutes <= 0.0 {
            continue;
        }
        // (Read on till it's finished: put down only for a pressing need.)
        if let Some(a) = queue.0.front_mut() {
            let pressing = [crate::sim::HUNGER, crate::sim::BLADDER, crate::sim::ENERGY].iter().any(|n| motives.0[*n] < -50.0);
            match a.phase {
                _ if pressing => a.cancel = true,
                crate::interact::Phase::Running(t) if t > 45.0 => a.phase = crate::interact::Phase::Running(15.0),
                _ => {}
            }
        }
        let Some(mut novels) = novels else {
            commands.entity(e).insert(Novels::default());
            continue;
        };
        let bookworm = sim.traits.contains(&crate::life::Trait::Bookworm);
        let entry = novels.entry(&b.key);
        // (The fun as the book's: the chair's own is the usual.)
        let scale = fun_scale(b.price, entry.finished, bookworm);
        motives.add(crate::sim::FUN, READ_FUN_PER_HOUR * (scale - 1.0) * minutes / 60.0);
        let rate = if bookworm { b.bookworm_pages_per_minute } else { b.pages_per_minute };
        entry.pages = (entry.pages + rate * minutes).min(b.pages as f32);
        if entry.pages >= b.pages as f32 {
            entry.pages = 0.0;
            entry.finished += 1;
            let stop = if b.name.ends_with(['!', '?', '.']) { "" } else { "." };
            notes.push(format!("{} finished reading {} by {}{stop}", sim.first, b.name, b.author));
            dreams.write(crate::dreams::DreamEvent { key: Some(b.key.clone()), ..crate::dreams::DreamEvent::new(e, "kReadBook") });
            if let Some(a) = queue.0.front_mut() {
                a.cancel = true;
            }
            commands.entity(e).remove::<ReadingNovel>();
        }
    }
}

/// NOVEL=<title words> (tests): the selected Sim's household has that book on its shelf (`@opp`:
/// the book of their opportunity), and NOVEL_READ=1 sets them reading it from the nearest
/// bookshelf.
fn novel_test(
    mut commands: Commands,
    time: Res<Time>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut sel: Query<(Entity, Option<&mut Novels>, &mut crate::interact::ActionQueue, &Transform, Option<&crate::opportunities::SimOpportunities>), With<crate::sim::Selected>>,
    shelves: Query<(Entity, &crate::interact::GameObject, &Transform)>,
    mut done: Local<bool>,
) {
    let Ok(want) = std::env::var("NOVEL") else { return };
    if *done || time.elapsed_secs() < 15.0 {
        return;
    }
    let (Some(ui), Ok((e, novels, mut queue, tf, opps))) = (ui, sel.single_mut()) else { return };
    let want = match want.as_str() {
        // (Once the opportunity's taken.)
        "@opp" => match opps.and_then(|o| o.active.iter().find_map(|a| ui.data.opportunities.get(a.index).map(|x| x.book.clone()).filter(|b| !b.is_empty()))) {
            Some(b) => b,
            None => return,
        },
        w => w.to_string(),
    }
    .to_ascii_lowercase();
    *done = true;
    let Some(i) = ui.data.novels.iter().position(|b| !want.is_empty() && (b.name.to_ascii_lowercase().contains(&want) || b.key.to_ascii_lowercase().contains(&want))) else {
        info!("novel test: no book like '{want}' of {}", ui.data.novels.len());
        return;
    };
    let b = &ui.data.novels[i];
    info!("novel test: {} by {} ({}, {} pages, §{})", b.name, b.author, b.genre, b.pages, b.price);
    match novels {
        Some(mut n) => n.own(&b.key),
        None => {
            let mut n = Novels::default();
            n.own(&b.key);
            commands.entity(e).insert(n);
        }
    }
    if std::env::var("NOVEL_READ").is_ok() {
        let def = crate::interact::interactions_for(crate::interact::ObjectKind::Bookshelf).iter().position(|d| d.special == crate::interact::Special::GetBook);
        let shelf = shelves.iter().filter(|s| s.1.kind == crate::interact::ObjectKind::Bookshelf).min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
        if let (Some(def), Some((s, ..))) = (def, shelf) {
            let mut a = crate::interact::Action::new("Read a Book", crate::interact::ActionKind::Object { target: s, def }, false);
            a.book_choice = Some(NOVEL_CHOICE + i as u16);
            queue.push_player(a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use s3bake::gamedata::NovelInfo;

    #[test]
    fn a_book_is_read_for_fun_as_the_games_and_kept_on_the_shelf() {
        // (Dearer books more fun, each reading less; a Bookworm's twice; within a tenth and twice.)
        assert!((fun_scale(0, 0, false) - 1.0).abs() < 1e-5);
        assert!((fun_scale(25, 0, false) - 1.25).abs() < 1e-5);
        assert!((fun_scale(25, 3, false) - 0.95).abs() < 1e-5);
        assert!((fun_scale(500, 0, false) - 2.0).abs() < 1e-5);
        assert!((fun_scale(0, 20, false) - 0.1).abs() < 1e-5);
        assert!((fun_scale(25, 0, true) - 2.5).abs() < 1e-5);
        let book = |key: &str, name: &str| NovelInfo { key: key.into(), name: name.into(), pages: 200, ..Default::default() };
        let all = [book("AB", "Zebra Tales"), book("CD", "Apple Days"), book("EF", "Not Ours")];
        let mut mine = Novels::default();
        mine.own("AB");
        mine.entry("AB").pages = 50.0;
        let mut theirs = Novels::default();
        theirs.own("CD");
        theirs.entry("CD").finished = 1;
        let owned = shelved([&mine, &theirs].into_iter());
        assert_eq!(owned, ["AB", "CD"]);
        // (The household's, by title, how far this Sim is.)
        let r = readable(&all, &owned, Some(&mine));
        assert_eq!(r, [(1, "Apple Days".to_string()), (0, "Zebra Tales (25%)".to_string())]);
        assert_eq!(readable(&all, &owned, Some(&theirs))[0].1, "Apple Days (read)");
        assert_eq!(by_label(&all, "Zebra Tales (25%)"), Some(0));
        assert!(theirs.finished("CD") && !mine.finished("AB"));
    }
}
