//! The game's wishes ("Dreams and Promises"), as its `DreamsAndPromisesManager` loads them
//! (`ParseDreamsAndPromises`): the wish types (`DreamsAndPromisesNodes`' `Primitives`), the dream
//! trees (resources 0x0604ABDA, each node a wish or a hidden trigger of one of those types, its
//! fields over the `NodeDefaults`), and the tables beside them (`MiscDnPData`'s equivalent wishes
//! and those valid after death, `LinkedWishes`, `DnPDefaultIcons`, `ObjectCategoryData`). The base
//! game's only: a tree or type of another pack's is left out, and with a type its nodes and all
//! beneath them, as the game drops what isn't installed.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The dream trees' resource type, and the timer nodes' prototype (they've no type of their own).
pub const T_DREAM_TREE: u32 = 0x0604_ABDA;
pub const TIMER: u32 = 245_410_120;
/// What a node's `FulfillmentScore` is multiplied by on loading
/// (`kLifetimeHappinessGlobalWishMultiplier`).
const SCORE_MULTIPLIER: f32 = 10.0;

/// A wish type (the game's `DreamNodePrimitive`): its id and designer's name ("Learn $skill"; a
/// social's, the social's key), the event it listens for, its subject's kind, whether it's a
/// social's, whether others' doings fulfil it, the game's functions that test and fulfil it (by
/// name), its icons, and its words (generic, a man's, a woman's, the description).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnpPrimitive {
    pub id: u32,
    pub name: String,
    pub event: String,
    pub subject: String,
    pub social: bool,
    pub third_party: bool,
    pub vacation: bool,
    pub species: String,
    pub check: String,
    pub feedback: String,
    pub start: String,
    pub count_text: String,
    pub icon: String,
    pub icon2: String,
    pub generic: String,
    pub male: String,
    pub female: String,
    pub description: String,
}

/// A node's trait: wanted, required or ruled out.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnpTrait {
    pub name: String,
    pub required: bool,
    pub excluded: bool,
}

/// The axes a node is weighed on, in this order (its `AgeWeight`, `TraitsWeight`...).
pub const AGE: usize = 0;
pub const TRAITS: usize = 1;
pub const RELATIONSHIP: usize = 2;
pub const MOOD: usize = 3;
pub const CAREERS: usize = 4;
pub const SKILLS: usize = 5;

/// A node of a dream tree (the game's `DreamNodeInstance`): its type (or a timer), whether it's a
/// wish to offer or a hidden trigger, how it repeats, whether it's offered when its parent's
/// promised rather than done, the chance its completion offers a child, its points (already
/// times ten), how long it's offered and lasts, how it's weighed, its subject and number, its
/// words in place of its type's, a timer's beats, a social's role, and its children.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnpNode {
    pub id: u32,
    pub prim: u32,
    pub visible: bool,
    pub repeatable: bool,
    pub promise_link: bool,
    pub next_chance: f32,
    pub score: f32,
    pub display_hours: f32,
    pub satisfaction_hours: f32,
    pub repeat_timeout: f32,
    pub between_repeats: f32,
    pub min_ad: f32,
    pub max_ad: f32,
    pub start: bool,
    pub zero: [bool; 6],
    pub weight: [f32; 6],
    pub ages: Vec<String>,
    pub traits: Vec<DnpTrait>,
    pub min_traits: f32,
    pub skills: Vec<String>,
    pub skill_range: [f32; 2],
    pub inherit_skill: bool,
    pub careers: Vec<String>,
    pub career_range: [f32; 2],
    pub inherit_career: bool,
    pub moods: Vec<String>,
    pub mood_range: [f32; 2],
    pub relationships: Vec<String>,
    pub rel_range: [f32; 2],
    pub inherit_subject: bool,
    pub input_subject: String,
    pub input_number: f32,
    /// Replacement words' keys: generic, a man's, a woman's, the description.
    pub text_keys: [String; 4],
    pub satisfy_on_start: bool,
    pub satisfies: u32,
    pub between_satisfies: f32,
    pub social_role: String,
    /// Its children, by index in the tree's nodes.
    pub children: Vec<usize>,
}

/// A dream tree: its id (the resource's instance), name, category, hash (a changed one re-seeds
/// it in a save), its root, and its nodes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnpTree {
    pub id: u64,
    pub name: String,
    pub category: String,
    pub hash: u32,
    pub root: usize,
    pub nodes: Vec<DnpNode>,
}

/// A wish that helps toward a lifetime wish (`LinkedWishes`): its type, subject (if any), number
/// (if any), and the lifetime wish's type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnpLink {
    pub prim: u32,
    pub subject: String,
    pub number: Option<f32>,
    pub lifetime: u32,
}

/// An object category (`ObjectCategoryData`): its key, the classes it covers, its wish icon, and
/// its name.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnpCategory {
    pub key: String,
    pub classes: Vec<String>,
    pub icon: String,
    pub name: String,
    pub vacation: bool,
}

/// All of it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnpData {
    pub primitives: Vec<DnpPrimitive>,
    pub trees: Vec<DnpTree>,
    /// Wish types that count as one another (`MiscDnPData`'s `EquivalentNodes`).
    pub equivalent: Vec<Vec<u32>>,
    /// Wish types still good after their subject's died (`IsValidAfterDeath`).
    pub after_death: Vec<u32>,
    pub links: Vec<DnpLink>,
    /// The icon for a subject's kind, and the corner numbers' (`DnPDefaultIcons`).
    pub subject_icons: Vec<(String, String)>,
    pub number_icons: Vec<(u32, String)>,
    pub categories: Vec<DnpCategory>,
}

impl DnpData {
    pub fn primitive(&self, id: u32) -> Option<&DnpPrimitive> {
        self.primitives.iter().find(|p| p.id == id)
    }
}

/// A primitive's name as the game's `DreamNames` enum has it (`OutputPrimitives`): lower case,
/// `$n` "num", `$` "var ", no punctuation, words joined by `_`, `&` "_and_", `*`
/// "major_dream_".
pub fn dream_name(name: &str) -> String {
    let mut s = name.to_ascii_lowercase().replace("$n", "num").replace('$', "var ");
    s.retain(|c| !",':!()<>\"?".contains(c));
    s.replace([' ', '-', '/', '.'], "_").replace('&', "_and_").replace('*', "major_dream_")
}

/// The text of a tag in a piece of XML (`<Tag>text</Tag>`; empty for `<Tag />`).
fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let a = xml.find(&open)? + open.len();
    let b = xml[a..].find(&format!("</{name}>"))?;
    Some(xml[a..a + b].trim())
}

/// The `<string>`s of a list tag.
fn strings(xml: &str, name: &str) -> Vec<String> {
    let Some(body) = tag(xml, name) else { return Vec::new() };
    body.split("<string>").skip(1).filter_map(|s| s.split("</string>").next()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// The blocks of a repeated element (`<InstanceNode ...> ... </InstanceNode>`).
fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{name}"), format!("</{name}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(a) = rest.find(&open) {
        let Some(b) = rest[a..].find(&close) else { break };
        out.push(&rest[a..a + b]);
        rest = &rest[a + b + close.len()..];
    }
    out
}

fn yes(v: Option<&str>, or: bool) -> bool {
    v.map_or(or, |v| v.eq_ignore_ascii_case("true"))
}

fn num(v: Option<&str>, or: f32) -> f32 {
    v.and_then(|v| v.parse().ok()).unwrap_or(or)
}

/// A node from its XML, over the `NodeDefaults` (the parent's id beside it).
fn node(x: &str) -> (DnpNode, u32, bool) {
    let t = |n: &str| tag(x, n);
    let traits = tag(x, "Traits").map_or(Vec::new(), |body| {
        blocks(body, "SelectedTrait")
            .into_iter()
            .map(|b| DnpTrait { name: tag(b, "Trait").unwrap_or("").to_string(), required: yes(tag(b, "Required"), false), excluded: yes(tag(b, "Excluded"), false) })
            .collect()
    });
    let key = |n: &str| tag(x, n).and_then(|b| tag(b, "StringKey")).unwrap_or("").to_string();
    let n = DnpNode {
        id: t("Id").and_then(|v| v.parse().ok()).unwrap_or(0),
        prim: t("PrototypeId").and_then(|v| v.parse().ok()).unwrap_or(0),
        visible: yes(t("DreamVisible"), true),
        repeatable: yes(t("Repeatable"), false),
        promise_link: yes(t("PromiseConnection"), false),
        next_chance: num(t("NextDreamSelectionProbability"), 100.0),
        score: num(t("FulfillmentScore"), 0.0) * SCORE_MULTIPLIER,
        display_hours: num(t("DisplayTime"), 6.0),
        satisfaction_hours: num(t("SatisfactionTime"), 0.0),
        repeat_timeout: num(t("RepeatableTimeout"), 0.0),
        between_repeats: num(t("TimeBetweenRepeats"), 0.0),
        min_ad: num(t("MinAdvertisement"), 0.0),
        max_ad: num(t("MaxAdvertisement"), 100.0),
        start: yes(t("PotentialStartNode"), false),
        zero: ["Age", "Traits", "Relationship", "Mood", "Careers", "Skills"].map(|a| yes(t(&format!("{a}ZeroCheck")), false)),
        weight: ["Age", "Traits", "Relationship", "Mood", "Careers", "Skills"].map(|a| num(t(&format!("{a}Weight")), 0.0)),
        ages: strings(x, "AgeGroups"),
        traits,
        min_traits: num(t("MinTraitsMatch"), 0.0),
        skills: strings(x, "Skills"),
        skill_range: [num(t("MinSkills"), 0.0), num(t("MaxSkills"), 0.0)],
        inherit_skill: yes(t("InheritSkill"), false),
        careers: strings(x, "Careers"),
        career_range: [num(t("MinCareers"), 0.0), num(t("MaxCareers"), 0.0)],
        inherit_career: yes(t("InheritCareer"), false),
        moods: strings(x, "MoodFlavors"),
        mood_range: [num(t("MinMood"), 0.0), num(t("MaxMood"), 0.0)],
        relationships: strings(x, "Relationships"),
        rel_range: [num(t("MinRelationship"), 0.0), num(t("MaxRelationship"), 0.0)],
        inherit_subject: yes(t("InheritSubject"), false),
        input_subject: t("InputSubject").unwrap_or("").to_string(),
        input_number: num(t("InputNumber"), 0.0),
        text_keys: [key("LocalizationKeyGenericOverride"), key("LocalizationKeyMaleOverride"), key("LocalizationKeyFemaleOverride"), key("LocalizationKeyDescriptionOverride")],
        satisfy_on_start: yes(t("SatisfyOnStart"), false),
        satisfies: num(t("NumberSatisfies"), 0.0) as u32,
        between_satisfies: num(t("TimeBetweenSatisfies"), 0.0),
        social_role: t("SocialRole").unwrap_or("kActor").to_string(),
        children: Vec::new(),
    };
    let parent = t("ParentId").and_then(|v| v.parse().ok()).unwrap_or(u32::MAX);
    (n, parent, yes(t("IsRoot"), false))
}

/// A dream tree from its resource, if it's the base game's: its nodes those whose types are
/// loaded (or timers) and whose parents are, children linked.
pub fn tree(id: u64, xml: &str, known: &dyn Fn(u32) -> bool) -> Option<DnpTree> {
    // (Its own fields: what's left without its nodes.)
    let mut own = xml.to_string();
    for b in blocks(xml, "InstanceNode") {
        own = own.replacen(b, "", 1);
    }
    if tag(&own, "RequiredProductVersions").is_some_and(|v| !v.split([',', ' ']).any(|p| p.eq_ignore_ascii_case("BaseGame"))) {
        return None;
    }
    let parsed: Vec<(DnpNode, u32, bool)> = blocks(xml, "InstanceNode").into_iter().map(node).collect();
    let root = parsed.iter().position(|(n, p, r)| *r || *p == n.id)?;
    // (Down from the root: a node whose type isn't loaded goes with all beneath it.)
    let by_id: HashMap<u32, usize> = parsed.iter().enumerate().map(|(i, (n, ..))| (n.id, i)).collect();
    let mut kids: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, (n, p, _)) in parsed.iter().enumerate() {
        if i != root
            && let Some(pi) = by_id.get(p)
            && *pi != i
        {
            kids.entry(*pi).or_default().push(i);
        }
        let _ = n;
    }
    let ok = |i: usize| parsed[i].0.prim == TIMER || known(parsed[i].0.prim);
    if !ok(root) {
        return None;
    }
    let mut keep: Vec<usize> = vec![root];
    let mut at = 0;
    while at < keep.len() {
        let i = keep[at];
        at += 1;
        for k in kids.get(&i).into_iter().flatten() {
            if ok(*k) && !keep.contains(k) {
                keep.push(*k);
            }
        }
    }
    let index: HashMap<usize, usize> = keep.iter().enumerate().map(|(new, old)| (*old, new)).collect();
    let mut nodes: Vec<DnpNode> = keep.iter().map(|i| parsed[*i].0.clone()).collect();
    for (new, old) in keep.iter().enumerate() {
        nodes[new].children = kids.get(old).into_iter().flatten().filter_map(|k| index.get(k).copied()).collect();
    }
    Some(DnpTree {
        id,
        name: tag(&own, "Name").unwrap_or("").to_string(),
        category: tag(&own, "Category").unwrap_or("").to_string(),
        hash: tag(&own, "TreeHash").and_then(|v| v.parse().ok()).unwrap_or(0),
        root: 0,
        nodes,
    })
}

/// The wish types from `DreamsAndPromisesNodes` (its rows over its first, the defaults), the
/// base game's, with their words; and every type's `DreamNames` name (all packs', for the names'
/// order).
pub fn primitives(rows: &[HashMap<String, String>], words: &dyn Fn(&str) -> String) -> (Vec<DnpPrimitive>, HashMap<String, u32>) {
    let defaults = rows.first().cloned().unwrap_or_default();
    let mut out = Vec::new();
    let mut names: HashMap<String, u32> = HashMap::new();
    for r in rows.iter().skip(1) {
        let g = |k: &str| r.get(k).or_else(|| defaults.get(k)).cloned().unwrap_or_default();
        let Ok(id) = g("Id").parse::<u32>() else { continue };
        let mut dn = dream_name(&g("Name"));
        while names.contains_key(&dn) {
            dn.push('_');
        }
        names.insert(dn, id);
        if !g("RequiredProductVersions").split([',', ' ']).any(|p| p.eq_ignore_ascii_case("BaseGame")) {
            continue;
        }
        let key = |f: &str| words(&format!("Gameplay/Dreams/{id:08x}:{f}"));
        out.push(DnpPrimitive {
            id,
            name: g("Name"),
            event: g("TriggerEvent"),
            subject: g("SubjectType"),
            social: g("IsSocialPrimitive").eq_ignore_ascii_case("true"),
            third_party: g("SatisfiableByThirdParty").eq_ignore_ascii_case("true"),
            vacation: !g("IsValidInVacationWorld").eq_ignore_ascii_case("false"),
            species: g("SpeciesAvailability"),
            check: g("CheckFunction"),
            feedback: g("FeedbackFunction"),
            start: g("PotentialStartFunction"),
            count_text: g("CountTextFunction"),
            icon: g("PrimaryIcon"),
            icon2: g("SecondaryIcon"),
            generic: key("GenericName"),
            male: key("MaleName"),
            female: key("FemaleName"),
            description: key("Description"),
        });
    }
    (out, names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dream_names_as_the_game_makes_them() {
        assert_eq!(dream_name("Learn $skill"), "learn_var_skill");
        assert_eq!(dream_name("Reach level $n of $career"), "reach_level_num_of_var_career");
        assert_eq!(dream_name("*Level 10 of culinary"), "major_dream_level_10_of_culinary");
        assert_eq!(dream_name("Rock & Roll!"), "rock__and__roll");
    }

    #[test]
    fn a_tree_keeps_what_its_types_allow() {
        // (Ids past what an f32 holds exactly, as the game's are.)
        let n = |id: u32, parent: u32, prim: u32, extra: &str| format!("<InstanceNode d2p1:type=\"DreamNodeInstance\"><Id>{id}</Id><ParentId>{parent}</ParentId><PrototypeId>{prim}</PrototypeId>{extra}<AgeGroups /><Traits /></InstanceNode>");
        let xml = format!(
            "<DreamTree Version=\"1\"><Category>Traits</Category>{}{}{}{}<Name>Couch Potato</Name><RequiredProductVersions>BaseGame</RequiredProductVersions><TreeHash>7</TreeHash></DreamTree>",
            n(966197961, 966197961, 10, "<IsRoot>true</IsRoot><DreamVisible>false</DreamVisible>"),
            n(966197963, 966197961, 20, "<FulfillmentScore>15</FulfillmentScore><Traits><SelectedTrait><Trait>CouchPotato</Trait><Required>true</Required><Excluded>false</Excluded></SelectedTrait></Traits><AgeGroups><string>Adult</string></AgeGroups>"),
            n(966197965, 966197961, 99, ""),
            n(966197967, 966197965, 20, ""),
        );
        let t = tree(5, &xml, &|p| p != 99).expect("a base game tree");
        assert_eq!((t.name.as_str(), t.hash, t.nodes.len()), ("Couch Potato", 7, 2), "the pack's node and its child gone");
        assert_eq!(t.nodes[0].children, vec![1]);
        let w = &t.nodes[1];
        assert_eq!((w.score, w.visible, w.ages.clone()), (150.0, true, vec!["Adult".to_string()]));
        assert!(w.traits[0].required && w.traits[0].name == "CouchPotato");
        assert!(tree(5, &xml.replace(">BaseGame<", ">EP3<"), &|_| true).is_none());
    }
}
