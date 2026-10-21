//! The game's own names for interactions, where ours (kept as the interactions' keys all
//! through the code) differ: each from its interaction's string (`<class>:InteractionName`,
//! e.g. `Gameplay/Objects/Plumbing/Toilet/UseToilet`), shown on the pie menus and in the queue.

/// Ours, and the game's.
const NAMES: &[(&str, &str)] = &[
    // TrashcanIndoor.EmptyTrash
    ("Empty Trash", "Take Out Trash"),
    // Bar.MakeDrink
    ("Make a Drink", "Have Drink"),
    // Computer.SolveHardProblems, Computer.WorkOnJobRelatedIssues
    ("Solve Hard Problems", "Solve the Unsolvable"),
    ("Work on Job-Related Issues", "Work On Current Job Issues"),
    // Toilet.UseToilet
    ("Use Toilet", "Use"),
    // Shower.TakeShower
    ("Take Shower", "Shower"),
    // Sprinkler.PlayWithSprinkler
    ("Play in Sprinkler", "Play with Sprinkler"),
    // Candle.ExtinguishCandleInteraction
    ("Blow Out", "Extinguish Candle"),
    // Dresser.ChangeClothes
    ("Change Into", "Change Outfit"),
    // Dishwasher.UseDishwasher
    ("Load Dishes", "Use Dishwasher"),
    // Dryer and Clothesline: DryClothing, GetCleanLaundry
    ("Load the Dryer", "Dry Clothes"),
    ("Empty Dryer", "Get Clean Laundry"),
    ("Hang Clothes", "Dry Clothes"),
    ("Take Down Clothes", "Get Clean Laundry"),
    // Fireplace.WarmSelf
    ("Warm Hands", "Warm Self"),
    // Snowman.DestroySnowman, SnowAngel.DestroySnowAngel
    ("Destroy Snowman", "Destroy"),
    ("Destroy Snow Angel", "Destroy"),
    // LeafPile.PlayIn
    ("Play in Leaves", "Play In"),
    // Sandbox.PlayOn
    ("Play in Sandbox", "Play in Sand"),
    // DollHouse.DisposeDollHouse
    ("Throw Away", "Dispose"),
    // HotTubBase.GetIn
    ("Relax in Hot Tub", "Get In"),
    // AlarmClock.SetAlarmClock
    ("Set Alarm", "Set Alarm Clock"),
    // BuffExhausted.PassOut
    ("Passed Out", "Pass Out"),
    // (Their seated parts: Newspaper.ReadNewspaper, TV.WatchTV.)
    ("Read the Paper", "Read"),
    ("Watch the TV", "Watch TV"),
];

/// How an interaction's shown: the game's name for it, else as it is (a submenu's mark kept).
pub fn shown(label: &str) -> std::borrow::Cow<'_, str> {
    let (base, more) = match label.strip_suffix(" ›") {
        Some(b) => (b, " ›"),
        None => (label, ""),
    };
    match NAMES.iter().find(|(ours, _)| *ours == base) {
        Some((_, game)) => format!("{game}{more}").into(),
        None => label.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_games_names() {
        assert_eq!(shown("Empty Trash"), "Take Out Trash");
        assert_eq!(shown("Change Into ›"), "Change Outfit ›");
        assert_eq!(shown("Watch TV"), "Watch TV");
        assert_eq!(shown("Swim"), "Swim");
    }

    #[test]
    fn each_once() {
        for (i, (a, _)) in NAMES.iter().enumerate() {
            assert!(!NAMES[i + 1..].iter().any(|(b, _)| b == a), "{a} twice");
        }
    }
}
