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
}

impl Relationship {
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
}

impl SocialCat {
    pub const ALL: [SocialCat; 5] = [SocialCat::Friendly, SocialCat::Funny, SocialCat::Romantic, SocialCat::Mean, SocialCat::Special];
    pub fn name(self) -> &'static str {
        match self {
            SocialCat::Friendly => "Friendly",
            SocialCat::Funny => "Funny",
            SocialCat::Romantic => "Romantic",
            SocialCat::Mean => "Mean",
            SocialCat::Special => "Special",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SocialEffect {
    None,
    Kiss,
    GoSteady,
    Propose,
    Marry,
    BreakUp,
    MoveIn,
    AskToLeave,
    WooHoo,
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
    pub autonomous: bool,
    pub effect: SocialEffect,
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
        autonomous: false,
        effect: SocialEffect::None,
    }
}

use SocialCat::*;
pub static SOCIALS: [SocialDef; 25] = [
    SocialDef { autonomous: true, ..sd("Chat", Friendly, 25.0, 110.0, 10.0, 8.0, 0.0) },
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
    SocialDef { min_romance: 70.0, effect: SocialEffect::Propose, ..sd("Propose Marriage", Romantic, 10.0, 140.0, 20.0, 10.0, 15.0) },
    SocialDef { min_romance: 70.0, effect: SocialEffect::Marry, ..sd("Get Married", Romantic, 30.0, 160.0, 40.0, 15.0, 20.0) },
    SocialDef { min_romance: 60.0, effect: SocialEffect::WooHoo, ..sd("WooHoo", Romantic, 30.0, 200.0, 120.0, 6.0, 15.0) },
    sd("Insult", Mean, 6.0, 40.0, -30.0, -12.0, -5.0),
    sd("Argue", Mean, 15.0, 60.0, -60.0, -15.0, -5.0),
    SocialDef { min_friendship: -100.0, ..sd("Slap", Mean, 4.0, 30.0, -40.0, -20.0, -15.0) },
    SocialDef { effect: SocialEffect::BreakUp, ..sd("Break Up", Mean, 10.0, 40.0, -60.0, -30.0, -60.0) },
    SocialDef { min_friendship: 40.0, effect: SocialEffect::MoveIn, ..sd("Ask to Move In", Special, 10.0, 100.0, 10.0, 5.0, 0.0) },
    SocialDef { effect: SocialEffect::AskToLeave, ..sd("Say Goodbye", Special, 4.0, 40.0, 0.0, 1.0, 0.0) },
];

pub fn social_index(name: &str) -> Option<usize> {
    SOCIALS.iter().position(|s| s.name == name)
}

/// Whether `actor` can start this social with `target` (shown in the pie menu).
pub fn available(def: &SocialDef, rel: &Relationship, actor: &Sim, target: &Sim, target_in_household: bool) -> bool {
    if rel.friendship < def.min_friendship || rel.romance < def.min_romance {
        return false;
    }
    let adults = actor.age != Age::Child && target.age != Age::Child;
    if def.cat == SocialCat::Romantic && !adults {
        return false;
    }
    match def.effect {
        SocialEffect::GoSteady => rel.status == RelStatus::None || rel.status == RelStatus::Ex,
        SocialEffect::Propose => rel.status == RelStatus::Partner,
        SocialEffect::Marry => rel.status == RelStatus::Engaged,
        SocialEffect::BreakUp => matches!(rel.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married),
        SocialEffect::MoveIn => !target_in_household && (rel.friendship >= 40.0 || rel.status != RelStatus::None),
        SocialEffect::AskToLeave => !target_in_household,
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
        SocialCat::Romantic => {
            let base = match def.effect {
                SocialEffect::GoSteady => 0.2 + rel.romance / 120.0,
                SocialEffect::Propose => 0.1 + rel.romance / 110.0,
                SocialEffect::Marry | SocialEffect::WooHoo => 0.5 + rel.romance / 200.0,
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
    }
    if def.cat == SocialCat::Friendly && has(Trait::Loner) {
        p -= 0.15;
    }
    p.clamp(0.02, 1.0)
}
