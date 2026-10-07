//! Changing household (from the pause menu): play another of the town's families. The family
//! played until now stays in town just as they are, kept in the save to come back to, and the
//! one chosen takes over at the same time of the same day, with the town's story and family
//! tree as they stand: a household played before picks up where it was left, a town family never
//! played moves into their own home. Who knew whom goes across too: the new household knows the
//! old one as the old one knew them.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::AppState;
use crate::dialog::{Answer, Answered, Ask, Question, Questions};
use crate::interact::{Household, Notifications};
use crate::save::{SaveGame, SavedSim};
use crate::sim::{Age, HouseholdMember, Sim};
use crate::social::Relationships;

pub struct HouseholdPlugin;

impl Plugin for HouseholdPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Dormant>()
            .add_message::<ChooseHousehold>()
            .add_systems(OnEnter(AppState::Loading), the_town_as_it_is)
            .add_systems(OnEnter(AppState::MainMenu), |mut commands: Commands| {
                commands.remove_resource::<CarryOver>();
                commands.remove_resource::<Switching>();
            })
            .add_systems(Update, (ask_which, chosen, switch_over, carry_over).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// The town's households played before, as they were left (kept in the save to come back to).
#[derive(Resource, Default, Clone)]
pub struct Dormant(pub Vec<SaveGame>);

impl Dormant {
    /// As saved: without anyone who has since joined the household being played (and so without
    /// a household left with no one).
    pub fn kept(&self, playing: &[u64]) -> Vec<SaveGame> {
        self.0
            .iter()
            .filter_map(|d| {
                let mut d = d.clone();
                d.sims.retain(|s| !(s.member && playing.contains(&s.id)));
                d.sims.iter().any(|s| s.member).then(|| {
                    someone_selected(&mut d);
                    d
                })
            })
            .collect()
    }

    /// Everyone living in them.
    pub fn member_ids(&self) -> Vec<u64> {
        member_ids(&self.0)
    }
}

fn member_ids(dormant: &[SaveGame]) -> Vec<u64> {
    dormant.iter().flat_map(|d| d.sims.iter().filter(|s| s.member).map(|s| s.id)).collect()
}

/// (One of the household is the one being played.)
fn someone_selected(g: &mut SaveGame) {
    if !g.sims.iter().any(|s| s.member && s.selected)
        && let Some(s) = g.sims.iter_mut().find(|s| s.member)
    {
        s.selected = true;
    }
}

/// A household to play instead.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    /// One played before (by its place among them).
    Played(usize),
    /// A town family never played (by household id).
    Town(u64),
}

/// Asked for from the pause menu: which household to play.
#[derive(Message)]
pub struct ChooseHousehold;

/// A change of household under way, waiting for the game as it stands.
#[derive(Resource)]
struct Switching(Choice);

/// A town family moving in, and what goes on from the game before: the time, the town's story
/// and family tree, the households played before, and those of their Sims the family knows.
#[derive(Resource, Clone)]
pub struct CarryOver {
    pub minutes: f64,
    pub town: crate::story::TownStory,
    pub family: Vec<crate::family::Person>,
    pub dormant: Vec<SaveGame>,
    pub known: Vec<SavedSim>,
}

/// The game going on with the household played before at `i` (none if there's no such): the
/// household played until now joins those played before, and the time, the town's story and
/// its family tree go on as they are.
pub fn take_up(mut game: SaveGame, i: usize) -> Option<SaveGame> {
    if i >= game.dormant.len() {
        return None;
    }
    let mut rest = std::mem::take(&mut game.dormant);
    let mut next = rest.remove(i);
    next.minutes = game.minutes;
    next.town = std::mem::take(&mut game.town);
    next.family = std::mem::take(&mut game.family);
    rest.push(game);
    for d in &rest {
        acquaint(&mut next, d);
    }
    someone_selected(&mut next);
    next.dormant = rest;
    Some(next)
}

/// A town family never played moving in: the household played until now joins those played
/// before.
pub fn move_in_town(mut game: SaveGame, newcomers: &[u64]) -> CarryOver {
    let mut dormant = std::mem::take(&mut game.dormant);
    let minutes = game.minutes;
    let town = std::mem::take(&mut game.town);
    let family = std::mem::take(&mut game.family);
    dormant.push(game);
    let known = dormant
        .iter()
        .flat_map(|d| d.sims.iter().filter(|s| s.member && s.relationships.iter().any(|r| newcomers.contains(&r.with))))
        .map(known_as)
        .collect();
    CarryOver { minutes, town, family, dormant, known }
}

/// A Sim of another household, as one the household knows (living elsewhere, at home).
fn known_as(s: &SavedSim) -> SavedSim {
    SavedSim { member: false, selected: false, whereabouts: "away".into(), ..s.clone() }
}

/// The Sims of `other` (a household played before) that `game`'s household knows: kept in
/// `game` as Sims they know, with the household's relationships with them as `other` has them.
fn acquaint(game: &mut SaveGame, other: &SaveGame) {
    let ids: Vec<u64> = game.sims.iter().filter(|s| s.member).map(|s| s.id).collect();
    for o in other.sims.iter().filter(|s| s.member) {
        let mut knows = false;
        for r in o.relationships.iter().filter(|r| ids.contains(&r.with)) {
            knows = true;
            let Some(m) = game.sims.iter_mut().find(|s| s.id == r.with) else { continue };
            let theirs = crate::save::SavedRel { with: o.id, ..r.clone() };
            match m.relationships.iter_mut().find(|x| x.with == o.id) {
                // (Whichever is the more recent.)
                Some(x) if x.last > r.last => {}
                Some(x) => *x = theirs,
                None => m.relationships.push(theirs),
            }
        }
        if !knows {
            continue;
        }
        match game.sims.iter_mut().find(|s| s.id == o.id) {
            Some(s) if s.member => {}
            Some(s) => *s = known_as(o),
            None => game.sims.push(known_as(o)),
        }
    }
}

/// Starting to load a game: the town's story and the households played before, as the save
/// (or the game being changed from) has them; a new game starts the town afresh.
fn the_town_as_it_is(mut commands: Commands, save: Option<Res<crate::save::PendingLoad>>, carry: Option<Res<CarryOver>>, mut dormant: ResMut<Dormant>) {
    let (story, d) = match (save, carry) {
        (Some(s), _) => (s.0.town.clone(), s.0.dormant.clone()),
        (None, Some(c)) => (c.town.clone(), c.dormant.clone()),
        (None, None) => Default::default(),
    };
    commands.insert_resource(story);
    dormant.0 = d;
}

/// The Sims of the households played before who are about town (the grown-ups).
pub fn about_town(dormant: &[SaveGame]) -> Vec<Sim> {
    dormant.iter().flat_map(|d| d.members()).filter(|s| matches!(s.age, Age::YoungAdult | Age::Adult | Age::Elder)).collect()
}

/// The households that can be played instead of this one: those played before (but not one
/// whose home this one lives in now), then the town's families never played (not one with
/// anyone of this household or one played before, nor one whose home either lives in, nor one
/// the town has lost). Each with its name and who's in it.
fn offered(
    hh: &Household,
    mine: &[u64],
    dormant: &[SaveGame],
    town: Option<&s3bake::PremadesBaked>,
    story: &crate::story::TownStory,
    world: &crate::loading::WorldInfo,
) -> Vec<(Choice, Answer)> {
    let money = |n: i64| format!("§{}", crate::lifetime::group(n));
    let mut out = Vec::new();
    for (i, d) in dormant.iter().enumerate() {
        if d.lot_index == hh.lot_index {
            continue;
        }
        let names: Vec<String> = d.sims.iter().filter(|s| s.member).map(|s| s.first.clone()).collect();
        out.push((
            Choice::Played(i),
            Answer { label: format!("The {} household", d.household), detail: format!("Played before · {} · {} · {}", names.join(", "), money(d.funds), d.lot_name), icon: String::new() },
        ));
    }
    let Some(town) = town else { return out };
    let gone = member_ids(dormant);
    let taken: Vec<usize> = dormant.iter().map(|d| d.lot_index).chain([hh.lot_index]).collect();
    for h in town.playable() {
        let lot = world.lots.iter().position(|l| l.id == h.lot_id);
        let living: Vec<String> = h
            .members
            .iter()
            .filter(|m| !m.first_name.is_empty())
            .filter_map(|m| story.apply(crate::premade::to_sim(m)))
            .map(|s| s.first)
            .collect();
        // (Not by name: a household of their own may share a town family's surname.)
        if h.name.to_ascii_lowercase().contains("ghost")
            || living.is_empty()
            || h.members.iter().any(|m| mine.contains(&m.id) || gone.contains(&m.id))
            || lot.is_none_or(|l| taken.contains(&l))
        {
            continue;
        }
        let at = lot.and_then(|l| world.lot_names.get(l)).cloned().unwrap_or_default();
        out.push((
            Choice::Town(h.id),
            Answer { label: format!("The {} household", h.name), detail: format!("{} · {} · {at}", living.join(", "), money(h.funds.max(0))), icon: String::new() },
        ));
    }
    out
}

/// Which household to play: put to the player.
#[allow(clippy::too_many_arguments)]
fn ask_which(
    mut asked: MessageReader<ChooseHousehold>,
    mut questions: ResMut<Questions>,
    household: Option<Res<Household>>,
    members: Query<&Sim, With<HouseholdMember>>,
    dormant: Res<Dormant>,
    town: Option<Res<crate::premade::TownPremades>>,
    (story, world): (Res<crate::story::TownStory>, Res<crate::loading::CurrentWorld>),
    mut notes: ResMut<Notifications>,
) {
    if asked.read().count() == 0 {
        return;
    }
    let Some(hh) = household else { return };
    let mine: Vec<u64> = members.iter().map(|s| s.id).collect();
    let (choices, mut answers): (Vec<Choice>, Vec<Answer>) = offered(&hh, &mine, &dormant.0, town.as_ref().map(|t| &*t.0), &story, &world.data).into_iter().unzip();
    if choices.is_empty() {
        notes.push("There's no other household in town to play.".to_string());
        return;
    }
    answers.push(Answer { label: format!("Keep playing the {} household", hh.name), detail: String::new(), icon: String::new() });
    questions.ask(Ask {
        about: Question::Household(choices),
        icon: String::new(),
        heading: "Change Household".into(),
        title: "Which household will you play?".into(),
        text: format!(
            "The {} household will stay in town just as they are, to play again whenever you like. (Changing household doesn't save the game: save once you're playing the new one to keep the change.)",
            hh.name
        ),
        answers,
    });
}

/// A household chosen: the game as it stands is taken (not saved), to change over from.
fn chosen(mut commands: Commands, mut answers: MessageReader<Answered>, mut snapshot: MessageWriter<crate::save::SnapshotRequest>) {
    for a in answers.read() {
        let Question::Household(choices) = &a.about else { continue };
        if let Some(c) = choices.get(a.answer) {
            commands.insert_resource(Switching(c.clone()));
            snapshot.write(crate::save::SnapshotRequest);
        }
    }
}

/// With the game as it stands, over to the household chosen: one played before is loaded as it
/// was left; a town family moves into their home.
#[allow(clippy::too_many_arguments)]
fn switch_over(
    mut commands: Commands,
    switching: Option<Res<Switching>>,
    snapshot: Option<Res<crate::save::Snapshot>>,
    worlds: Res<crate::data::WorldList>,
    slot: Res<crate::save::SaveSlot>,
    town: Option<Res<crate::premade::TownPremades>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let (Some(s), Some(snap)) = (switching, snapshot) else { return };
    commands.remove_resource::<Switching>();
    commands.remove_resource::<crate::save::Snapshot>();
    let game = snap.0.clone();
    let from = game.household.clone();
    match s.0 {
        Choice::Played(i) => {
            let Some(g) = take_up(game, i) else { return };
            info!("changing household: from the {from} household to the {} household (played before)", g.household);
            // (Saved, when it is, to the same file: it's the same game.)
            if crate::save::begin_load(&mut commands, &worlds, g, slot.0.clone()) {
                next.set(AppState::Loading);
            }
        }
        Choice::Town(id) => {
            let Some(h) = town.as_ref().and_then(|t| t.0.households.iter().find(|h| h.id == id)).cloned() else { return };
            let ids: Vec<u64> = h.members.iter().map(|m| m.id).collect();
            let carry = move_in_town(game, &ids);
            let members: Vec<Sim> = h.members.iter().filter(|m| !m.first_name.is_empty()).map(crate::premade::to_sim).filter_map(|s| carry.town.apply(s)).collect();
            info!("changing household: from the {from} household to the {} household (moving in)", h.name);
            commands.insert_resource(crate::home::PendingHousehold { last_name: h.name.clone(), members, premade: Some(h), ties: Vec::new() });
            commands.remove_resource::<crate::save::PendingLoad>();
            commands.insert_resource(carry);
            next.set(AppState::Loading);
        }
    }
}

/// A town family moved in (and their own ties made): the game goes on from where it was, with
/// the town's story and family tree, and the households played before as they knew them.
#[allow(clippy::type_complexity)]
fn carry_over(
    mut commands: Commands,
    carry: Option<Res<CarryOver>>,
    choice: Option<Res<crate::premade::PremadeChoice>>,
    household: Option<Res<Household>>,
    mut clock: ResMut<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &mut Relationships, Has<HouseholdMember>)>,
    mut dormant: ResMut<Dormant>,
    mut notes: ResMut<Notifications>,
) {
    let Some(c) = carry else { return };
    if choice.is_some() || household.is_none() || !sims.iter().any(|q| q.3) {
        return;
    }
    commands.remove_resource::<CarryOver>();
    clock.minutes = c.minutes;
    commands.insert_resource(c.town.clone());
    let people = c.family.clone();
    commands.queue(move |w: &mut World| w.resource_mut::<crate::family::Genealogy>().restore(&people));
    dormant.0 = c.dormant.clone();
    // Who knew whom, both ways.
    let by_id: HashMap<u64, Entity> = sims.iter().map(|q| (q.1.id, q.0)).collect();
    let mut links = Vec::new();
    for s in c.dormant.iter().flat_map(|d| d.sims.iter().filter(|s| s.member)) {
        let Some(&them) = by_id.get(&s.id) else { continue };
        for r in &s.relationships {
            if let Some(&other) = by_id.get(&r.with) {
                links.push((them, other, r.clone()));
            }
        }
    }
    for (a, b, r) in links {
        for (x, y) in [(a, b), (b, a)] {
            if let Ok((_, _, mut rels, _)) = sims.get_mut(x) {
                let e = rels.entry(y);
                e.friendship = r.friendship;
                e.romance = r.romance;
                e.status = crate::save::status_from(&r.status);
                e.kissed = r.kissed;
                e.last = r.last;
            }
        }
    }
    if let Some(h) = household {
        notes.push(format!("You're now playing the {} household.", h.name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(id: u64, member: bool, rels: &[(u64, f32)]) -> SavedSim {
        let mut s: SavedSim = serde_json::from_value(serde_json::json!({
            "id": id, "look": id, "first": format!("S{id}"), "last": "X", "female": false, "age": "Adult", "traits": [],
            "skin": [0.5, 0.5, 0.5], "hair": [0.1, 0.1, 0.1], "top": [0.2, 0.2, 0.2], "bottom": [0.3, 0.3, 0.3],
            "member": member, "selected": false, "whereabouts": "home", "position": [0.0, 0.0, 0.0], "yaw": 0.0, "floor": 1,
            "motives": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0], "skills": [], "moodlets": [], "relationships": []
        }))
        .unwrap();
        s.relationships = rels.iter().map(|&(with, f)| crate::save::SavedRel { with, friendship: f, romance: 0.0, status: "None".into(), kissed: false, last: 10.0 }).collect();
        s
    }

    fn game(name: &str, lot: usize, minutes: f64, sims: Vec<SavedSim>) -> SaveGame {
        let mut g: SaveGame = serde_json::from_value(serde_json::json!({
            "version": 1, "world": "Sunset Valley", "lot_index": lot, "lot_name": "", "household": name, "funds": 100,
            "last_bill_day": 0, "minutes": minutes, "sims": [], "bought": [], "removed": []
        }))
        .unwrap();
        g.sims = sims;
        g
    }

    #[test]
    fn old_saves_have_no_households_played_before() {
        let g = game("Smith", 1, 0.0, vec![sim(1, true, &[])]);
        let json = serde_json::to_string(&g).unwrap();
        assert!(!json.contains("dormant"));
        let back: SaveGame = serde_json::from_str(&json).unwrap();
        assert!(back.dormant.is_empty());
    }

    #[test]
    fn changing_household_and_back() {
        // The Smiths (1, 2) know Bella (10) of the Goths, played before.
        let mut smiths = game("Smith", 1, 5000.0, vec![sim(1, true, &[(10, 40.0)]), sim(2, true, &[]), sim(3, false, &[])]);
        smiths.town.day = 4;
        let goths = game("Goth", 2, 100.0, vec![sim(10, true, &[]), sim(11, true, &[(10, 90.0)])]);
        smiths.dormant = vec![goths];
        let g = take_up(smiths, 0).unwrap();
        // The Goths go on at the Smiths' time, in the town as it is; the Smiths wait.
        assert_eq!(g.household, "Goth");
        assert_eq!(g.minutes, 5000.0);
        assert_eq!(g.town.day, 4);
        assert_eq!(g.dormant.len(), 1);
        assert_eq!(g.dormant[0].household, "Smith");
        assert!(g.dormant[0].dormant.is_empty());
        // Bella knows Sim 1 as Sim 1 knows her, and Sim 1 is someone the Goths know.
        let bella = g.sims.iter().find(|s| s.id == 10).unwrap();
        assert!(bella.relationships.iter().any(|r| r.with == 1 && r.friendship == 40.0));
        let one = g.sims.iter().find(|s| s.id == 1).unwrap();
        assert!(!one.member && one.whereabouts == "away");
        // (Sim 2 knows none of them: not added.)
        assert!(!g.sims.iter().any(|s| s.id == 2));
        assert!(g.sims.iter().any(|s| s.member && s.selected));
        // And back.
        let back = take_up(g, 0).unwrap();
        assert_eq!(back.household, "Smith");
        assert_eq!(back.dormant[0].household, "Goth");
        assert_eq!(back.minutes, 5000.0);
    }

    #[test]
    fn a_town_family_moving_in() {
        let smiths = game("Smith", 1, 700.0, vec![sim(1, true, &[(20, 15.0)]), sim(2, true, &[])]);
        let c = move_in_town(smiths, &[20, 21]);
        assert_eq!(c.minutes, 700.0);
        assert_eq!(c.dormant.len(), 1);
        assert_eq!(c.known.len(), 1);
        assert!(c.known[0].id == 1 && !c.known[0].member);
    }

    #[test]
    fn those_who_joined_leave_their_old_household() {
        let d = Dormant(vec![game("Goth", 2, 0.0, vec![sim(10, true, &[]), sim(11, true, &[])]), game("Alto", 3, 0.0, vec![sim(30, true, &[])])]);
        let kept = d.kept(&[10, 30]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].sims.len(), 1);
        assert!(kept[0].sims[0].selected);
    }
}
