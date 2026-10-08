//! Relationships and social interactions: separate friendship and romance, relationship
//! statuses (going steady, engaged, married), socials grouped like the game's pie menu, and
//! the chance a Sim accepts them.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::life::{Mood, Trait};
use crate::sim::{Age, Sim};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RelStatus {
    #[default]
    None,
    /// "Going steady".
    Partner,
    Engaged,
    Married,
    /// Broke up or divorced.
    Ex,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Relationship {
    pub friendship: f32,
    pub romance: f32,
    pub status: RelStatus,
    pub kissed: bool,
    /// When they last spent time together (game minutes; 0: not known, taken as now).
    pub last: f64,
}

/// Days without seeing or speaking to each other before a relationship starts to fade.
pub const FADE_AFTER_DAYS: f64 = 3.0;

/// Keeps track of when Sims last spent time together (a social in person or on the phone).
pub fn note_contact(clock: Res<crate::clock::GameClock>, mut events: MessageReader<crate::life::LifeEvent>, mut rels: Query<&mut Relationships>) {
    for ev in events.read() {
        if let crate::life::LifeEventKind::Socialized { other, .. } = ev.kind
            && let Ok(mut r) = rels.get_mut(ev.sim)
        {
            r.entry(other).last = clock.minutes;
        }
    }
}

/// Relationships fade when Sims don't see or speak to each other for a few days, as the game's
/// do: friends drift towards acquaintances (good friends more slowly), romance cools unless
/// they're together, and grudges soften. Spouses, partners and family stay close. The
/// household's relationships, both ways (the town's Sims among themselves are left as they
/// are).
pub fn fade_relationships(
    clock: Res<crate::clock::GameClock>,
    mut last_hour: Local<i64>,
    mut q: Query<(Entity, &crate::sim::Sim, &mut Relationships, Has<crate::sim::HouseholdMember>, Option<&crate::wishes::Wishes>, Option<&crate::journal::SkillJournal>)>,
    family: Res<crate::family::Genealogy>,
) {
    let hour = (clock.minutes / 60.0) as i64;
    if hour == *last_hour {
        return;
    }
    *last_hour = hour;
    let household: std::collections::HashSet<Entity> = q.iter().filter(|x| x.3).map(|x| x.0).collect();
    let ids: HashMap<Entity, u64> = q.iter().map(|x| (x.0, x.1.id)).collect();
    for (_, sim, mut rels, member, wishes, journal) in &mut q {
        // (A Long Distance Friend's friends never drift, nor a Super Friendly Sim's.)
        if crate::wishes::has(wishes, "LongDistanceFriend") || crate::journal::earned(journal, "Super Friendly") {
            continue;
        }
        for (other, r) in rels.0.iter_mut() {
            if r.last <= 0.0 {
                r.last = clock.minutes;
                continue;
            }
            if (!member && !household.contains(other)) || (clock.minutes - r.last) / 1440.0 < FADE_AFTER_DAYS {
                continue;
            }
            let together = matches!(r.status, RelStatus::Married | RelStatus::Engaged | RelStatus::Partner);
            let kin = ids.get(other).is_some_and(|o| family.kin(sim.id, *o).is_some());
            let per_day = if together || kin {
                0.25
            } else if r.friendship >= 60.0 {
                0.5
            } else {
                1.0
            };
            let step = per_day / 24.0;
            let floor = if together || kin { 40.0 } else { 0.0 };
            if r.friendship > floor {
                r.friendship = (r.friendship - step).max(floor);
            } else if r.friendship < 0.0 {
                r.friendship = (r.friendship + step * 0.5).min(0.0);
            }
            if !together && r.romance > 0.0 {
                r.romance = (r.romance - step).max(0.0);
            }
        }
    }
}

impl Relationship {
    /// What they are to one another, in the words for `other` (a woman or a man): "Wife",
    /// "Boyfriend", "Fiancée", "Good Friend".
    pub fn label_for(&self, other_female: bool) -> String {
        let pick = |f: &'static str, m: &'static str| if other_female { f } else { m };
        match self.status {
            RelStatus::Married => pick("Wife", "Husband").into(),
            RelStatus::Engaged => pick("Fiancée", "Fiancé").into(),
            RelStatus::Partner => pick("Girlfriend", "Boyfriend").into(),
            _ => self.label(),
        }
    }

    pub fn label(&self) -> String {
        let friend = match self.friendship {
            v if v < -60.0 => "Enemy",
            v if v < -20.0 => "Disliked",
            v if v < 15.0 => "Acquaintance",
            v if v < 40.0 => "Friend",
            v if v < 75.0 => "Good Friend",
            _ => "Best Friend",
        };
        match self.status {
            RelStatus::Married => "Spouse".into(),
            RelStatus::Engaged => "Fiancé(e)".into(),
            RelStatus::Partner => "Partner".into(),
            RelStatus::Ex => format!("Ex · {friend}"),
            RelStatus::None if self.romance >= 20.0 => format!("{friend} · Romantic Interest"),
            RelStatus::None => friend.into(),
        }
    }
}

#[derive(Component, Default, Clone)]
pub struct Relationships(pub HashMap<Entity, Relationship>);

impl Relationships {
    pub fn get(&self, e: Entity) -> Relationship {
        self.0.get(&e).copied().unwrap_or_default()
    }
    pub fn friendship(&self, e: Entity) -> f32 {
        self.get(e).friendship
    }
    pub fn entry(&mut self, e: Entity) -> &mut Relationship {
        self.0.entry(e).or_default()
    }
    pub fn add(&mut self, e: Entity, friendship: f32, romance: f32) {
        let r = self.entry(e);
        r.friendship = (r.friendship + friendship).clamp(-100.0, 100.0);
        r.romance = (r.romance + romance).clamp(-100.0, 100.0);
    }
    /// The Sim's current partner, fiancé(e) or spouse.
    pub fn partner(&self) -> Option<(Entity, RelStatus)> {
        self.0.iter().find(|(_, r)| matches!(r.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married)).map(|(e, r)| (*e, r.status))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SocialCat {
    Friendly,
    Funny,
    Romantic,
    Mean,
    Special,
    /// Looking after a baby or toddler.
    Care,
}

impl SocialCat {
    pub const ALL: [SocialCat; 6] = [SocialCat::Care, SocialCat::Friendly, SocialCat::Funny, SocialCat::Romantic, SocialCat::Mean, SocialCat::Special];
    pub fn name(self) -> &'static str {
        match self {
            SocialCat::Friendly => "Friendly",
            SocialCat::Funny => "Funny",
            SocialCat::Romantic => "Romantic",
            SocialCat::Mean => "Mean",
            SocialCat::Special => "Special",
            SocialCat::Care => "Care",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SocialEffect {
    None,
    /// Throwing a ball back and forth, a few metres apart (with a ball about).
    PlayCatch,
    Kiss,
    GoSteady,
    Propose,
    Marry,
    BreakUp,
    MoveIn,
    AskToLeave,
    WooHoo,
    TryForBaby,
    PutToBed,
    AskOnDate,
    /// A toddler's lesson in walking or talking.
    TeachWalk,
    TeachTalk,
    /// A scuffle: one wins, the other is left embarrassed.
    Fight,
    /// Making up after falling out.
    Apologize,
    DeclareNemesis,
    /// Comforting a Sim in a bad mood.
    CheerUp,
    /// A massage that leaves the other Sim comfortable.
    BackRub,
    /// Helping a child or teen through their homework (it's done, and done well).
    HelpHomework,
    /// Greeting a caller at the door (who then comes in).
    Greet,
}

pub struct SocialDef {
    pub name: &'static str,
    pub cat: SocialCat,
    pub minutes: f32,
    pub social_per_hour: f32,
    pub fun_per_hour: f32,
    pub friendship: f32,
    pub romance: f32,
    pub min_friendship: f32,
    pub min_romance: f32,
    /// Only for Sims who don't get on this well (a fight, an apology).
    pub max_friendship: f32,
    pub autonomous: bool,
    pub effect: SocialEffect,
    /// What a care social does for the little one's needs, per hour.
    pub care: [f32; 6],
}

const fn sd(name: &'static str, cat: SocialCat, minutes: f32, social: f32, fun: f32, friendship: f32, romance: f32) -> SocialDef {
    SocialDef {
        name,
        cat,
        minutes,
        social_per_hour: social,
        fun_per_hour: fun,
        friendship,
        romance,
        min_friendship: -100.0,
        min_romance: -100.0,
        max_friendship: 101.0,
        autonomous: false,
        effect: SocialEffect::None,
        care: [0.0; 6],
    }
}

use SocialCat::*;
pub static SOCIALS: [SocialDef; 54] = [
    SocialDef { autonomous: true, ..sd("Chat", Friendly, 25.0, 110.0, 10.0, 8.0, 0.0) },
    SocialDef { effect: SocialEffect::Greet, ..sd("Greet", Friendly, 3.0, 80.0, 5.0, 4.0, 0.0) },
    SocialDef { effect: SocialEffect::HelpHomework, ..sd("Help with Homework", Friendly, 40.0, 60.0, -10.0, 5.0, 0.0) },
    SocialDef { autonomous: true, ..sd("Get to Know", Friendly, 15.0, 100.0, 5.0, 9.0, 0.0) },
    sd("Compliment", Friendly, 8.0, 80.0, 0.0, 7.0, 0.0),
    SocialDef { min_friendship: 15.0, autonomous: true, ..sd("Talk About Hobbies", Friendly, 20.0, 110.0, 15.0, 9.0, 0.0) },
    SocialDef { min_friendship: 30.0, ..sd("Hug", Friendly, 6.0, 140.0, 20.0, 8.0, 0.0) },
    SocialDef { min_friendship: 20.0, ..sd("Dance Together", Friendly, 30.0, 80.0, 80.0, 10.0, 2.0) },
    SocialDef { min_friendship: 40.0, ..sd("High Five", Friendly, 4.0, 90.0, 30.0, 6.0, 0.0) },
    SocialDef { autonomous: true, ..sd("Tell Joke", Funny, 12.0, 90.0, 80.0, 6.0, 0.0) },
    SocialDef { min_friendship: 10.0, ..sd("Do Funny Impression", Funny, 10.0, 80.0, 90.0, 6.0, 0.0) },
    SocialDef { min_friendship: 20.0, ..sd("Tickle", Funny, 6.0, 90.0, 70.0, 5.0, 1.0) },
    SocialDef { min_friendship: 5.0, ..sd("Flirt", Romantic, 8.0, 100.0, 20.0, 2.0, 10.0) },
    SocialDef { min_romance: 10.0, ..sd("Compliment Appearance", Romantic, 6.0, 90.0, 10.0, 3.0, 8.0) },
    SocialDef { min_romance: 25.0, ..sd("Hold Hands", Romantic, 8.0, 120.0, 20.0, 3.0, 9.0) },
    SocialDef { min_romance: 30.0, effect: SocialEffect::Kiss, ..sd("Kiss", Romantic, 6.0, 160.0, 40.0, 4.0, 14.0) },
    SocialDef { min_romance: 50.0, effect: SocialEffect::Kiss, ..sd("Make Out", Romantic, 15.0, 180.0, 60.0, 5.0, 16.0) },
    SocialDef { min_romance: 50.0, effect: SocialEffect::GoSteady, ..sd("Ask to Go Steady", Romantic, 8.0, 120.0, 10.0, 5.0, 10.0) },
    SocialDef { min_romance: 12.0, effect: SocialEffect::AskOnDate, ..sd("Ask on Date", Romantic, 6.0, 80.0, 20.0, 3.0, 4.0) },
    SocialDef { min_romance: 70.0, effect: SocialEffect::Propose, ..sd("Propose Marriage", Romantic, 10.0, 140.0, 20.0, 10.0, 15.0) },
    SocialDef { min_romance: 70.0, effect: SocialEffect::Marry, ..sd("Get Married", Romantic, 30.0, 160.0, 40.0, 15.0, 20.0) },
    SocialDef { min_romance: 60.0, effect: SocialEffect::WooHoo, ..sd("WooHoo", Romantic, 30.0, 200.0, 120.0, 6.0, 15.0) },
    sd("Insult", Mean, 6.0, 40.0, -30.0, -12.0, -5.0),
    sd("Argue", Mean, 15.0, 60.0, -60.0, -15.0, -5.0),
    SocialDef { min_friendship: -100.0, ..sd("Slap", Mean, 4.0, 30.0, -40.0, -20.0, -15.0) },
    SocialDef { effect: SocialEffect::BreakUp, ..sd("Break Up", Mean, 10.0, 40.0, -60.0, -30.0, -60.0) },
    SocialDef { min_friendship: 40.0, effect: SocialEffect::MoveIn, ..sd("Ask to Move In", Special, 10.0, 100.0, 10.0, 5.0, 0.0) },
    SocialDef { effect: SocialEffect::AskToLeave, ..sd("Say Goodbye", Special, 4.0, 40.0, 0.0, 1.0, 0.0) },
    SocialDef { min_romance: 60.0, effect: SocialEffect::TryForBaby, ..sd("Try for Baby", Romantic, 30.0, 200.0, 120.0, 6.0, 15.0) },
    // Hunger, bladder, energy, social, hygiene, fun (per hour, for the little one).
    SocialDef { autonomous: true, care: [500.0, 0.0, 0.0, 60.0, 0.0, 0.0], ..sd("Feed", Care, 15.0, 40.0, 10.0, 5.0, 0.0) },
    SocialDef { autonomous: true, care: [0.0, 700.0, 0.0, 30.0, 400.0, 0.0], ..sd("Change Diaper", Care, 10.0, 30.0, -10.0, 4.0, 0.0) },
    SocialDef { autonomous: true, care: [0.0, 0.0, 0.0, 300.0, 0.0, 200.0], ..sd("Play With", Care, 20.0, 120.0, 120.0, 8.0, 0.0) },
    SocialDef { care: [0.0, 0.0, 0.0, 250.0, 0.0, 120.0], ..sd("Read to", Care, 30.0, 80.0, 60.0, 8.0, 0.0) },
    SocialDef { effect: SocialEffect::PutToBed, care: [0.0, 0.0, 0.0, 60.0, 0.0, 0.0], ..sd("Put to Bed", Care, 6.0, 40.0, 0.0, 3.0, 0.0) },
    SocialDef { effect: SocialEffect::TeachWalk, care: [0.0, 0.0, -20.0, 150.0, 0.0, 60.0], ..sd("Teach to Walk", Care, 30.0, 60.0, 20.0, 6.0, 0.0) },
    SocialDef { effect: SocialEffect::TeachTalk, care: [0.0, 0.0, 0.0, 200.0, 0.0, 40.0], ..sd("Teach to Talk", Care, 30.0, 70.0, 10.0, 6.0, 0.0) },
    // More of the game's socials.
    SocialDef { autonomous: true, min_friendship: 10.0, ..sd("Tell Funny Story", Funny, 12.0, 100.0, 50.0, 8.0, 0.0) },
    SocialDef { min_friendship: 10.0, ..sd("Tell Dramatic Story", Friendly, 15.0, 100.0, 20.0, 7.0, 0.0) },
    sd("Brag", Friendly, 8.0, 70.0, 5.0, 2.0, 0.0),
    SocialDef { autonomous: true, min_friendship: 20.0, ..sd("Goof Around", Funny, 10.0, 90.0, 70.0, 7.0, 0.0) },
    sd("Make Silly Face", Funny, 5.0, 60.0, 40.0, 4.0, 0.0),
    SocialDef { min_friendship: 40.0, ..sd("Cry on Shoulder", Friendly, 10.0, 120.0, -10.0, 10.0, 0.0) },
    SocialDef { effect: SocialEffect::CheerUp, min_friendship: 15.0, ..sd("Cheer Up", Friendly, 8.0, 90.0, 20.0, 8.0, 0.0) },
    SocialDef { effect: SocialEffect::Apologize, max_friendship: 0.0, ..sd("Apologize", Special, 6.0, 40.0, 0.0, 12.0, 0.0) },
    SocialDef { effect: SocialEffect::Fight, max_friendship: -20.0, ..sd("Fight", Mean, 8.0, -20.0, 10.0, -20.0, -10.0) },
    sd("Yell At", Mean, 6.0, -10.0, -5.0, -9.0, -3.0),
    sd("Irritate", Mean, 6.0, -5.0, 5.0, -6.0, 0.0),
    SocialDef { effect: SocialEffect::DeclareNemesis, max_friendship: -60.0, ..sd("Declare Nemesis", Mean, 5.0, -20.0, 0.0, -20.0, -10.0) },
    SocialDef { min_romance: 15.0, ..sd("Embrace", Romantic, 6.0, 120.0, 20.0, 5.0, 8.0) },
    SocialDef { min_romance: 20.0, ..sd("Gaze Into Eyes", Romantic, 6.0, 100.0, 20.0, 3.0, 9.0) },
    SocialDef { effect: SocialEffect::BackRub, min_friendship: 30.0, ..sd("Give Back Rub", Romantic, 12.0, 120.0, 40.0, 6.0, 6.0) },
    SocialDef { min_romance: 50.0, ..sd("Leap Into Arms", Romantic, 5.0, 140.0, 40.0, 4.0, 10.0) },
    SocialDef { effect: SocialEffect::Kiss, min_romance: 60.0, ..sd("Dip Kiss", Romantic, 6.0, 150.0, 30.0, 4.0, 12.0) },
    SocialDef { effect: SocialEffect::PlayCatch, ..sd("Play Catch", Funny, 30.0, 60.0, 120.0, 6.0, 0.0) },
];

/// How far apart the two Sims stand for a social: a few metres to play catch, else close
/// enough to talk.
pub fn apart(def: &SocialDef) -> f32 {
    if def.effect == SocialEffect::PlayCatch { 3.6 } else { 0.9 }
}

pub fn social_index(name: &str) -> Option<usize> {
    SOCIALS.iter().position(|s| s.name == name)
}

/// Whether `actor` can start this social with `target` (shown in the pie menu); `kin`: whether
/// they're family (no romance between relatives).
pub fn available(def: &SocialDef, rel: &Relationship, actor: &Sim, target: &Sim, target_in_household: bool, kin: bool) -> bool {
    if rel.friendship < def.min_friendship || rel.romance < def.min_romance || rel.friendship > def.max_friendship {
        return false;
    }
    if kin && (def.cat == SocialCat::Romantic || matches!(def.effect, SocialEffect::WooHoo | SocialEffect::TryForBaby | SocialEffect::Propose | SocialEffect::Marry | SocialEffect::GoSteady)) {
        return false;
    }
    // Babies and toddlers are looked after rather than chatted with.
    if actor.age.is_little() {
        return false;
    }
    if target.age.is_little() {
        let toddler_only = matches!(def.effect, SocialEffect::TeachWalk | SocialEffect::TeachTalk) || def.name == "Read to";
        return def.cat == SocialCat::Care && !matches!(actor.age, Age::Child) && (!toddler_only || target.age == Age::Toddler);
    }
    if def.cat == SocialCat::Care {
        return false;
    }
    if def.effect == SocialEffect::TryForBaby
        && (actor.female == target.female || !matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married))
    {
        return false;
    }
    let grown = |a: Age| !matches!(a, Age::Child | Age::Teen);
    let adults = grown(actor.age) && grown(target.age);
    // Teens may court other teens, but marriage and woohoo are for adults.
    let teens = actor.age == Age::Teen && target.age == Age::Teen;
    if def.cat == SocialCat::Romantic && !adults && !teens {
        return false;
    }
    if !adults && matches!(def.effect, SocialEffect::Propose | SocialEffect::Marry | SocialEffect::WooHoo) {
        return false;
    }
    match def.effect {
        SocialEffect::GoSteady => rel.status == RelStatus::None || rel.status == RelStatus::Ex,
        SocialEffect::Propose => rel.status == RelStatus::Partner,
        SocialEffect::Marry => rel.status == RelStatus::Engaged,
        SocialEffect::BreakUp => matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married),
        SocialEffect::MoveIn => !target_in_household && (rel.friendship >= 40.0 || rel.status != RelStatus::None),
        SocialEffect::AskToLeave => !target_in_household,
        // (A teen or grown-up helps a child or teen; whether there's homework is for the menu.)
        SocialEffect::HelpHomework => actor.age != Age::Child && matches!(target.age, Age::Child | Age::Teen),
        // (Who's at the door is for the menu: the household's own let callers in.)
        SocialEffect::Greet => target_in_household == false,
        SocialEffect::WooHoo => matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married) || rel.romance >= 80.0,
        _ => true,
    }
}

/// Chance (0..1) that the target goes along with the social.
pub fn acceptance(def: &SocialDef, rel: &Relationship, target: &Sim, target_mood: &Mood, has_other_partner: bool) -> f32 {
    let has = |t: Trait| target.traits.contains(&t);
    let mood = target_mood.level() / 100.0;
    let mut p = match def.cat {
        // Friendly chat is almost always welcome unless the Sim is in a foul mood or dislikes you.
        SocialCat::Friendly | SocialCat::Funny => 0.97 + rel.friendship.min(0.0) / 150.0 + mood.min(0.0) * 0.2,
        SocialCat::Mean => 1.0,
        SocialCat::Special => 0.6 + rel.friendship / 200.0,
        SocialCat::Care => 1.0,
        SocialCat::Romantic => {
            let base = match def.effect {
                SocialEffect::GoSteady => 0.2 + rel.romance / 120.0,
                SocialEffect::Propose => 0.1 + rel.romance / 110.0,
                SocialEffect::Marry | SocialEffect::WooHoo | SocialEffect::TryForBaby => 0.5 + rel.romance / 200.0,
                _ => 0.35 + rel.romance / 100.0 + rel.friendship / 300.0,
            };
            base + mood * 0.15
        }
    };
    if def.cat == SocialCat::Romantic {
        if has(Trait::Flirty) || has(Trait::HopelessRomantic) {
            p += 0.15;
        }
        if has(Trait::Unflirty) {
            p -= 0.3;
        }
        if has(Trait::CommitmentIssues) && matches!(def.effect, SocialEffect::GoSteady | SocialEffect::Propose | SocialEffect::Marry) {
            p -= 0.35;
        }
        if has_other_partner {
            p -= 0.5;
        }
    }
    if def.cat == SocialCat::Funny {
        if has(Trait::GoodSenseOfHumor) {
            p += 0.1;
        }
        if has(Trait::Grumpy) {
            p -= 0.35;
        }
        if has(Trait::NoSenseOfHumor) {
            p -= 0.5;
        }
    }
    if has(Trait::EasilyImpressed) {
        p += 0.1;
    }
    // The evil and the mean-spirited take an insult in their stride.
    if def.cat == SocialCat::Mean && (has(Trait::Evil) || has(Trait::MeanSpirited)) {
        p = p.max(0.9);
    }
    if def.cat == SocialCat::Friendly && has(Trait::Loner) {
        p -= 0.15;
    }
    p.clamp(0.02, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendships_fade_without_contact() {
        let mut app = App::new();
        app.insert_resource(crate::clock::GameClock::default()).init_resource::<crate::family::Genealogy>().add_systems(Update, fade_relationships);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(3);
        let a_sim = crate::sim::random_sim(&mut rng, "Test", Some(true), Age::Adult);
        let b_sim = crate::sim::random_sim(&mut rng, "Test", Some(false), Age::Adult);
        let b = app.world_mut().spawn((b_sim, Relationships::default())).id();
        let a = app.world_mut().spawn((a_sim, crate::sim::HouseholdMember, Relationships::default())).id();
        let start = 1440.0;
        for (me, other, status) in [(a, b, RelStatus::None), (b, a, RelStatus::None)] {
            let mut rels = app.world_mut().get_mut::<Relationships>(me).unwrap();
            *rels.entry(other) = Relationship { friendship: 50.0, romance: 20.0, status, kissed: false, last: start };
        }
        let mut run_hours = |app: &mut App, from: f64, hours: usize| {
            for h in 0..hours {
                app.world_mut().resource_mut::<crate::clock::GameClock>().minutes = from + h as f64 * 60.0;
                app.update();
            }
        };
        // Two days apart: as they were.
        run_hours(&mut app, start + 60.0, 48);
        assert_eq!(app.world().get::<Relationships>(a).unwrap().friendship(b), 50.0);
        // Five days apart: a day and a half of fading, both ways (friendship a point a day,
        // romance too).
        run_hours(&mut app, start + 3.5 * 1440.0, 48);
        for (me, other) in [(a, b), (b, a)] {
            let r = app.world().get::<Relationships>(me).unwrap().get(other);
            assert!(r.friendship < 50.0 && r.friendship > 47.5, "{}", r.friendship);
            assert!(r.romance < 20.0, "{}", r.romance);
        }
        // Spouses stay close.
        app.world_mut().get_mut::<Relationships>(a).unwrap().entry(b).status = RelStatus::Married;
        app.world_mut().get_mut::<Relationships>(a).unwrap().entry(b).friendship = 41.0;
        run_hours(&mut app, start + 10.0 * 1440.0, 24 * 10);
        assert_eq!(app.world().get::<Relationships>(a).unwrap().friendship(b), 40.0);
    }
}
