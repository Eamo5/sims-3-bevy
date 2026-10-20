//! Conversation topics, as the game's (its `ActiveTopic`s): what's happened to a Sim lately (a
//! fire, a burglary, a birthday, a new outfit, a meal they cooked, a day at work, a skill come
//! on) is talked about for a while, by them or by others with them, opening the game's socials
//! offered under it ("Talk About Fire", "Compliment Cooking", "Announce Promotion"...). What a
//! Sim's doing is one too, while they do it and a little after ("Ask: Good show?"). Sims bring
//! them up of their own accord now and then: thanking the cook after dinner, congratulating
//! someone on their birthday.

use std::collections::HashMap;

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, Phase};
use crate::life::{LifeEvent, LifeEventKind};
use crate::sim::{HouseholdMember, Sim};

pub struct TopicsPlugin;

impl Plugin for TopicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AddTopic>().add_systems(Update, (give, (mess, expecting), events, doing, chatter).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// A topic given a Sim (`about`: the Sim it concerns, when it does: the cook to thank).
#[derive(Message, Clone, Copy, Debug)]
pub struct AddTopic {
    pub sim: Entity,
    pub name: &'static str,
    pub about: Option<Entity>,
}

impl AddTopic {
    pub fn new(sim: Entity, name: &'static str) -> Self {
        Self { sim, name, about: None }
    }
}

/// A Sim's topics.
#[derive(Component, Default, Debug, Clone)]
pub struct Topics(pub Vec<Had>);

/// A topic a Sim has: until when (game minutes), whom it's about, and those it's been used with
/// already (when it's used up once done between two Sims).
#[derive(Debug, Clone)]
pub struct Had {
    pub name: &'static str,
    pub until: f64,
    pub about: Option<Entity>,
    pub used_with: Vec<Entity>,
}

impl Topics {
    pub fn get(&self, name: &str) -> Option<&Had> {
        self.0.iter().find(|h| h.name == name)
    }

    /// Has it until then (or longer, if they had it already).
    pub fn add(&mut self, name: &'static str, until: f64, about: Option<Entity>) {
        match self.0.iter_mut().find(|h| h.name == name && h.about == about) {
            Some(h) => h.until = h.until.max(until),
            None => self.0.push(Had { name, until, about, used_with: Vec::new() }),
        }
    }
}

/// How long a topic lasts (game minutes), by the game's table: a day for those that last as
/// long as what gives them, two hours for any it doesn't have.
fn minutes(ui: Option<&crate::icons::GameUi>, name: &str) -> f64 {
    let hours = ui.and_then(|u| u.data.social_topics.iter().find(|t| t.name == name)).map_or(2.0, |t| if t.hours > 0.0 { t.hours } else { 24.0 });
    hours as f64 * 60.0
}

/// Whether a social under a topic is done only once between two Sims.
fn used_up(ui: Option<&crate::icons::GameUi>, name: &str) -> bool {
    ui.and_then(|u| u.data.social_topics.iter().find(|t| t.name == name)).is_none_or(|t| t.used_up)
}

/// What a Sim's doing is talked about while they do it and this long after (game minutes; the
/// game's `kMaxTimeToTalkAboutActionAfterFinishing`), by its topic.
const ACTION_MINUTES: f64 = 20.0;
const ACTION_TOPICS: &[(&str, &[&str])] = &[
    ("Action Watch TV", &["Watch TV", "Watch the TV"]),
    ("Action Read", &["Read a Book", "Read Book", "Read"]),
    ("Action Homework", &["Do Homework"]),
    ("Action Repair", &["Repair", "Upgrade"]),
    ("Action Music", &["Play Guitar", "Play Piano", "Play Drums", "Play Keyboard", "Play Laser Harp", "Play Xylophone", "Sing Karaoke"]),
    ("Action Athletic", &["Work Out", "Pump Iron", "Go Jogging"]),
    ("Action Sleep", &["Sleep", "Nap"]),
    ("Action Game", &["Play Computer Games", "Play Video Games", "Play Chess", "Play Arcade Game", "Play Pool", "Play Foosball", "Throw Darts"]),
    ("Action Read Newspaper", &["Read the Paper"]),
    ("Action Take Bath", &["Take Bath", "Take Bubble Bath"]),
];

/// The topic a skill coming on gives (the game's "Acquired ... Skill").
fn skill_topic(skill: &str) -> Option<&'static str> {
    Some(match skill {
        "Cooking" => "Acquired Cooking Skill",
        "Athletic" => "Acquired Athletic Skill",
        "Fishing" => "Acquired Fishing Skill",
        "Gardening" => "Acquired Gardening Skill",
        _ => return None,
    })
}

/// Below this the household worries about money (the game's `kWorryinglyLowFamilyFunds`).
const LOW_FUNDS: i64 = 500;
/// How long a topic lasting while a visit or a party is on is had at a time (game minutes; it's
/// looked at hourly).
const WHILE_ON: f64 = 70.0;
/// How far from a fire, or a meal's cook, Sims are taken to be there (metres).
const THERE: f32 = 30.0;

/// How near dirty dishes or mess a Sim is to notice it (metres), and how often it's looked at
/// (game minutes).
const MESS_NEAR: f32 = 5.0;
const MESS_EVERY: f64 = 15.0;

/// Mess about: dirty dishes for anyone near to complain of (and ask someone to clean up), and a
/// mess for the Neat (the game's broadcasters' "Complain About Dirty Dishes", "Disgusted By
/// Mess").
fn mess(
    clock: Res<GameClock>,
    mut next: Local<f64>,
    mut adds: MessageWriter<AddTopic>,
    sims: Query<(Entity, &Sim, &Transform, &InheritedVisibility), With<HouseholdMember>>,
    objects: Query<(&crate::interact::GameObject, &Transform)>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + MESS_EVERY;
    use crate::interact::ObjectKind as K;
    let dishes: Vec<Vec3> = objects.iter().filter(|(o, _)| o.kind == K::DirtyDishes).map(|(_, tf)| tf.translation).collect();
    let mess: Vec<Vec3> = objects.iter().filter(|(o, _)| matches!(o.kind, K::DirtyDishes | K::ClothingPile | K::Puddle)).map(|(_, tf)| tf.translation).collect();
    let near = |at: Vec3, spots: &[Vec3]| spots.iter().any(|p| (p.y - at.y).abs() < 1.8 && p.with_y(0.0).distance(at.with_y(0.0)) < MESS_NEAR);
    for (e, sim, tf, shown) in &sims {
        if !shown.get() || sim.age.is_little() {
            continue;
        }
        if near(tf.translation, &dishes) {
            debug!("{} notices the dirty dishes", sim.first);
            adds.write(AddTopic::new(e, "Complain About Dirty Dishes"));
        }
        if sim.traits.contains(&crate::life::Trait::Neat) && near(tf.translation, &mess) {
            adds.write(AddTopic::new(e, "Disgusted By Mess"));
        }
    }
}

/// A pregnancy begun is news to announce (the game's "Announce Pregnancy").
fn expecting(mut adds: MessageWriter<AddTopic>, new: Query<Entity, Added<crate::little::Pregnancy>>) {
    for e in &new {
        adds.write(AddTopic::new(e, "Announce Pregnancy"));
    }
}

/// Every Sim has their topics.
fn give(mut commands: Commands, sims: Query<Entity, (With<Sim>, Without<Topics>)>) {
    for e in &sims {
        commands.entity(e).insert(Topics::default());
    }
}

/// Topics given by what happens: life's events, a change of clothes, a swim, a fire, a burglar,
/// money running low; those past their time forgotten, those used up marked.
#[allow(clippy::too_many_arguments)]
fn events(
    clock: Res<GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    (mut adds, mut life, mut clothes): (MessageReader<AddTopic>, MessageReader<LifeEvent>, MessageReader<crate::laundry::ChangedClothes>),
    mut swum: RemovedComponents<crate::swim::Swimming>,
    fires: Query<&Transform, With<crate::fire::Fire>>,
    burglary: Option<Res<crate::burglar::Break>>,
    household: Option<Res<crate::interact::Household>>,
    (party, visitors): (Option<Res<crate::interact::Party>>, Query<(Entity, &InheritedVisibility), With<crate::interact::Visitor>>),
    mut was: Local<(bool, bool, f64, Option<Entity>)>,
    mut sims: Query<(Entity, &mut Topics, &Transform, &InheritedVisibility, Has<HouseholdMember>)>,
) {
    let ui = ui.as_deref();
    let now = clock.minutes;
    let mut given: Vec<(Entity, &'static str, Option<Entity>)> = adds.read().map(|a| (a.sim, a.name, a.about)).collect();
    let mut used: Vec<(Entity, Entity, &'static str)> = Vec::new();
    let near = |at: Vec3, sims: &Query<(Entity, &mut Topics, &Transform, &InheritedVisibility, Has<HouseholdMember>)>| -> Vec<Entity> {
        sims.iter().filter(|(_, _, tf, shown, _)| shown.get() && tf.translation.distance(at) < THERE).map(|(e, ..)| e).collect()
    };
    for ev in life.read() {
        let e = ev.sim;
        match &ev.kind {
            LifeEventKind::Birthday => given.push((e, "Birthday", None)),
            LifeEventKind::Promoted => given.push((e, "Announce Promotion", None)),
            LifeEventKind::Engaged => given.push((e, "Announce Engagement", None)),
            LifeEventKind::Married => given.extend([(e, "Wedding", None), (e, "Just Married", None)]),
            LifeEventKind::SkillUp { skill, .. } => given.extend(skill_topic(skill).map(|t| (e, t, None))),
            LifeEventKind::HomeFromWork { worse, .. } => {
                given.push((e, "After Work", None));
                if *worse {
                    given.push((e, "Complain About Work", None));
                }
            }
            LifeEventKind::Finished { activity, .. } => match *activity {
                // (The cook's made food; those at the meal thank them.)
                "Cook Dinner" | "Grill" => {
                    given.push((e, "Has Made Food", None));
                    if let Ok((_, _, tf, ..)) = sims.get(e) {
                        given.extend(near(tf.translation, &sims).into_iter().filter(|o| *o != e).map(|o| (o, "Thank For Cooking", Some(e))));
                    }
                }
                "Bake Birthday Cake" | "Prepare Food" => given.push((e, "Has Made Food", None)),
                "Clean" | "Mop Up" | "Clean Up" | "Wash Dishes" | "Load Dishes" => given.push((e, "Thank For Cleaning", None)),
                // (A novel finished: praised by those who hear of it.)
                "Write Novel" => given.push((e, "Written Book", None)),
                _ => {}
            },
            // (Done between them: a topic of theirs that's used up once done isn't again.)
            LifeEventKind::Socialized { other, social } => {
                if let Some((_, s)) = crate::social::all_socials().find(|(_, s)| s.name == *social)
                    && let Some(g) = s.game
                {
                    for (t, _) in &g.topics {
                        if let crate::social::Topic::Event(name) = t
                            && used_up(ui, name)
                        {
                            used.push((e, *other, name));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for c in clothes.read() {
        if c.to != Some(crate::simbody::OutfitKind::Naked) {
            given.push((c.sim, "Outfit Change", None));
        }
    }
    for e in swum.read() {
        given.push((e, "Just Been Swimming", None));
    }
    // (A fire breaking out: all those there.)
    let burning = !fires.is_empty();
    if burning && !was.0 {
        let at: Vec<Vec3> = fires.iter().map(|f| f.translation).collect();
        let there: Vec<Entity> = sims.iter().filter(|(_, _, tf, shown, member)| *member || (shown.get() && at.iter().any(|f| f.distance(tf.translation) < THERE))).map(|(e, ..)| e).collect();
        given.extend(there.into_iter().map(|e| (e, "Talk About Fire", None)));
    }
    was.0 = burning;
    // (A burglar seen: the household.)
    let spotted = burglary.is_some_and(|b| b.spotted());
    if spotted && !was.1 {
        given.extend(sims.iter().filter(|s| s.4).map(|(e, ..)| (e, "Talk About Burglary", None)));
    }
    was.1 = spotted;
    // (A party: the host's while it's on, and once it's over.)
    match (party.as_deref(), was.3) {
        (Some(p), _) => {
            if was.3.is_none() {
                given.push((p.host, "Party Happening", None));
            }
            was.3 = Some(p.host);
        }
        (None, Some(host)) => {
            given.push((host, "Party Over", None));
            was.3 = None;
        }
        (None, None) => {}
    }
    // (Money running low, and visitors over, looked at hourly: a visit's for the household and
    // its guests.)
    if now - was.2 >= 60.0 {
        was.2 = now;
        if household.is_some_and(|h| h.funds < LOW_FUNDS) {
            given.extend(sims.iter().filter(|s| s.4).map(|(e, ..)| (e, "Worry About Money", None)));
        }
        if let Some(p) = party.as_deref() {
            given.push((p.host, "Party Happening", None));
        }
        let guests: Vec<Entity> = visitors.iter().filter(|(_, shown)| shown.get()).map(|(e, _)| e).collect();
        if !guests.is_empty() {
            given.extend(guests.into_iter().map(|e| (e, "Visit", None)));
            given.extend(sims.iter().filter(|s| s.4 && s.3.get()).map(|(e, ..)| (e, "Visit", None)));
        }
    }
    for (e, name, about) in given {
        if let Ok((_, mut t, ..)) = sims.get_mut(e) {
            debug!("topic for {e}: {name}");
            // (Those that last while the visit or the party does: until the next look.)
            let last = if matches!(name, "Visit" | "Party Happening") { WHILE_ON } else { minutes(ui, name) };
            t.add(name, now + last, about);
        }
    }
    for (e, other, name) in used {
        if let Ok((_, mut t, ..)) = sims.get_mut(e) {
            for h in t.0.iter_mut().filter(|h| h.name == name && !h.used_with.contains(&other)) {
                h.used_with.push(other);
            }
        }
    }
    for (_, mut t, ..) in &mut sims {
        if t.0.iter().any(|h| h.until < now) {
            t.0.retain(|h| h.until >= now);
        }
    }
}

/// What a Sim's doing is a topic while they do it, and a little after.
fn doing(clock: Res<GameClock>, mut sims: Query<(&ActionQueue, &mut Topics)>) {
    for (q, mut t) in &mut sims {
        let Some(a) = q.0.front() else { continue };
        // (Not a social of the same name: telling someone to "Do Homework".)
        if !matches!(a.phase, Phase::Running(_)) || matches!(a.kind, ActionKind::Social { .. }) {
            continue;
        }
        if let Some((topic, _)) = ACTION_TOPICS.iter().find(|(_, labels)| labels.contains(&a.label.as_str())) {
            t.add(topic, clock.minutes + ACTION_MINUTES, None);
        }
    }
}

/// How often an idle Sim may bring up a topic (game minutes), how likely they are to then, how
/// near the other must be (metres), and how long before they bring one up again.
const CHATTER_EVERY: f64 = 5.0;
const CHATTER_CHANCE: f64 = 0.3;
const CHATTER_REACH: f32 = 10.0;
const CHATTER_REST: f64 = 60.0;
/// How long before a Sim brings up the same thing again (game minutes).
const SAID_REST: f64 = 24.0 * 60.0;
/// The traits that have a Sim say something mean unasked (the rest, only to those they dislike).
const MEAN: &[crate::life::Trait] = &[crate::life::Trait::MeanSpirited, crate::life::Trait::Evil, crate::life::Trait::Grumpy, crate::life::Trait::Inappropriate, crate::life::Trait::HotHeaded];
/// How much a Sim must dislike someone to be mean to them unasked, and how much romance there
/// must be for something romantic.
const DISLIKE: f32 = -20.0;
const FANCY: f32 = 30.0;

/// Sims bring up a topic of their own accord now and then: one of the game's socials under a
/// topic that's there between them and someone near (what's happened to one of them, a trait, a
/// moodlet), that they'd do unasked (its `Available auto`), as their traits favour. Something
/// mean only from the mean, or to someone disliked; something romantic only where there's
/// romance; nothing said again for a day.
#[allow(clippy::type_complexity)]
fn chatter(
    clock: Res<GameClock>,
    settings: Res<crate::options::Settings>,
    mut next: Local<f64>,
    mut rested: Local<HashMap<Entity, f64>>,
    mut said: Local<HashMap<(Entity, &'static str), f64>>,
    members: Query<Has<crate::sim::Selected>, With<HouseholdMember>>,
    sides: Query<crate::social::SideQuery>,
    rels: Query<&crate::social::Relationships>,
    mut sims: Query<(Entity, &Sim, &Transform, &InheritedVisibility, &mut ActionQueue, &crate::sim::SimAnim), (Without<crate::interact::AtWork>, Without<crate::rabbitholes::AtRabbitHole>)>,
) {
    if clock.minutes < *next {
        return;
    }
    *next = clock.minutes + CHATTER_EVERY;
    let mut rng = rand::rng();
    let about: Vec<(Entity, Sim, Vec3)> = sims.iter().filter(|(_, s, _, shown, ..)| shown.get() && !s.age.is_little()).map(|(e, s, tf, ..)| (e, s.clone(), tf.translation)).collect();
    for (me, sim, tf, shown, mut queue, anim) in &mut sims {
        // (The household's own free will, as the rest of what they do unasked.)
        if members.get(me).is_ok_and(|selected| settings.free_will_of(true, selected) == crate::options::FreeWill::Off) {
            continue;
        }
        if !shown.get() || sim.age.is_little() || !queue.0.is_empty() || anim.pose == crate::sim::Pose::Lie || rested.get(&me).is_some_and(|t| clock.minutes < *t) || !rng.random_bool(CHATTER_CHANCE) {
            continue;
        }
        let mine = crate::social::Side::of(me, sides.get(me).ok());
        let Ok(my_rels) = rels.get(me) else { continue };
        let mut fits: Vec<(usize, Entity, &'static str, f32)> = Vec::new();
        for (other, target, at) in &about {
            if *other == me || at.distance(tf.translation) > CHATTER_REACH {
                continue;
            }
            let theirs = crate::social::Side::of(*other, sides.get(*other).ok());
            let rel = my_rels.get(*other);
            let mean = sim.traits.iter().any(|t| MEAN.contains(t)) || rel.friendship < DISLIKE;
            for (i, s) in crate::social::all_socials() {
                let Some(g) = s.game else { continue };
                if !g.auto || g.topics.iter().all(|(t, _)| *t == crate::social::Topic::Always) || !crate::social::actor_may(g, &sim, true) || said.contains_key(&(me, s.name)) {
                    continue;
                }
                let fits_tone = match s.cat {
                    crate::social::SocialCat::Mean => mean,
                    crate::social::SocialCat::Romantic => rel.romance >= FANCY,
                    _ => true,
                };
                if !fits_tone {
                    continue;
                }
                if crate::social::available(s, &rel, &sim, target, members.contains(*other), false, [mine, theirs]) {
                    let keen = if g.keen.iter().any(|t| sim.traits.contains(t)) { 3.0 } else { 1.0 };
                    fits.push((i, *other, s.name, keen));
                }
            }
        }
        use rand::seq::IndexedRandom;
        if let Ok((i, other, name, _)) = fits.choose_weighted(&mut rng, |f| f.3).copied() {
            debug!("{} brings up {name}", sim.first);
            rested.insert(me, clock.minutes + CHATTER_REST);
            said.insert((me, name), clock.minutes + SAID_REST);
            queue.0.push_back(Action::new(name, ActionKind::Social { target: other, social: i }, true));
        }
    }
    rested.retain(|_, t| *t > clock.minutes);
    said.retain(|_, t| *t > clock.minutes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_last_their_time_and_refresh() {
        let mut t = Topics::default();
        t.add("Birthday", 100.0, None);
        t.add("Birthday", 50.0, None);
        assert_eq!(t.0.len(), 1);
        assert_eq!(t.get("Birthday").unwrap().until, 100.0, "not cut short");
        let cook = Entity::from_raw_u32(7).unwrap();
        t.add("Thank For Cooking", 120.0, Some(cook));
        assert_eq!(t.get("Thank For Cooking").unwrap().about, Some(cook));
        assert!(t.get("Talk About Fire").is_none());
    }

    #[test]
    fn whats_being_done_is_a_topic() {
        let topic = |label: &str| ACTION_TOPICS.iter().find(|(_, l)| l.contains(&label)).map(|t| t.0);
        assert_eq!(topic("Watch TV"), Some("Action Watch TV"));
        assert_eq!(topic("Play Guitar"), Some("Action Music"));
        assert_eq!(topic("Use Toilet"), None);
        assert_eq!(skill_topic("Cooking"), Some("Acquired Cooking Skill"));
    }
}
