//! Guests' manners, as the game's `VisitSituation` checks them: what a visitor to the
//! household's home does there on their own depends on how well they know the household (the
//! most friendly of them: `LTRData.VisitorPriveleges`). Each interaction a guest might find
//! forward carries a value (its tuning's `InappropriateForVisitor` check), and a guest leaves
//! alone those at or past their privilege. A stranger browses the web and uses the bathroom but
//! won't cook, clean or use the computer otherwise; a friend cooks and plays computer games but
//! won't work out, bathe or sleep; a best friend or a partner bathes and naps but won't
//! overclock the computer; a spouse or fiancé does as they like, all but sabotage. Someone
//! disliked keeps to what has no such value at all.

use bevy::prelude::*;

use crate::interact::{InteractionDef, ObjectKind, Special};
use crate::sim::Age;

/// A guest staying the night (asked to: "Ask NPC To Stay Over"): at home as a spouse would be,
/// and not sent home by the hour or their needs, till the morning.
#[derive(Component)]
pub struct StayingOver;

/// What a guest staying over may do (the game's `kVisitorPrivilegesForStayOver`).
pub const STAY_OVER_PRIVILEGE: f32 = 4.0;
/// The hours a guest can be asked to stay (the game's `kAskToStayOverTimeStart`, `End`: 4 PM to
/// 4 AM), how well liked they must be, and the hour they leave in the morning.
const ASK_FROM: f32 = 16.0;
const ASK_UNTIL: f32 = 4.0;
const STAY_LIKING: f32 = 40.0;
pub const STAY_UNTIL: f64 = 10.0;

/// The words (the game's `SocialCallback:NPCStayOver`, `NPCStayOverRejected`).
pub const STAYING: &str = "Thanks for inviting me over!  I'm really looking forward to spending the night at your place. It'll be fun!";
pub const NOT_STAYING: &str = "I don't know you well enough to stay over right now...";

/// Whether a guest may be asked to stay over: liked well enough, and of an age with the one
/// asking (children with children, teens with teens, grown-ups with grown-ups).
pub fn may_stay_over(liking: f32, a: Age, b: Age) -> bool {
    let band = |x: Age| match x {
        Age::Child => 0,
        Age::Teen => 1,
        _ => 2,
    };
    liking >= STAY_LIKING && !a.is_little() && !b.is_little() && a != Age::Baby && band(a) == band(b)
}

/// Whether it's the hour to ask someone to stay over.
pub fn stay_over_hour(hour: f32) -> bool {
    !(ASK_UNTIL..ASK_FROM).contains(&hour)
}

/// When a guest asked to stay now goes home: the next morning.
pub fn stay_until(now: f64) -> f64 {
    let day = (now / 1440.0).floor();
    let hour = (now - day * 1440.0) / 60.0;
    (day + if hour >= STAY_UNTIL { 1.0 } else { 0.0 }) * 1440.0 + STAY_UNTIL * 60.0
}

/// What a guest may do, by their relationship with the household (`socializing.xml`).
pub fn privilege(state: &str) -> f32 {
    match state {
        "Friend" | "Good Friend" | "Old Friend" | "Romantic Interest" | "Ex" | "Ex Spouse" => 2.0,
        "Best Friend" | "Best Friends Forever" | "Partner" => 3.0,
        "Fiancee" | "Spouse" => 4.0,
        "Disliked" | "Enemy" | "Old Enemies" => -1.0,
        // (Strangers, acquaintances, distant friends.)
        _ => 1.0,
    }
}

/// How forward an interaction is for a guest (the game's `InappropriateForVisitor` values),
/// if it's anything they'd think twice about.
pub fn check(kind: ObjectKind, d: &InteractionDef) -> Option<f32> {
    use ObjectKind as K;
    let v = match (kind, d.special) {
        (K::Computer, Special::Sabotage) => 10.0,
        (K::Computer, Special::Overclock) => 3.5,
        (K::Computer, Special::Hack) => 1.0,
        (K::Computer, _) if d.name == "Browse Web" => 0.5,
        (K::Computer, _) => 1.5,
        (K::Tv | K::Stereo, Special::Sabotage) => 10.0,
        (K::Tv, _) if d.name == "Work Out" => 2.0,
        (K::Stereo, Special::CardioWorkOut | Special::StrengthWorkOut) => 2.0,
        (K::Treadmill | K::WorkoutBench, _) => 2.0,
        (K::Shower | K::Bathtub | K::ShowerTub, _) => 2.5,
        (K::Toilet | K::Sink, Special::Scrub) => 2.5,
        (K::Sink, Special::SpongeBath) => 2.5,
        (K::BedSingle | K::BedDouble, Special::MakeBed) => 2.9,
        (K::BedSingle | K::BedDouble, _) if matches!(d.name, "Sleep" | "Nap") => 2.9,
        (K::Sofa, _) if d.name == "Nap" => 2.0,
        (K::Fridge, Special::Cook | Special::CleanOutFridge) => 2.5,
        (K::Stove, Special::ServeMeal) => 2.5,
        (K::Meal, Special::PutAway) => 1.5,
        (K::Newspaper, Special::ClipCoupon) => 1.1,
        (K::HotBeverage, _) => 1.5,
        (K::PoolLadder | K::DivingBoard, _) => 1.5,
        (K::TrashCan | K::TrashCompactor, Special::EmptyTrash) => 1.5,
        (K::FishBowl, Special::CleanBowl) => 1.5,
        (K::PottyChair, Special::EmptyPotty) => 3.0,
        _ => return None,
    };
    Some(v)
}

/// Whether a guest of this privilege would do it on their own.
pub fn minds(privilege: f32, check: Option<f32>) -> bool {
    check.is_some_and(|v| v >= privilege)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interact::interactions_for;

    fn value(kind: ObjectKind, name: &str) -> Option<f32> {
        interactions_for(kind).iter().find(|d| d.name == name).and_then(|d| check(kind, d))
    }

    #[test]
    fn staying_over() {
        assert!(may_stay_over(45.0, Age::Adult, Age::YoungAdult));
        assert!(!may_stay_over(30.0, Age::Adult, Age::Adult));
        assert!(!may_stay_over(80.0, Age::Teen, Age::Adult));
        assert!(may_stay_over(80.0, Age::Child, Age::Child));
        assert!(stay_over_hour(20.0) && stay_over_hour(1.0) && !stay_over_hour(12.0));
        // (Asked at 9 PM: home by ten the next morning; at 1 AM, the same morning.)
        assert_eq!(stay_until(1440.0 + 21.0 * 60.0), 2.0 * 1440.0 + 600.0);
        assert_eq!(stay_until(2.0 * 1440.0 + 60.0), 2.0 * 1440.0 + 600.0);
    }

    #[test]
    fn manners_by_how_well_they_know_the_household() {
        let stranger = privilege("Acquaintance");
        let friend = privilege("Friend");
        let best = privilege("Best Friend");
        let spouse = privilege("Spouse");
        // (A stranger browses the web, but won't play games on the computer.)
        assert!(!minds(stranger, value(ObjectKind::Computer, "Browse Web")));
        assert!(minds(stranger, value(ObjectKind::Computer, "Play Computer Games")));
        assert!(!minds(friend, value(ObjectKind::Computer, "Play Computer Games")));
        // (Bathing and sleeping: best friends on.)
        assert!(minds(friend, value(ObjectKind::Shower, "Take Shower")));
        assert!(!minds(best, value(ObjectKind::Shower, "Take Shower")));
        assert!(minds(friend, value(ObjectKind::BedDouble, "Sleep")));
        assert!(!minds(best, value(ObjectKind::BedDouble, "Sleep")));
        // (A spouse does as they like, all but sabotage.)
        assert!(!minds(spouse, value(ObjectKind::Computer, "Overclock")));
        assert!(minds(spouse, value(ObjectKind::Tv, "Sabotage")));
        // (Anyone uses the toilet; the disliked keep to that sort of thing.)
        assert_eq!(value(ObjectKind::Toilet, "Use Toilet"), None);
        assert!(minds(privilege("Disliked"), value(ObjectKind::Computer, "Browse Web")));
        assert!(!minds(privilege("Disliked"), value(ObjectKind::Toilet, "Use Toilet")));
    }
}
