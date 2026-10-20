//! Guests' manners, as the game's `VisitSituation` checks them: what a visitor to the
//! household's home does there on their own depends on how well they know the household (the
//! most friendly of them: `LTRData.VisitorPriveleges`). Each interaction a guest might find
//! forward carries a value (its tuning's `InappropriateForVisitor` check), and a guest leaves
//! alone those at or past their privilege. A stranger browses the web and uses the bathroom but
//! won't cook, clean or use the computer otherwise; a friend cooks and plays computer games but
//! won't work out, bathe or sleep; a best friend or a partner bathes and naps but won't
//! overclock the computer; a spouse or fiancé does as they like, all but sabotage. Someone
//! disliked keeps to what has no such value at all.

use crate::interact::{InteractionDef, ObjectKind, Special};

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
