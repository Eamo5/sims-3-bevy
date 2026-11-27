//! Photography's subjects, as the game's (`PhotographySubjects`, `SubjectScoringFunctions`): what
//! a camera's aimed at is scored against every subject in the `Photography` table by its own
//! method (a sofa in the frame, two Sims, a couple in love by night, the maid at work, a Sim
//! fishing, a tomato plant...), each thing in the frame worth the share of the frame it covers,
//! a subject made of others' scores (`Op_And`, `Op_All`, `Op_AtLeastOne`, `KeyObjectCount`); the
//! best, scaled by its collection and many times over if the photographer's never caught it (or
//! not today), is the photo's subject, named under the frame as they aim ("A Couch (Still
//! Life)"). A new subject's worth its skill points and simoleons over the photo's own, half again
//! the first time each day; its collection makes the photo worth more, more still once complete;
//! and the photo takes the subject's title ("My relative Jack Bunch"). Each photographer's
//! captures are kept (`PhotoRecords`), in saves.

use std::collections::HashMap;

use bevy::prelude::*;
use s3bake::gamedata::{PhotoCollection, PhotoSubject};

use crate::family::Kin;

/// Something in the frame (`ScreenGrabEntity`): the share of it it covers, its name, and what it
/// is: an object's class and catalogue name (a Sim's class the game's `Actors.Sim`), and its
/// place; a plant's kind; a Sim's doings.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub entity: Option<Entity>,
    pub coverage: f32,
    pub name: String,
    pub class: String,
    pub instance: String,
    pub burning: bool,
    pub indoors: bool,
    pub residential: bool,
    /// The lot's venue (the game's `MetaAutonomyType`: `BigPark`, `Gym`...).
    pub venue: String,
    pub plant: Option<String>,
    pub sim: Option<SimSeen>,
}

/// A Sim in the frame, as the scoring functions ask after them.
#[derive(Clone, Debug, Default)]
pub struct SimSeen {
    pub human: bool,
    /// `Human`, `Dog`, `Cat`, `Horse`...
    pub species: &'static str,
    pub female: bool,
    pub ghost: bool,
    /// The service they're here for (the game's `ServiceType`: `Maid`, `Repairman`...).
    pub service: &'static str,
    /// Their career (the game's `OccupationNames`) and level.
    pub career: Option<(String, u32)>,
    /// What they're doing, as the game names the interaction (`Computer+BrowseWeb`), and to what.
    pub doing: String,
    pub target: Option<Entity>,
    /// Their running interaction's commodities (`Hunger`, `TraitNeat`).
    pub commodities: Vec<&'static str>,
    /// Their posture's (`Sitting`, `Standing`, `SwimmingInPool`, `CarryingChild`) and what it's
    /// on.
    pub posture: Vec<&'static str>,
    pub posture_on: Option<Entity>,
    /// `Run`, `Jog`.
    pub walk: &'static str,
    pub moodlets: Vec<String>,
    /// What they are to the photographer: family, their relationship (the game's `LTR` names),
    /// a friend, a romantic partner, one of their household.
    pub kin: Option<Kin>,
    pub ltr: &'static str,
    pub friend: bool,
    pub romantic: bool,
    pub housemate: bool,
}

/// Two Sims in the frame to each other.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pair {
    pub friends: bool,
    pub romantic: bool,
    pub enemies: bool,
    pub ltr: &'static str,
    /// What the second is to the first.
    pub kin: Option<Kin>,
}

/// What a photo's taken of: what's in the frame, and when and where.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub seen: Vec<Seen>,
    pub hour: f32,
    /// The world (`UserCreated` at home), and whether it's a holiday destination.
    pub world: String,
    pub vacation: bool,
    /// Between the Sims in the frame (indexes into `seen`).
    pub pairs: HashMap<(usize, usize), Pair>,
}

/// A subject's score: the key objects that made it (indexes into the scene's `seen`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scored {
    pub score: f32,
    pub keys: Vec<usize>,
}

impl Scored {
    fn of(score: f32) -> Self {
        Scored { score, keys: Vec::new() }
    }

    fn pass(p: bool) -> Self {
        Self::of(if p { 1.0 } else { 0.0 })
    }

    /// A key object, adding its coverage (`AddKeyObject`).
    fn add(&mut self, i: usize, by: f32) {
        if self.keys.is_empty() {
            self.score = 0.0;
        }
        if !self.keys.contains(&i) {
            self.keys.push(i);
            self.score += by;
        }
    }
}

/// The game's `StringUtil.ContainsAny`: whether any of the words (not empty) is in the text,
/// letters' case aside.
pub fn contains_any(text: &str, words: &[String]) -> bool {
    if text.is_empty() {
        return false;
    }
    let t = text.to_ascii_lowercase();
    words.iter().filter(|w| !w.is_empty()).any(|w| t.contains(&w.to_ascii_lowercase()))
}

/// The subjects and collections, by key.
pub struct Subjects<'a> {
    pub subjects: &'a [PhotoSubject],
    by_key: HashMap<&'a str, usize>,
    pub collections: HashMap<&'a str, &'a PhotoCollection>,
}

impl<'a> Subjects<'a> {
    pub fn new(subjects: &'a [PhotoSubject], collections: &'a [PhotoCollection]) -> Self {
        Subjects { subjects, by_key: subjects.iter().enumerate().map(|(i, s)| (s.key.as_str(), i)).collect(), collections: collections.iter().map(|c| (c.key.as_str(), c)).collect() }
    }

    pub fn get(&self, key: &str) -> Option<&'a PhotoSubject> {
        self.by_key.get(key).map(|i| &self.subjects[*i])
    }

    /// A collection's subjects (those shown, not the parts of others).
    pub fn in_collection(&self, collection: &str) -> impl Iterator<Item = &'a PhotoSubject> + '_ {
        let c = collection.to_string();
        self.subjects.iter().filter(move |s| !s.hidden && s.collection == c)
    }
}

/// Scoring a scene, subject by subject, each scored once (`SubjectScoringHelper`).
pub struct Scorer<'a, 'b> {
    pub subjects: &'b Subjects<'a>,
    pub scene: &'b Scene,
    cache: HashMap<String, Scored>,
}

impl<'a, 'b> Scorer<'a, 'b> {
    pub fn new(subjects: &'b Subjects<'a>, scene: &'b Scene) -> Self {
        Scorer { subjects, scene, cache: HashMap::new() }
    }

    /// A subject's score by its key (none, unknown).
    pub fn score_key(&mut self, key: &str) -> Scored {
        if let Some(s) = self.cache.get(key) {
            return s.clone();
        }
        let Some(def) = self.subjects.get(key) else { return Scored::default() };
        let mut r = self.method(def);
        // (Its collection's say: `Collection.Score`.)
        r.score *= self.subjects.collections.get(def.collection.as_str()).map_or(1.0, |c| c.score);
        self.cache.insert(key.to_string(), r.clone());
        r
    }

    fn sims(&self, human: Option<bool>) -> impl Iterator<Item = (usize, &'b Seen, &'b SimSeen)> + '_ {
        self.scene.seen.iter().enumerate().filter_map(move |(i, s)| s.sim.as_ref().filter(|x| human.is_none_or(|h| x.human == h)).map(|x| (i, s, x)))
    }

    fn objects(&self) -> impl Iterator<Item = (usize, &'b Seen)> + '_ {
        self.scene.seen.iter().enumerate().filter(|(_, s)| s.sim.is_none())
    }

    /// Each Sim (humans; or all) the test holds for a key object.
    fn each_sim(&self, human: Option<bool>, test: impl Fn(&SimSeen) -> bool) -> Scored {
        let mut r = Scored::default();
        for (i, s, x) in self.sims(human) {
            if test(x) {
                r.add(i, s.coverage);
            }
        }
        r
    }

    /// Each pair of Sims the test holds for, both key objects.
    fn each_pair(&self, test: impl Fn(&Pair) -> bool) -> Scored {
        let mut r = Scored::default();
        let mut pairs: Vec<(&(usize, usize), &Pair)> = self.scene.pairs.iter().collect();
        pairs.sort_by_key(|p| *p.0);
        for (&(a, b), p) in pairs {
            if test(p) {
                r.add(a, self.scene.seen[a].coverage);
                r.add(b, self.scene.seen[b].coverage);
            }
        }
        r
    }

    fn each_object(&self, test: impl Fn(&Seen) -> bool) -> Scored {
        let mut r = Scored::default();
        for (i, s) in self.scene.seen.iter().enumerate() {
            if test(s) {
                r.add(i, s.coverage);
            }
        }
        r
    }

    fn method(&mut self, d: &PhotoSubject) -> Scored {
        let a = &d.args;
        let arg = |i: usize| a.get(i).map(String::as_str).unwrap_or("");
        let words = |from: usize| a.iter().skip(from).cloned().collect::<Vec<String>>();
        let scene = self.scene;
        match d.method.as_str() {
            "FixedScore" => Scored::of(arg(0).parse().unwrap_or(0.0)),
            "ExactSimCount" => {
                let n: usize = arg(0).parse().unwrap_or(usize::MAX);
                let mut r = Scored::default();
                let humans: Vec<(usize, f32)> = self.sims(Some(true)).map(|(i, s, _)| (i, s.coverage)).collect();
                if humans.len() == n {
                    for (i, c) in humans {
                        r.add(i, c);
                    }
                }
                r
            }
            "SpeciesOr" => {
                let w = words(0);
                self.each_sim(None, |x| w.iter().any(|s| !s.is_empty() && s.eq_ignore_ascii_case(x.species)))
            }
            "SimFlagsOr" | "SimFlagsOrPetsOnly" => {
                let pets = d.method == "SimFlagsOrPetsOnly";
                let w = words(0);
                self.each_sim(Some(!pets), |x| w.iter().any(|f| (f == "Female" && x.female) || (f == "Male" && !x.female)))
            }
            "SimOfService" => self.each_sim(Some(true), |x| !x.service.is_empty() && x.service.eq_ignore_ascii_case(arg(0))),
            "HasCareer" => {
                let (lo, hi): (u32, u32) = (arg(1).parse().unwrap_or(0), arg(2).parse().unwrap_or(10));
                self.each_sim(Some(true), |x| x.career.as_ref().is_some_and(|(c, l)| c.eq_ignore_ascii_case(arg(0)) && (lo..=hi).contains(l)))
            }
            "Ghost" => self.each_sim(Some(true), |x| x.ghost),
            "InteractionForCommodity" => self.each_sim(Some(true), |x| x.commodities.iter().any(|c| c.eq_ignore_ascii_case(arg(0)))),
            "WalkStyle" => self.each_sim(Some(true), |x| !x.walk.is_empty() && x.walk.eq_ignore_ascii_case(arg(0))),
            "InteractionNamed" | "PetIncludedInteractionNamed" => {
                // (The Sim, and what they're doing it to if that's in the frame too.)
                let human = (d.method == "InteractionNamed").then_some(true);
                let w = words(0);
                let mut r = Scored::default();
                for (i, s, x) in self.sims(human) {
                    if !contains_any(&x.doing, &w) {
                        continue;
                    }
                    r.add(i, s.coverage);
                    if let Some(t) = x.target
                        && let Some(j) = scene.seen.iter().position(|o| o.entity == Some(t))
                    {
                        r.add(j, scene.seen[j].coverage);
                    }
                }
                r
            }
            "PostureSatisfies" => {
                let w: Vec<&str> = arg(0).split(',').map(str::trim).collect();
                let mut r = Scored::default();
                for (i, s, x) in self.sims(Some(true)) {
                    if !x.posture.iter().any(|p| w.iter().any(|q| q.eq_ignore_ascii_case(p))) {
                        continue;
                    }
                    r.add(i, s.coverage);
                    if let Some(t) = x.posture_on
                        && let Some(j) = scene.seen.iter().position(|o| o.entity == Some(t))
                    {
                        r.add(j, scene.seen[j].coverage);
                    }
                }
                r
            }
            "HasMoodlet" => {
                let w = words(0);
                self.each_sim(Some(true), |x| x.moodlets.iter().any(|m| contains_any(m, &w)))
            }
            "GenealogyWithPhotographer" => self.each_sim(Some(true), |x| kin_is(x.kin, arg(0))),
            "Genealogy" => self.each_pair(|p| kin_is(p.kin, arg(0))),
            "LTRWithPhotographer" => self.each_sim(Some(true), |x| !x.ltr.is_empty() && arg(0).replace(' ', "").contains(x.ltr)),
            "LTR" => self.each_pair(|p| !p.ltr.is_empty() && arg(0).replace(' ', "").contains(p.ltr)),
            "AreFriends" => self.each_pair(|p| p.friends),
            "AreRomantic" => self.each_pair(|p| p.romantic),
            "AreFriendsOrRomantic" => self.each_pair(|p| p.friends || p.romantic),
            "AreEnemies" => self.each_pair(|p| p.enemies),
            "AreFriendsWithPhotographer" => self.each_sim(Some(true), |x| x.friend),
            "AreRomanticWithPhotographer" => self.each_sim(Some(true), |x| x.romantic),
            "HousemateOfPhotographer" => self.each_sim(Some(true), |x| x.housemate),
            "AnyOfTheseObjects" => {
                let w = words(0);
                self.each_object(|s| contains_any(&s.class, &w))
            }
            "AllOfTheseObjects" => {
                // (The largest of each in the frame, all of them there.)
                let mut r = Scored::default();
                let mut best: Vec<Option<(usize, f32)>> = vec![None; 4];
                let want: Vec<usize> = (0..4).filter(|i| !arg(*i).is_empty()).collect();
                if want.is_empty() {
                    return r;
                }
                for (i, s) in self.objects() {
                    for k in &want {
                        if contains_any(&s.class, &[arg(*k).to_string()]) && best[*k].is_none_or(|b| s.coverage > b.1) {
                            best[*k] = Some((i, s.coverage));
                        }
                    }
                }
                if want.iter().all(|k| best[*k].is_some()) {
                    for k in &want {
                        let (i, c) = best[*k].unwrap_or_default();
                        r.add(i, c);
                    }
                }
                r
            }
            "NoneOfTheseClassStrings" => {
                let w = words(0);
                self.each_object(|s| !contains_any(&s.class, &w))
            }
            "NoneOfTheseInstanceStrings" => {
                let w = words(0);
                self.each_object(|s| !contains_any(&s.instance, &w))
            }
            "ObjectInstanceName" => {
                let w = words(0);
                self.each_object(|s| contains_any(&s.instance, &w))
            }
            "BurningObjects" => self.each_object(|s| s.sim.is_none() && s.burning),
            "Indoors" => self.each_object(|s| s.indoors),
            "Outdoors" => self.each_object(|s| !s.indoors),
            "Residential" => self.each_object(|s| s.residential),
            "Venue" => {
                let w = words(0);
                self.each_object(|s| !s.venue.is_empty() && w.iter().any(|v| v.eq_ignore_ascii_case(&s.venue)))
            }
            "Plant" => self.each_object(|s| s.plant.as_ref().is_some_and(|p| arg(0) == "Any" || contains_any(p, &[arg(0).to_string()]))),
            "OnlyInWorldName" => Scored::pass(scene.world.eq_ignore_ascii_case(arg(0))),
            "OnlyInWorldType" => Scored::pass(if arg(0) == "Vacation" { scene.vacation } else { !scene.vacation }),
            "TimeOfDay" => {
                let (from, to): (f32, f32) = (arg(0).parse().unwrap_or(0.0), arg(1).parse().unwrap_or(0.0));
                let h = scene.hour.floor();
                Scored::pass(if from < to { h > from && h < to } else { h > from || h < to })
            }
            "KeyObjectCount" => {
                let (lo, hi) = (arg(1).parse::<usize>(), arg(2).parse::<usize>());
                let (Ok(lo), Ok(hi)) = (lo, hi) else { return Scored::default() };
                let r = self.score_key(arg(0));
                if (lo..=hi).contains(&r.keys.len()) { r } else { Scored::default() }
            }
            "Op_AtLeastOne" => {
                let mut total = 0.0;
                let mut keys: Vec<usize> = Vec::new();
                for i in 0..4 {
                    if arg(i).is_empty() {
                        continue;
                    }
                    let r = self.score_key(arg(i));
                    if r.score > 0.0 {
                        total += r.score;
                        for k in r.keys {
                            if !keys.contains(&k) {
                                keys.push(k);
                            }
                        }
                    }
                }
                if total > 0.0 { Scored { score: total, keys } } else { Scored::default() }
            }
            "Op_All" | "Op_And" => {
                // (Every one of them scoring; `Op_And`'s key objects those common to them all.)
                let and = d.method == "Op_And";
                let mut score = 1.0;
                let mut had = false;
                let mut keys: Vec<usize> = Vec::new();
                for i in 0..4 {
                    if score == 0.0 || arg(i).is_empty() {
                        continue;
                    }
                    let r = self.score_key(arg(i));
                    score += r.score;
                    if r.score == 0.0 {
                        score = 0.0;
                        keys.clear();
                    } else if keys.is_empty() {
                        keys.extend(r.keys.iter().copied());
                    } else if and {
                        let mut common = Vec::new();
                        let mut any = false;
                        for k in &r.keys {
                            any = true;
                            if keys.contains(k) {
                                common.push(*k);
                            } else if had {
                                return Scored::default();
                            }
                        }
                        if any {
                            keys = common;
                        }
                    } else {
                        for k in &r.keys {
                            if !keys.contains(k) {
                                keys.push(*k);
                            }
                        }
                    }
                    had = had || !keys.is_empty();
                }
                if had && keys.is_empty() { Scored::default() } else { Scored { score, keys } }
            }
            // (What this town hasn't got: occult Sims, celebrities, the packs' doings.)
            _ => Scored::default(),
        }
    }
}

/// Whether a relation's what the game's `Genealogy.Is<arg>` asks.
fn kin_is(kin: Option<Kin>, arg: &str) -> bool {
    match (arg, kin) {
        (_, None) => false,
        ("BloodRelated", Some(_)) => true,
        ("Parent" | "ParentOrStepParent", Some(Kin::Parent)) => true,
        ("Child", Some(Kin::Child)) | ("Sibling", Some(Kin::Sibling)) | ("Grandchild", Some(Kin::Grandchild)) | ("Grandparent", Some(Kin::Grandparent)) => true,
        _ => false,
    }
}

/// How much more a subject's score counts never caught, or not today (`kScoreMultiplierNewSubject`,
/// `kScoreMultiplierNewToday`).
const NEW_SUBJECT: f32 = 20.0;
const NEW_TODAY: f32 = 10.0;
/// Of a subject's worth, the first shot of a day's (`kFirstShotOfTheDay...Multiplier`).
pub const FIRST_TODAY: f32 = 0.5;

/// A photographer's captures (`mSubjectRecords`): each subject's count and the day it was last
/// caught.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PhotoRecords(pub HashMap<String, (u32, u32)>);

impl PhotoRecords {
    pub fn would_be_new(&self, key: &str) -> bool {
        !self.0.contains_key(key)
    }

    pub fn new_today(&self, key: &str, day: u32) -> bool {
        self.0.get(key).is_none_or(|r| r.1 != day)
    }

    /// Whether every subject of a collection's been caught.
    pub fn completed(&self, subjects: &Subjects, collection: &str) -> bool {
        let mut any = false;
        for s in subjects.in_collection(collection) {
            any = true;
            if self.would_be_new(&s.key) {
                return false;
            }
        }
        any
    }

    /// As kept in saves.
    pub fn to_saved(&self) -> Vec<(String, u32, u32)> {
        let mut v: Vec<(String, u32, u32)> = self.0.iter().map(|(k, (n, d))| (k.clone(), *n, *d)).collect();
        v.sort();
        v
    }

    pub fn from_saved(v: &[(String, u32, u32)]) -> Self {
        PhotoRecords(v.iter().map(|(k, n, d)| (k.clone(), (*n, *d))).collect())
    }
}

/// The best subject in a scene (`GetBestSubjectMatchForScreenGrab`): of those shown (and Nothing),
/// the highest scoring, scaled, and the more for being new to the photographer.
#[derive(Clone, Debug, PartialEq)]
pub struct Best {
    pub key: String,
    pub score: f32,
    pub keys: Vec<usize>,
}

pub fn best(subjects: &Subjects, scene: &Scene, records: Option<&PhotoRecords>, day: u32) -> Option<Best> {
    let mut scorer = Scorer::new(subjects, scene);
    let mut top: Option<Best> = None;
    for s in subjects.subjects.iter().filter(|s| !s.hidden) {
        let r = scorer.score_key(&s.key);
        let mut score = r.score * s.scale;
        if score > 0.0 && s.key != "Nothing" {
            match records {
                Some(rec) if rec.would_be_new(&s.key) => score *= NEW_SUBJECT,
                Some(rec) if rec.new_today(&s.key, day) => score *= NEW_TODAY,
                None => score *= NEW_SUBJECT,
                _ => {}
            }
        }
        if score > 0.0 && top.as_ref().is_none_or(|t| score > t.score) {
            top = Some(Best { key: s.key.clone(), score, keys: r.keys });
        }
    }
    top
}

/// A subject's title with its key objects' names in it (`{0.SimName}`, `{1.String}`...), or its
/// name if it hasn't one.
pub fn title(s: &PhotoSubject, names: &[String]) -> String {
    if s.title.is_empty() {
        return s.name.clone();
    }
    let mut out = String::new();
    let mut rest = s.title.as_str();
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find('}') else { break };
        let token = &rest[at + 1..at + end];
        let i: usize = token.split('.').next().and_then(|n| n.parse().ok()).unwrap_or(usize::MAX);
        out.push_str(names.get(i).map(String::as_str).unwrap_or(""));
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// What the photographer's told of a capture (`PhotoTaken...`): a new subject (how many of its
/// collection are left of how many, and one still to catch), the first today, or one again.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    New { remaining: usize, total: usize, suggest: String },
    Today,
    Again,
}

/// A subject captured (`OnPhotoTakenPreInstantiate`): the photo's title, the subject's name and
/// collection's, the skill and simoleons it adds, its collection's multiplier on the worth, and
/// what's said of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub title: String,
    pub name: String,
    pub collection: String,
    pub points: f32,
    pub value: f32,
    pub multiplier: f32,
    pub note: Option<Note>,
}

/// A subject captured by a photographer on a day: recorded, and what it's worth (`pick`, 0 to 1:
/// which of the collection's subjects still to catch is suggested).
pub fn capture(subjects: &Subjects, records: &mut PhotoRecords, key: &str, names: &[String], day: u32, pick: f32) -> Capture {
    let Some(def) = subjects.get(key).filter(|s| s.key != "Nothing") else {
        let title = subjects.get("Nothing").map(|n| title(n, &[])).filter(|t| !t.is_empty() && t != "Nothing").unwrap_or_else(|| "Untitled".into());
        return Capture { title, name: String::new(), collection: String::new(), points: 0.0, value: 0.0, multiplier: 1.0, note: None };
    };
    let (new, today) = (records.would_be_new(key), records.new_today(key, day));
    let r = records.0.entry(key.to_string()).or_insert((0, day));
    *r = (r.0 + 1, day);
    let share = if new { 1.0 } else if today { FIRST_TODAY } else { 0.0 };
    let coll = subjects.collections.get(def.collection.as_str());
    let multiplier = coll.map_or(1.0, |c| if records.completed(subjects, &def.collection) { c.value_complete } else { c.value });
    let note = if new {
        let left: Vec<&PhotoSubject> = subjects.in_collection(&def.collection).filter(|s| records.would_be_new(&s.key)).collect();
        let total = subjects.in_collection(&def.collection).count();
        let suggest = left.get(((pick * left.len() as f32) as usize).min(left.len().saturating_sub(1))).map(|s| s.name.clone()).unwrap_or_default();
        Note::New { remaining: left.len(), total, suggest }
    } else if today {
        Note::Today
    } else {
        Note::Again
    };
    Capture {
        title: title(def, names),
        name: def.name.clone(),
        collection: coll.map(|c| c.name.clone()).unwrap_or_default(),
        points: def.skill_points as f32 * share,
        value: def.value as f32 * share,
        multiplier,
        note: Some(note),
    }
}

/// The game's words for a capture (`Strings:PhotoTaken...`), the photo's worth in them.
pub fn note_words(c: &Capture, worth: i64) -> Option<String> {
    let (name, coll) = (&c.name, &c.collection);
    Some(match c.note.as_ref()? {
        Note::New { remaining: 0, total, .. } => format!("Awesome! By capturing {name} (worth §{worth}) you finally captured all {total} subjects in the {coll} collection! Good for you!"),
        Note::New { remaining: 1, suggest, .. } => format!("You captured {name}, the second-to-last subject in the {coll} collection, worth §{worth}! The only subject left is {suggest}.  Go get 'em!"),
        Note::New { remaining, suggest, .. } => format!("You captured \"{name},\" that's a new subject worth §{worth}! There are only {remaining} subjects left to complete the \"{coll}\" collection. Have you tried to capture \"{suggest}\" yet?"),
        Note::Today => format!("New Today: {name}, worth §{worth}!"),
        Note::Again => format!("Why don't you try capturing something a little different? ({name}, worth §{worth}.)"),
    })
}

/// The line under the frame naming what's aimed at ("A Couch (Still Life)"; nothing for
/// Nothing): the collection named if it's one shown at the photographer's level.
pub fn aim_line(subjects: &Subjects, key: &str, level: u32) -> String {
    let Some(s) = subjects.get(key).filter(|s| s.key != "Nothing") else { return String::new() };
    match subjects.collections.get(s.collection.as_str()) {
        Some(c) if !c.hide_in_ui && c.min_level >= 0 && c.min_level as u32 <= level => format!("{} ({})", s.name, c.name),
        _ => s.name.clone(),
    }
}

/// The share of a frame (a screen rectangle, logical pixels) a box covers (its corners in the
/// world): its outline on the screen, clipped to the frame; nothing if any of it's behind the
/// camera.
pub fn coverage(corners: &[Vec3], camera: &Camera, at: &GlobalTransform, frame: Rect) -> f32 {
    let mut outline = Rect { min: Vec2::splat(f32::MAX), max: Vec2::splat(f32::MIN) };
    for c in corners {
        let Ok(p) = camera.world_to_viewport(at, *c) else { return 0.0 };
        outline = outline.union_point(p);
    }
    let seen = outline.intersect(frame);
    if seen.is_empty() || frame.width() <= 0.0 || frame.height() <= 0.0 {
        return 0.0;
    }
    (seen.width() * seen.height() / (frame.width() * frame.height())).min(1.0)
}

/// Whether a wall stands between two points on a floor (lot tiles), the line of sight crossing it.
fn wall_between(walls: &[s3bake::types::WallBaked], level: u8, a: Vec2, b: Vec2) -> bool {
    let cross = |p: Vec2, q: Vec2, r: Vec2, s: Vec2| {
        let (d1, d2) = (q - p, s - r);
        let den = d1.perp_dot(d2);
        if den.abs() < 1e-6 {
            return false;
        }
        let t = (r - p).perp_dot(d2) / den;
        let u = (r - p).perp_dot(d1) / den;
        (0.02..0.98).contains(&t) && (0.0..=1.0).contains(&u)
    };
    walls.iter().any(|w| w.level.max(1) == level && cross(a, b, Vec2::from(w.a), Vec2::from(w.b)))
}

/// A community lot's venue, as the game's lot types have it (`MetaAutonomyType`), by its name.
fn venue(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    [("park", "BigPark"), ("gym", "Gym"), ("fitness", "Gym"), ("beach", "BeachVenue"), ("pool", "Pool"), ("library", "Library"), ("cemetery", "Graveyard"), ("graveyard", "Graveyard"), ("fishing", "FishingSpot")]
        .iter()
        .find(|(k, _)| n.contains(k))
        .map_or("", |(_, v)| v)
}

/// What a Sim's doing, as the game names its interaction: the object's class and the
/// interaction (`Computer+BrowseWeb`), or the social; and its commodities, as near as they go.
fn doing(a: &crate::interact::Action, class: Option<&str>) -> (String, Vec<&'static str>) {
    let bare: String = a.label.chars().filter(|c| c.is_alphanumeric() || *c == '!').collect();
    let name = match class {
        Some(c) => format!("{c}+{bare} {}", a.label),
        None => format!("{bare} {}", a.label),
    };
    let l = a.label.to_ascii_lowercase();
    let mut commodities = Vec::new();
    if matches!(a.kind, crate::interact::ActionKind::EatItem { .. }) || l.starts_with("eat") || l.starts_with("have ") || l.contains("snack") || l.contains("serve") {
        commodities.push("Hunger");
    }
    if ["clean", "scrub", "wash", "mop", "tidy", "dishes", "make bed", "unclog"].iter().any(|k| l.contains(k)) {
        commodities.push("TraitNeat");
    }
    (name, commodities)
}

/// The game's name for the service a Sim's here for.
#[derive(bevy::ecs::query::QueryData)]
pub struct Service {
    maid: Has<crate::services::Maid>,
    repair: Has<crate::services::Repairman>,
    mail: Has<crate::services::MailCarrier>,
    pizza: Has<crate::services::PizzaDelivery>,
    sitter: Has<crate::services::Babysitter>,
    social: Has<crate::services::SocialWorker>,
    burglar: Has<crate::burglar::Burglar>,
    police: Has<crate::burglar::Officer>,
    fire: Has<crate::fire::Firefighter>,
    repo: Has<crate::repoman::Repoman>,
    reaper: Has<crate::death::GrimReaper>,
}

impl ServiceItem<'_, '_> {
    fn name(&self) -> &'static str {
        [
            (self.maid, "Maid"),
            (self.repair, "Repairman"),
            (self.mail, "MailCarrier"),
            (self.pizza, "PizzaDelivery"),
            (self.sitter, "Babysitter"),
            (self.social, "SocialWorkerChildProtection"),
            (self.burglar, "Burglar"),
            (self.police, "Police"),
            (self.fire, "Firefighter"),
            (self.repo, "Repoman"),
            (self.reaper, "GrimReaper"),
        ]
        .into_iter()
        .find(|(h, _)| *h)
        .map_or("", |(_, n)| n)
    }
}

/// A Sim as the scene asks after them.
#[derive(bevy::ecs::query::QueryData)]
pub struct SimView {
    entity: Entity,
    sim: &'static crate::sim::Sim,
    at: &'static GlobalTransform,
    vis: &'static InheritedVisibility,
    queue: Option<&'static crate::interact::ActionQueue>,
    anim: Option<&'static crate::sim::SimAnim>,
    moodlets: Option<&'static crate::life::Moodlets>,
    job: Option<&'static crate::careers::Job>,
    rels: Option<&'static crate::social::Relationships>,
    ghost: Has<crate::ghosts::Ghost>,
    swimming: Has<crate::swim::Swimming>,
    holding: Has<crate::carry::Holding>,
    jogging: Has<crate::jog::Jogging>,
    member: Has<crate::sim::HouseholdMember>,
    service: Service,
}

/// What's needed to see what's in a frame.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SceneParams<'w, 's> {
    objects: Query<'w, 's, (Entity, &'static crate::interact::GameObject, &'static GlobalTransform, &'static InheritedVisibility, Option<&'static crate::gardening::GrowingPlant>, Option<&'static crate::visit::LotObject>)>,
    sims: Query<'w, 's, SimView>,
    pets: Query<'w, 's, (Entity, &'static crate::pets::Pet, &'static GlobalTransform, &'static InheritedVisibility)>,
    fires: Query<'w, 's, &'static GlobalTransform, With<crate::fire::Fire>>,
    baked: Option<Res<'w, crate::baked::Baked>>,
    ui: Option<Res<'w, crate::icons::GameUi>>,
    building: Option<Res<'w, crate::building::ActiveBuilding>>,
    world: Res<'w, crate::loading::CurrentWorld>,
    clock: Res<'w, crate::clock::GameClock>,
    genealogy: Res<'w, crate::family::Genealogy>,
    classes: Local<'s, HashMap<s3bake::Key, (String, String)>>,
}

/// An upright box's corners: standing at a point, so wide each way and so high.
fn upright(p: Vec3, half: f32, height: f32) -> Vec<Vec3> {
    let mut v = Vec::with_capacity(8);
    for x in [-half, half] {
        for z in [-half, half] {
            for y in [0.0, height] {
                v.push(p + Vec3::new(x, y, z));
            }
        }
    }
    v
}

/// How far off things are seen in a photo.
const FAR: f32 = 60.0;

impl SceneParams<'_, '_> {
    /// The game day (for what's new today).
    pub fn day(&self) -> u32 {
        (self.clock.minutes / 1440.0) as u32
    }

    /// What's in a frame (logical pixels) seen through the camera, for a photographer.
    pub fn gather(&mut self, camera: &Camera, at: &GlobalTransform, frame: Rect, photographer: Entity) -> Scene {
        let eye = at.translation();
        if self.classes.is_empty()
            && let Some(b) = &self.baked
        {
            *self.classes = b.0.catalog.iter().map(|c| (c.objd, (c.script.clone(), c.instance_name.clone()))).collect();
        }
        let mut scene = Scene { hour: (self.clock.minutes / 60.0 % 24.0) as f32, world: "UserCreated".into(), ..Default::default() };
        let b = self.building.as_deref();
        // (Not behind a wall of the house, and on the floor seen from.)
        let in_sight = |p: Vec3| match b {
            Some(b) => {
                let (from, to) = (b.level_at(eye.y), b.level_at(p.y));
                from != to || !wall_between(&b.data.walls, from, b.local(eye), b.local(p))
            }
            None => true,
        };
        let indoors = |p: Vec3| b.is_some_and(|b| b.room_at(b.level_at(p.y + 0.3), p).is_some_and(|k| !matches!(k, s3bake::ROOM_PORCH | s3bake::ROOM_OUTSIDE)));
        let fires: Vec<Vec3> = self.fires.iter().map(|f| f.translation()).collect();
        for (e, o, tf, vis, plant, lot) in &self.objects {
            let p = tf.translation();
            if !vis.get() || p.distance(eye) > FAR || !in_sight(p + Vec3::Y * o.height * 0.5) {
                continue;
            }
            let r = tf.rotation();
            let corners: Vec<Vec3> = [(-1.0, 0.0, -1.0), (1.0, 0.0, -1.0), (-1.0, 0.0, 1.0), (1.0, 0.0, 1.0), (-1.0, 1.0, -1.0), (1.0, 1.0, -1.0), (-1.0, 1.0, 1.0), (1.0, 1.0, 1.0)]
                .iter()
                .map(|(x, y, z)| p + r * Vec3::new(o.center.x + x * o.half.x, y * o.height.max(0.05), o.center.y + z * o.half.y))
                .collect();
            let c = coverage(&corners, camera, at, frame);
            if c <= 0.0 {
                continue;
            }
            let (class, instance) = self.classes.get(&o.objd).cloned().unwrap_or_default();
            let plant = plant.and_then(|g| self.ui.as_ref().and_then(|u| u.data.plants.get(g.plant)).map(|i| i.name.clone()));
            scene.seen.push(Seen {
                entity: Some(e),
                coverage: c,
                name: o.name.clone(),
                class,
                instance,
                burning: fires.iter().any(|f| f.distance(p) < 1.0),
                indoors: indoors(p),
                residential: lot.is_none(),
                venue: lot.and_then(|l| self.world.data.lots.get(l.0)).map_or("", |l| venue(&l.internal_name)).to_string(),
                plant,
                sim: None,
            });
        }
        // (The town's rabbit holes, big as their buildings.)
        for (lot, building) in &self.world.data.buildings {
            for o in building.objects.iter().filter(|o| o.script.contains("RabbitHoles.")) {
                let p = Vec3::from(o.position);
                if p.distance(eye) > FAR * 3.0 {
                    continue;
                }
                let corners = upright(p, 10.0, 12.0);
                let c = coverage(&corners, camera, at, frame);
                if c <= 0.0 {
                    continue;
                }
                let instance = self.classes.get(&o.objd).map(|c| c.1.clone()).unwrap_or_default();
                let name = self.world.data.lot_names.get(*lot).cloned().unwrap_or_default();
                scene.seen.push(Seen { entity: None, coverage: c, name, class: o.script.clone(), instance, ..Default::default() });
            }
        }
        // The Sims (bar the photographer), and the pets.
        let me = self.sims.get(photographer).ok().map(|s| (s.sim.id, s.rels.cloned()));
        let household: Vec<Entity> = self.sims.iter().filter(|s| s.member).map(|s| s.entity).collect();
        let mut sim_at: Vec<(usize, Entity)> = Vec::new();
        for s in &self.sims {
            if s.entity == photographer || !s.vis.get() {
                continue;
            }
            let p = s.at.translation();
            let height = if s.sim.age.is_little() { 0.8 } else if s.sim.age == crate::sim::Age::Child { 1.25 } else { 1.8 };
            if p.distance(eye) > FAR || !in_sight(p + Vec3::Y * height * 0.5) {
                continue;
            }
            let corners = upright(p, 0.25, height);
            let c = coverage(&corners, camera, at, frame);
            if c <= 0.0 {
                continue;
            }
            let action = s.queue.and_then(|q| q.current()).filter(|a| !a.cancel);
            let target = action.and_then(|a| match a.kind {
                crate::interact::ActionKind::Object { target, .. } => Some(target),
                crate::interact::ActionKind::Social { target, .. } => Some(target),
                _ => None,
            });
            let class = target.and_then(|t| self.objects.get(t).ok()).and_then(|o| self.classes.get(&o.1.objd)).map(|c| c.0.clone());
            let (doing, commodities) = action.map(|a| doing(a, class.as_deref())).unwrap_or_default();
            let mut posture = Vec::new();
            match s.anim.map(|a| a.pose) {
                Some(crate::sim::Pose::Sit) => posture.push("Sitting"),
                Some(crate::sim::Pose::Lie) | None => {}
                _ => posture.push("Standing"),
            }
            if s.swimming {
                posture.push("SwimmingInPool");
            }
            if s.holding {
                posture.push("CarryingChild");
            }
            let rel = me.as_ref().and_then(|m| m.1.as_ref()).and_then(|r| r.0.get(&s.entity)).copied();
            let kin = me.as_ref().and_then(|m| self.genealogy.kin(m.0, s.sim.id));
            sim_at.push((scene.seen.len(), s.entity));
            scene.seen.push(Seen {
                entity: Some(s.entity),
                coverage: c,
                name: s.sim.full_name(),
                class: "Sims3.Gameplay.Actors.Sim".into(),
                instance: String::new(),
                indoors: indoors(p),
                residential: true,
                sim: Some(SimSeen {
                    human: true,
                    species: "Human",
                    female: s.sim.female,
                    ghost: s.ghost,
                    service: s.service.name(),
                    career: s.job.map(|j| (j.career().key.to_string(), j.level as u32)),
                    doing,
                    target,
                    commodities,
                    posture,
                    posture_on: None,
                    walk: if s.jogging { "Jog" } else { "" },
                    moodlets: s.moodlets.map(|m| m.0.iter().flat_map(|x| [format!("{:?}", x.kind), x.kind.def().name.to_string()]).collect()).unwrap_or_default(),
                    kin,
                    ltr: rel.map_or("Stranger", ltr),
                    friend: rel.is_some_and(|r| r.friendship >= crate::social::FRIEND_THRESHOLD),
                    romantic: rel.is_some_and(|r| r.romantic || r.status != crate::social::RelStatus::None && r.status != crate::social::RelStatus::Ex),
                    housemate: household.contains(&s.entity),
                }),
                ..Default::default()
            });
        }
        // (Between the Sims in the frame.)
        for (i, a) in &sim_at {
            for (j, b) in &sim_at {
                if i >= j {
                    continue;
                }
                let Ok(sa) = self.sims.get(*a) else { continue };
                let Ok(sb) = self.sims.get(*b) else { continue };
                let rel = sa.rels.and_then(|r| r.0.get(b)).copied();
                let pair = Pair {
                    friends: rel.is_some_and(|r| r.friendship >= crate::social::FRIEND_THRESHOLD),
                    romantic: rel.is_some_and(|r| r.romantic || matches!(r.status, crate::social::RelStatus::Partner | crate::social::RelStatus::Engaged | crate::social::RelStatus::Married)),
                    enemies: rel.is_some_and(|r| r.enemy),
                    ltr: rel.map_or("Stranger", ltr),
                    kin: self.genealogy.kin(sa.sim.id, sb.sim.id),
                };
                scene.pairs.insert((*i, *j), pair);
            }
        }
        for (e, pet, tf, vis) in &self.pets {
            let p = tf.translation();
            if !vis.get() || p.distance(eye) > FAR || !in_sight(p + Vec3::Y * 0.3) {
                continue;
            }
            let h = if pet.kind.eq_ignore_ascii_case("horse") { 1.8 } else { 0.6 };
            let corners = upright(p, 0.4, h);
            let c = coverage(&corners, camera, at, frame);
            if c <= 0.0 {
                continue;
            }
            let species = ["Dog", "Cat", "Horse"].into_iter().find(|s| pet.kind.to_ascii_lowercase().contains(&s.to_ascii_lowercase())).unwrap_or("Dog");
            scene.seen.push(Seen {
                entity: Some(e),
                coverage: c,
                name: pet.name.clone(),
                class: "Sims3.Gameplay.Actors.Sim".into(),
                indoors: indoors(p),
                residential: true,
                sim: Some(SimSeen { human: false, species, female: pet.female, ltr: "Stranger", ..Default::default() }),
                ..Default::default()
            });
        }
        scene
    }
}

/// What a camera's aimed at: the best subject, its key objects' names, and the line naming it
/// under the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Aim {
    pub key: String,
    pub names: Vec<String>,
    pub line: String,
}

impl SceneParams<'_, '_> {
    /// The subject a photographer (at a level, with their captures) has in a frame.
    pub fn aim(&mut self, camera: &Camera, at: &GlobalTransform, frame: Rect, photographer: Entity, records: Option<&PhotoRecords>, level: u32) -> Option<Aim> {
        let scene = self.gather(camera, at, frame, photographer);
        let day = self.day();
        let ui = self.ui.as_ref()?;
        let subjects = Subjects::new(&ui.data.photo_subjects, &ui.data.photo_collections);
        let b = best(&subjects, &scene, records, day)?;
        if std::env::var_os("PHOTO_DEBUG").is_some() {
            let seen: Vec<String> = scene.seen.iter().map(|s| format!("{} {:.3} {}", s.name, s.coverage, s.class.rsplit('.').next().unwrap_or(""))).collect();
            debug!("photo subjects: {} ({:.2}) of {seen:?}", b.key, b.score);
        }
        let names = b.keys.iter().filter_map(|i| scene.seen.get(*i)).map(|s| s.name.clone()).collect();
        Some(Aim { line: aim_line(&subjects, &b.key, level), key: b.key, names })
    }
}

/// A relationship as the game's `LTR` names it.
fn ltr(r: crate::social::Relationship) -> &'static str {
    use crate::social::RelStatus;
    match r.status {
        RelStatus::Married => return "Spouse",
        RelStatus::Engaged => return "Fiancee",
        RelStatus::Partner => return "Partner",
        RelStatus::Ex => return "Ex",
        RelStatus::None => {}
    }
    match r.friendship_label() {
        "Enemy" => "Enemy",
        "Disliked" => "Disliked",
        "Acquaintance" => "Acquaintance",
        "Friend" => "Friend",
        "Good Friend" => "GoodFriend",
        _ => "BFF",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(key: &str, collection: &str, method: &str, args: &[&str]) -> PhotoSubject {
        PhotoSubject {
            key: key.into(),
            collection: collection.into(),
            hidden: collection.is_empty(),
            skill_points: 100,
            value: 10,
            scale: 1.0,
            name: key.into(),
            title: String::new(),
            method: method.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            sku: String::new(),
        }
    }

    fn table() -> (Vec<PhotoSubject>, Vec<PhotoCollection>) {
        let subjects = vec![
            subject("Nothing", "Misc", "FixedScore", &["0.05"]),
            subject("StillLifeCouch", "StillLife", "AnyOfTheseObjects", &["Sofa", "Loveseat"]),
            subject("Sims", "", "AnyOfTheseObjects", &["Actors.Sim"]),
            subject("TwosCompany", "EverydayMoments", "ExactSimCount", &["2"]),
            subject("AreRomantic", "", "AreRomantic", &[]),
            subject("Nighttime", "", "TimeOfDay", &["19", "7"]),
            subject("LoversAtNight", "EverydayMoments", "Op_All", &["AreRomantic", "Nighttime"]),
            subject("StillLifeFinder", "", "AnyOfTheseObjects", &["Gameplay.Objects"]),
            subject("StillLifeGeneric", "StillLife", "KeyObjectCount", &["StillLifeFinder", "3", "6"]),
            subject("Maid", "EverydayMoments", "SimOfService", &["Maid"]),
            subject("BeingNeat", "", "InteractionForCommodity", &["TraitNeat"]),
            subject("MaidAtWork", "EverydayMoments", "Op_And", &["Maid", "BeingNeat"]),
            subject("FamilyMember", "Family", "GenealogyWithPhotographer", &["BloodRelated"]),
            subject("TomatoPlant", "PlantsAndFlowers", "Plant", &["Tomato"]),
        ];
        let collections = ["Misc", "StillLife", "EverydayMoments", "Family", "PlantsAndFlowers"]
            .iter()
            .map(|k| PhotoCollection { key: k.to_string(), name: format!("{k} name"), min_level: if *k == "Misc" { -1 } else { 1 }, hide_in_ui: *k == "Misc", score: 1.0, value: 1.0, value_complete: 1.25, sku: String::new() })
            .collect();
        (subjects, collections)
    }

    fn object(class: &str, coverage: f32) -> Seen {
        Seen { class: format!("Sims3.Gameplay.Objects.{class}"), instance: class.into(), coverage, name: class.into(), ..Default::default() }
    }

    fn sim(coverage: f32, f: impl FnOnce(&mut SimSeen)) -> Seen {
        let mut x = SimSeen { human: true, species: "Human", ..Default::default() };
        f(&mut x);
        Seen { class: "Sims3.Gameplay.Actors.Sim".into(), coverage, name: "Someone".into(), sim: Some(x), ..Default::default() }
    }

    #[test]
    fn the_best_subject_is_the_new_one_that_scores_most() {
        let (subjects, collections) = table();
        let s = Subjects::new(&subjects, &collections);
        // (A sofa filling a fifth of the frame: a couch, new.)
        let scene = Scene { seen: vec![object("Seating.Sofa", 0.2)], hour: 12.0, ..Default::default() };
        let b = best(&s, &scene, Some(&PhotoRecords::default()), 3).unwrap();
        assert_eq!(b.key, "StillLifeCouch");
        assert!((b.score - 0.2 * NEW_SUBJECT).abs() < 1e-4);
        // (Caught today already: still beats Nothing.)
        let mut rec = PhotoRecords::default();
        rec.0.insert("StillLifeCouch".into(), (1, 3));
        assert_eq!(best(&s, &scene, Some(&rec), 3).unwrap().key, "StillLifeCouch");
        assert!(!rec.new_today("StillLifeCouch", 3) && rec.new_today("StillLifeCouch", 4));
        // (Nothing in the frame: Nothing.)
        assert_eq!(best(&s, &Scene::default(), None, 3).unwrap().key, "Nothing");
        // (Three things: a still life, at least its new score beats the couch's when caught.)
        let scene = Scene { seen: vec![object("Seating.Sofa", 0.05), object("Electronics.TV", 0.1), object("Lighting.Lamp", 0.1)], ..Default::default() };
        let b = best(&s, &scene, Some(&rec), 3).unwrap();
        assert_eq!(b.key, "StillLifeGeneric");
        assert_eq!(b.keys.len(), 3);
    }

    #[test]
    fn subjects_made_of_others() {
        let (subjects, collections) = table();
        let s = Subjects::new(&subjects, &collections);
        let lovers = |hour: f32| {
            let mut scene = Scene { seen: vec![sim(0.2, |_| {}), sim(0.3, |_| {})], hour, ..Default::default() };
            scene.pairs.insert((0, 1), Pair { romantic: true, ..Default::default() });
            scene
        };
        let night = lovers(22.0);
        let mut sc = Scorer::new(&s, &night);
        assert_eq!(sc.score_key("TwosCompany").keys, vec![0, 1]);
        let r = sc.score_key("LoversAtNight");
        assert!(r.score > 1.0 && r.keys == vec![0, 1], "{r:?}");
        let noon = lovers(12.0);
        assert_eq!(Scorer::new(&s, &noon).score_key("LoversAtNight").score, 0.0);
        // (The maid, only while she's cleaning: `Op_And`'s common key object.)
        let scene = Scene { seen: vec![sim(0.1, |x| x.service = "Maid"), sim(0.1, |x| x.commodities = vec!["TraitNeat"])], ..Default::default() };
        let r = Scorer::new(&s, &scene).score_key("MaidAtWork");
        assert_eq!(r.score, 0.0, "{r:?}");
        let scene = Scene { seen: vec![sim(0.1, |x| { x.service = "Maid"; x.commodities = vec!["TraitNeat"] })], ..Default::default() };
        assert!(Scorer::new(&s, &scene).score_key("MaidAtWork").score > 0.0);
        // (Family; a tomato plant.)
        let scene = Scene { seen: vec![sim(0.1, |x| x.kin = Some(Kin::Sibling)), Seen { plant: Some("Tomato Plant".into()), coverage: 0.2, ..Default::default() }], ..Default::default() };
        let mut sc = Scorer::new(&s, &scene);
        assert_eq!(sc.score_key("FamilyMember").keys, vec![0]);
        assert_eq!(sc.score_key("TomatoPlant").keys, vec![1]);
    }

    #[test]
    fn a_capture_is_worth_most_new_and_half_the_first_time_each_day() {
        let (mut subjects, collections) = table();
        subjects[1].title = String::new();
        let s = Subjects::new(&subjects, &collections);
        let mut rec = PhotoRecords::default();
        let c = capture(&s, &mut rec, "StillLifeCouch", &[], 3, 0.0);
        assert_eq!((c.points, c.value, c.multiplier), (100.0, 10.0, 1.0));
        assert_eq!(c.title, "StillLifeCouch");
        // (One of the still lifes left: the generic.)
        assert_eq!(c.note, Some(Note::New { remaining: 1, total: 2, suggest: "StillLifeGeneric".into() }));
        assert!(note_words(&c, 12).unwrap().contains("second-to-last subject in the StillLife name collection, worth §12"));
        let again = capture(&s, &mut rec, "StillLifeCouch", &[], 3, 0.0);
        assert_eq!((again.points, again.value, again.note.clone()), (0.0, 0.0, Some(Note::Again)));
        let tomorrow = capture(&s, &mut rec, "StillLifeCouch", &[], 4, 0.0);
        assert_eq!((tomorrow.points, tomorrow.value, tomorrow.note), (50.0, 5.0, Some(Note::Today)));
        assert_eq!(rec.0["StillLifeCouch"], (3, 4));
        // (The collection complete: worth more, and said so.)
        let done = capture(&s, &mut rec, "StillLifeGeneric", &[], 4, 0.0);
        assert_eq!(done.multiplier, 1.25);
        assert!(note_words(&done, 30).unwrap().starts_with("Awesome!"));
        // (Nothing: untitled, worth nothing more.)
        let nothing = capture(&s, &mut rec, "Nothing", &[], 4, 0.0);
        assert_eq!((nothing.title.as_str(), nothing.points, nothing.note), ("Untitled", 0.0, None));
    }

    #[test]
    fn titles_and_the_line_under_the_frame() {
        let (mut subjects, collections) = table();
        subjects[12].title = "My relative {0.SimName}".into();
        let s = Subjects::new(&subjects, &collections);
        assert_eq!(title(&subjects[12], &["Jack Bunch".into()]), "My relative Jack Bunch");
        assert_eq!(title(&subjects[1], &[]), "StillLifeCouch");
        assert_eq!(aim_line(&s, "StillLifeCouch", 1), "StillLifeCouch (StillLife name)");
        assert_eq!(aim_line(&s, "StillLifeCouch", 0), "StillLifeCouch");
        assert_eq!(aim_line(&s, "Nothing", 5), "");
        assert!(contains_any("Sims3.Gameplay.Objects.Decorations.Flamingo", &["flamingo".into()]));
        assert!(!contains_any("", &["x".into()]));
        // (A collection's complete once each of its subjects is caught.)
        let mut rec = PhotoRecords::default();
        assert!(!rec.completed(&s, "Family"));
        rec.0.insert("FamilyMember".into(), (1, 0));
        assert!(rec.completed(&s, "Family"));
        assert_eq!(PhotoRecords::from_saved(&rec.to_saved()), rec);
    }
}
