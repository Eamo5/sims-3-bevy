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
            .add_systems(Update, (from_town, apply_ties).run_if(in_state(AppState::InGame)));
    }
}

/// Someone in a family tree.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Person {
    pub id: u64,
    pub name: String,
    pub female: bool,
    pub parents: Vec<u64>,
    /// Brothers and sisters known without parents (made so in Create a Sim).
    #[serde(default)]
    pub siblings: Vec<u64>,
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

    pub fn add_sibling(&mut self, a: u64, b: u64) {
        if a == b {
            return;
        }
        for (x, y) in [(a, b), (b, a)] {
            let p = self.0.entry(x).or_insert_with(|| Person { id: x, ..default() });
            if !p.siblings.contains(&y) {
                p.siblings.push(y);
            }
        }
    }

    pub fn siblings(&self, a: u64, b: u64) -> bool {
        a != b && (self.parents(a).iter().any(|p| self.parents(b).contains(p)) || self.0.get(&a).is_some_and(|p| p.siblings.contains(&b)))
    }

    /// Everyone who is a brother or sister to `id`.
    pub fn siblings_of(&self, id: u64) -> Vec<u64> {
        let mut v: Vec<u64> = self.0.values().filter(|p| self.siblings(id, p.id)).map(|p| p.id).collect();
        v.sort();
        v
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
        if ga.iter().any(|g| gb.contains(g)) || pa.iter().any(|p| pb.iter().any(|q| self.siblings(*p, *q))) {
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
            for &s in &p.siblings {
                self.add_sibling(p.id, s);
            }
        }
    }
}

/// What two Sims of a household made in Create a Sim are to each other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tie {
    Roommates,
    Spouses,
    Partners,
    Siblings,
    /// The first is the second's parent.
    ParentOf,
    /// The second is the first's parent.
    ChildOf,
}

impl Tie {
    pub const ALL: [Tie; 6] = [Tie::Roommates, Tie::Spouses, Tie::Partners, Tie::Siblings, Tie::ParentOf, Tie::ChildOf];

    /// Whether two Sims of these ages can be this to each other.
    pub fn fits(self, a: crate::sim::Age, b: crate::sim::Age) -> bool {
        let rank = |x: crate::sim::Age| x as u8;
        match self {
            Tie::Roommates | Tie::Siblings => true,
            Tie::Spouses => a.is_grown() && b.is_grown(),
            Tie::Partners => a.is_grown() == b.is_grown() && !a.is_little() && !b.is_little() && a != crate::sim::Age::Child && b != crate::sim::Age::Child,
            Tie::ParentOf => a.is_grown() && rank(a) > rank(b),
            Tie::ChildOf => b.is_grown() && rank(b) > rank(a),
        }
    }

    /// "Gretchen and Elvis are spouses".
    pub fn describe(self, a: &str, b: &str) -> String {
        match self {
            Tie::Roommates => format!("{a} & {b}: Roommates"),
            Tie::Spouses => format!("{a} & {b}: Spouses"),
            Tie::Partners => format!("{a} & {b}: Partners"),
            Tie::Siblings => format!("{a} & {b}: Siblings"),
            Tie::ParentOf => format!("{a} is {b}'s parent"),
            Tie::ChildOf => format!("{b} is {a}'s parent"),
        }
    }
}

/// A household made in Create a Sim, just moved in: what its Sims are to each other (by Sim
/// id), to be set once they're all about.
#[derive(Resource, Clone)]
pub struct HouseholdTies {
    pub members: Vec<u64>,
    pub ties: Vec<(u64, u64, Tie)>,
}

/// Household ties made real: marriages and partners, family (in the family tree too), and
/// housemates who at least know each other.
fn apply_ties(
    mut commands: Commands,
    ties: Option<Res<HouseholdTies>>,
    mut sims: Query<(Entity, &crate::sim::Sim, &mut crate::social::Relationships), With<crate::sim::HouseholdMember>>,
    mut g: ResMut<Genealogy>,
) {
    use crate::social::RelStatus;
    let Some(t) = ties else { return };
    let by_id: HashMap<u64, (Entity, String, bool)> = sims.iter().map(|(e, s, _)| (s.id, (e, s.full_name(), s.female))).collect();
    if !t.members.iter().all(|id| by_id.contains_key(id)) {
        return;
    }
    commands.remove_resource::<HouseholdTies>();
    for (id, (_, name, female)) in &by_id {
        g.note(*id, name, *female);
    }
    let tie_of = |a: u64, b: u64| t.ties.iter().find(|(x, y, _)| (*x, *y) == (a, b) || (*x, *y) == (b, a)).map(|(x, _, k)| (*x == a, *k));
    let mut links = Vec::new();
    for (i, &a) in t.members.iter().enumerate() {
        for &b in &t.members[i + 1..] {
            let (friendship, romance, status) = match tie_of(a, b) {
                Some((_, Tie::Spouses)) => (80.0, 80.0, RelStatus::Married),
                Some((_, Tie::Partners)) => (60.0, 60.0, RelStatus::Partner),
                Some((_, Tie::Siblings)) => {
                    g.add_sibling(a, b);
                    (60.0, 0.0, RelStatus::None)
                }
                Some((first_is_a, Tie::ParentOf)) => {
                    let (p, c) = if first_is_a { (a, b) } else { (b, a) };
                    g.add_parent(c, p);
                    (70.0, 0.0, RelStatus::None)
                }
                Some((first_is_a, Tie::ChildOf)) => {
                    let (c, p) = if first_is_a { (a, b) } else { (b, a) };
                    g.add_parent(c, p);
                    (70.0, 0.0, RelStatus::None)
                }
                _ => (25.0, 0.0, RelStatus::None),
            };
            links.push((by_id[&a].0, by_id[&b].0, friendship, romance, status));
        }
    }
    for (a, b, friendship, romance, status) in links {
        for (x, y) in [(a, b), (b, a)] {
            if let Ok((_, _, mut rels)) = sims.get_mut(x) {
                let r = rels.entry(y);
                r.friendship = r.friendship.max(friendship);
                r.romance = r.romance.max(romance);
                if status != RelStatus::None {
                    r.status = status;
                    r.kissed = true;
                }
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
    fn siblings_made_in_cas() {
        // Two sisters with no parents in the tree (20, 21); 20 has a son (22), 21 a daughter (23).
        let mut g = Genealogy::default();
        for (id, female) in [(20, true), (21, true), (22, false), (23, true)] {
            g.note(id, "", female);
        }
        g.add_sibling(20, 21);
        g.add_parent(22, 20);
        g.add_parent(23, 21);
        assert_eq!(g.word(20, 21), Some("Sister"));
        assert_eq!(g.word(22, 21), Some("Aunt"));
        assert_eq!(g.word(21, 22), Some("Nephew"));
        assert_eq!(g.word(22, 23), Some("Cousin"));
        assert_eq!(g.siblings_of(20), vec![21]);
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
