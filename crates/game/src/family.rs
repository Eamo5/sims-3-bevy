//! Families: who is whose parent, for everyone the household has heard of. Filled from the
//! town's premade families and from the household's babies born and children adopted, and
//! kept in saves. From it come the family words the Relationships panel uses
//! (mother, son, sister, grandfather, aunt, cousin...), and the game's rule that relatives
//! don't romance one another.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;

pub struct FamilyPlugin;

impl Plugin for FamilyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Genealogy>()
            .add_systems(OnEnter(AppState::Loading), |mut g: ResMut<Genealogy>| *g = Genealogy::default())
            .add_systems(Update, from_town.run_if(in_state(AppState::InGame)));
    }
}

/// Someone in a family tree.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Person {
    pub id: u64,
    pub name: String,
    pub female: bool,
    pub parents: Vec<u64>,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct Genealogy(pub HashMap<u64, Person>);

/// What one Sim is to another.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kin {
    Parent,
    Child,
    Sibling,
    Grandparent,
    Grandchild,
    AuntUncle,
    NieceNephew,
    Cousin,
}

impl Kin {
    /// The word for it, for a woman or a man.
    pub fn word(self, female: bool) -> &'static str {
        match (self, female) {
            (Kin::Parent, true) => "Mother",
            (Kin::Parent, false) => "Father",
            (Kin::Child, true) => "Daughter",
            (Kin::Child, false) => "Son",
            (Kin::Sibling, true) => "Sister",
            (Kin::Sibling, false) => "Brother",
            (Kin::Grandparent, true) => "Grandmother",
            (Kin::Grandparent, false) => "Grandfather",
            (Kin::Grandchild, true) => "Granddaughter",
            (Kin::Grandchild, false) => "Grandson",
            (Kin::AuntUncle, true) => "Aunt",
            (Kin::AuntUncle, false) => "Uncle",
            (Kin::NieceNephew, true) => "Niece",
            (Kin::NieceNephew, false) => "Nephew",
            (Kin::Cousin, _) => "Cousin",
        }
    }
}

impl Genealogy {
    /// Someone's name and sex (their parents kept).
    pub fn note(&mut self, id: u64, name: &str, female: bool) {
        let p = self.0.entry(id).or_insert_with(|| Person { id, ..default() });
        if !name.is_empty() {
            p.name = name.to_string();
        }
        p.female = female;
    }

    pub fn add_parent(&mut self, child: u64, parent: u64) {
        if child == parent {
            return;
        }
        let p = self.0.entry(child).or_insert_with(|| Person { id: child, ..default() });
        if !p.parents.contains(&parent) && p.parents.len() < 2 {
            p.parents.push(parent);
        }
    }

    pub fn parents(&self, id: u64) -> &[u64] {
        self.0.get(&id).map_or(&[], |p| &p.parents)
    }

    pub fn children(&self, id: u64) -> Vec<u64> {
        let mut v: Vec<u64> = self.0.values().filter(|p| p.parents.contains(&id)).map(|p| p.id).collect();
        v.sort();
        v
    }

    fn grandparents(&self, id: u64) -> Vec<u64> {
        self.parents(id).iter().flat_map(|p| self.parents(*p).iter().copied()).collect()
    }

    fn siblings(&self, a: u64, b: u64) -> bool {
        a != b && self.parents(a).iter().any(|p| self.parents(b).contains(p))
    }

    /// What `b` is to `a`, if they're family.
    pub fn kin(&self, a: u64, b: u64) -> Option<Kin> {
        if a == b {
            return None;
        }
        let (pa, pb) = (self.parents(a), self.parents(b));
        if pa.contains(&b) {
            return Some(Kin::Parent);
        }
        if pb.contains(&a) {
            return Some(Kin::Child);
        }
        if self.siblings(a, b) {
            return Some(Kin::Sibling);
        }
        let (ga, gb) = (self.grandparents(a), self.grandparents(b));
        if ga.contains(&b) {
            return Some(Kin::Grandparent);
        }
        if gb.contains(&a) {
            return Some(Kin::Grandchild);
        }
        if pa.iter().any(|p| self.siblings(*p, b)) {
            return Some(Kin::AuntUncle);
        }
        if pb.iter().any(|p| self.siblings(*p, a)) {
            return Some(Kin::NieceNephew);
        }
        if ga.iter().any(|g| gb.contains(g)) {
            return Some(Kin::Cousin);
        }
        None
    }

    /// The family word for what `b` is to `a` ("Mother", "Cousin").
    pub fn word(&self, a: u64, b: u64) -> Option<&'static str> {
        let female = self.0.get(&b).is_some_and(|p| p.female);
        self.kin(a, b).map(|k| k.word(female))
    }

    pub fn saved(&self) -> Vec<Person> {
        let mut v: Vec<Person> = self.0.values().cloned().collect();
        v.sort_by_key(|p| p.id);
        v
    }

    /// A save's families (on top of the town's).
    pub fn restore(&mut self, people: &[Person]) {
        for p in people {
            self.note(p.id, &p.name, p.female);
            for &parent in &p.parents {
                self.add_parent(p.id, parent);
            }
        }
    }
}

/// The town's premade families: parents and children from the world's own records.
fn from_town(town: Option<Res<crate::premade::TownPremades>>, mut g: ResMut<Genealogy>, mut done: Local<bool>) {
    let Some(town) = town else {
        *done = false;
        return;
    };
    if *done && !town.is_changed() {
        return;
    }
    *done = true;
    for h in &town.0.households {
        for s in &h.members {
            g.note(s.id, &format!("{} {}", s.first_name, s.last_name), s.female);
            for &p in &s.parents {
                g.add_parent(s.id, p);
            }
            for &c in &s.children {
                g.add_parent(c, s.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grandma (1) and Grandpa (2) have Mum (3) and Aunt (4); Mum and Dad (5) have Kid (6)
    /// and Sis (7); Aunt has Cousin (8).
    fn tree() -> Genealogy {
        let mut g = Genealogy::default();
        for (id, female) in [(1, true), (2, false), (3, true), (4, true), (5, false), (6, false), (7, true), (8, false)] {
            g.note(id, &format!("Sim {id}"), female);
        }
        for (child, parent) in [(3, 1), (3, 2), (4, 1), (4, 2), (6, 3), (6, 5), (7, 3), (7, 5), (8, 4)] {
            g.add_parent(child, parent);
        }
        g
    }

    #[test]
    fn kin() {
        let g = tree();
        assert_eq!(g.kin(6, 3), Some(Kin::Parent));
        assert_eq!(g.word(6, 3), Some("Mother"));
        assert_eq!(g.word(3, 6), Some("Son"));
        assert_eq!(g.word(6, 7), Some("Sister"));
        assert_eq!(g.word(7, 6), Some("Brother"));
        assert_eq!(g.word(6, 1), Some("Grandmother"));
        assert_eq!(g.word(2, 7), Some("Granddaughter"));
        assert_eq!(g.word(6, 4), Some("Aunt"));
        assert_eq!(g.word(4, 6), Some("Nephew"));
        assert_eq!(g.word(6, 8), Some("Cousin"));
        // In-laws and strangers aren't kin.
        assert_eq!(g.kin(5, 1), None);
        assert_eq!(g.kin(3, 5), None);
        assert_eq!(g.kin(6, 99), None);
    }

    #[test]
    fn saved_and_restored() {
        let g = tree();
        let mut h = Genealogy::default();
        h.restore(&g.saved());
        assert_eq!(h.word(6, 8), Some("Cousin"));
        assert_eq!(h.parents(6), g.parents(6));
    }
}
