//! Ingredients, as the game's `Recipe.UseUpIngredientsForOneServing`: a recipe's ingredients
//! (up to three, of a kind or a kind of thing: a Tomato, or any Fish) come from the cook's own
//! inventory, then the household's fridge, and whatever's missing is bought there and then, at
//! its price and half again (`kFridgeRestockingPriceMarkupPercentage`), of normal quality. One
//! that can't be bought (a Lifefruit) must be had, or the recipe can't be made. What they're
//! made of counts towards how a dish comes out: a perfect tomato's thirty of the game's food
//! points, a horrifying one fifty against (`Ingredient.QualityToIngredientPointMap`). A
//! household starts with three of each of the ingredients the simplest recipes take in its
//! fridge (`Household.GiveIngredients`).

use bevy::prelude::*;
use s3bake::GameDataBaked;
use s3bake::gamedata::IngredientInfo;
use serde::{Deserialize, Serialize};

use crate::inventory::{Inventory, ItemKind, Stack};

/// The price of what's missing, and half again.
const MARKUP: i64 = 50;
/// The fridge's start: three of each.
const STARTING_SERVINGS: u32 = 3;
/// What the grocery's are like (`kQualityOfPurchesdIngredients`: Nice), as a gardening tier.
pub const STORE_TIER: u8 = 4;
/// What it asks for those grown (`CostMultiplierForPlantables` at Nice).
const STORE_GROWN: f32 = 1.5;
/// The normal tier.
pub const NORMAL_TIER: u8 = 3;

/// The household's fridge: the ingredients put in it (none, not yet given its first).
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct Fridge(pub Option<Vec<Stack>>);

impl Fridge {
    pub fn stock(&self) -> &[Stack] {
        self.0.as_deref().unwrap_or(&[])
    }
}

/// Whether `key` is a `kind` (itself, or under it: a Tomato is a Vegetable is an Ingredient).
pub fn is_of_kind(data: &GameDataBaked, key: &str, kind: &str) -> bool {
    if key == kind || kind == "ROOT" {
        return true;
    }
    let mut at = key;
    for _ in 0..8 {
        let Some(i) = data.ingredients.iter().find(|i| i.key == at) else { return false };
        if i.parent == kind {
            return true;
        }
        if i.parent.is_empty() || i.parent == "ROOT" {
            return false;
        }
        at = &i.parent;
    }
    false
}

/// The cheapest of a kind the grocery sells (the game's `GetCheapestIngredientOfAbstractType`).
pub fn cheapest<'a>(data: &'a GameDataBaked, kind: &str) -> Option<&'a IngredientInfo> {
    data.ingredients.iter().filter(|i| !i.kind && i.can_buy && i.price > 0 && is_of_kind(data, &i.key, kind)).min_by_key(|i| i.price)
}

/// What the grocery asks for one (`StoreItem.GetPrice`): its price, half again for produce at
/// the quality it sells.
pub fn store_price(i: &IngredientInfo) -> i64 {
    if i.grown { (i.price as f32 * STORE_GROWN).ceil() as i64 } else { i.price as i64 }
}

/// An ingredient's food points by its quality tier (the gardening qualities).
pub fn points(tier: u8) -> f32 {
    [-50.0, -25.0, -5.0, 0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0][tier.min(9) as usize]
}

/// Whether a stack is an ingredient (produce, groceries, fish).
fn usable(s: &Stack) -> bool {
    s.count > 0 && matches!(s.kind, ItemKind::Produce | ItemKind::Fish)
}

/// Where an ingredient came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Cook,
    Fridge,
}

/// How a recipe's ingredients are come by: those had (where, which, its quality), what the
/// rest cost, and the food points they bring; none, if one can't be had.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub had: Vec<(Source, String, u8)>,
    pub bought: Vec<String>,
    pub cost: i64,
    pub points: f32,
}

/// A recipe's ingredients from the cook's inventory and the fridge, the rest bought.
pub fn plan(data: &GameDataBaked, wants: &[String], cook: Option<&Inventory>, fridge: &[Stack]) -> Option<Plan> {
    // (Counted down as they're taken: a recipe of two eggs takes two.)
    let mut left: Vec<(Source, usize, u32)> = Vec::new();
    let have = |src: Source, i: usize, s: &Stack, left: &[(Source, usize, u32)]| s.count - left.iter().filter(|l| l.0 == src && l.1 == i).map(|l| l.2).sum::<u32>();
    let mut out = Plan::default();
    let mut sum = 0i64;
    for want in wants {
        let mine = cook.and_then(|inv| inv.0.iter().enumerate().find(|(i, s)| usable(s) && is_of_kind(data, &s.key, want) && have(Source::Cook, *i, s, &left) > 0).map(|(i, s)| (Source::Cook, i, s)));
        let theirs = || fridge.iter().enumerate().find(|(i, s)| usable(s) && is_of_kind(data, &s.key, want) && have(Source::Fridge, *i, s, &left) > 0).map(|(i, s)| (Source::Fridge, i, s));
        match mine.or_else(theirs) {
            Some((src, i, s)) => {
                left.push((src, i, 1));
                out.had.push((src, s.key.clone(), s.quality));
                out.points += points(s.quality);
            }
            None => {
                let i = cheapest(data, want)?;
                sum += i.price as i64;
                out.bought.push(i.key.clone());
            }
        }
    }
    out.cost = sum + (sum * MARKUP + 99) / 100;
    Some(out)
}

/// Takes what a plan had from the cook's inventory and the fridge.
pub fn use_up(plan: &Plan, cook: Option<&mut Inventory>, fridge: &mut Fridge) {
    let mut cook = cook;
    for (src, key, quality) in &plan.had {
        let stacks = match src {
            Source::Cook => match cook.as_deref_mut() {
                Some(c) => &mut c.0,
                None => continue,
            },
            Source::Fridge => fridge.0.get_or_insert_with(Vec::new),
        };
        if let Some(i) = stacks.iter().position(|s| usable(s) && s.key == *key && s.quality == *quality) {
            let s = &mut stacks[i];
            let each = s.each();
            s.count -= 1;
            s.worth -= each;
            if s.count == 0 {
                stacks.remove(i);
            }
        }
    }
}

/// The fridge's first stock: three normal ones of each ingredient the simplest recipes take.
pub fn starting_stock(data: &GameDataBaked) -> Vec<Stack> {
    let mut out: Vec<Stack> = Vec::new();
    for r in data.recipes.iter().filter(|r| r.level == 0 && !r.snack) {
        for key in &r.ingredients {
            let Some(i) = data.ingredients.iter().find(|i| i.key == *key && !i.kind) else { continue };
            if out.iter().any(|s| s.key == i.key) {
                continue;
            }
            let kind = if is_of_kind(data, &i.key, "Fish") { ItemKind::Fish } else { ItemKind::Produce };
            let each = i.price as i64;
            out.push(Stack { kind, key: i.key.clone(), name: i.name.clone(), quality: NORMAL_TIER, count: STARTING_SERVINGS, worth: each * STARTING_SERVINGS as i64 });
        }
    }
    out
}

/// A household's fridge, stocked to start with once the game's data is in.
pub fn stock_fridge(mut fridge: ResMut<Fridge>, ui: Option<Res<crate::icons::GameUi>>) {
    if fridge.0.is_some() {
        return;
    }
    let Some(ui) = ui else { return };
    let stock = starting_stock(&ui.data);
    info!("the fridge stocked: {}", stock.iter().map(|s| format!("{} × {}", s.count, s.name)).collect::<Vec<_>>().join(", "));
    fridge.0 = Some(stock);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ing(key: &str, parent: &str, price: i32, grown: bool) -> IngredientInfo {
        IngredientInfo { key: key.into(), name: key.into(), parent: parent.into(), kind: price == 0 && parent != "Fish", price, can_buy: price > 0, grown, tab: String::new() }
    }

    fn data() -> GameDataBaked {
        GameDataBaked {
            ingredients: vec![
                ing("Ingredient", "ROOT", 0, false),
                ing("Vegetable", "Ingredient", 0, false),
                ing("Meat", "Ingredient", 0, false),
                ing("Fish", "Meat", 0, false),
                ing("Tomato", "Vegetable", 3, true),
                ing("Onion", "Vegetable", 7, true),
                ing("Egg", "Meat", 7, true),
                ing("Anchovy", "Fish", 5, false),
                IngredientInfo { can_buy: false, ..ing("Lifefruit", "Ingredient", 25, true) },
            ],
            ..Default::default()
        }
    }

    fn stack(key: &str, quality: u8, count: u32) -> Stack {
        Stack { kind: ItemKind::Produce, key: key.into(), name: key.into(), quality, count, worth: 3 * count as i64 }
    }

    #[test]
    fn kinds_and_prices() {
        let d = data();
        assert!(is_of_kind(&d, "Tomato", "Vegetable") && is_of_kind(&d, "Anchovy", "Meat") && !is_of_kind(&d, "Tomato", "Fish"));
        assert_eq!(cheapest(&d, "Vegetable").map(|i| i.key.as_str()), Some("Tomato"));
        assert_eq!(cheapest(&d, "Ingredient").map(|i| i.key.as_str()), Some("Tomato"));
        assert_eq!(store_price(&d.ingredients[4]), 5);
        assert_eq!(store_price(&d.ingredients[7]), 5);
        assert_eq!(points(9), 30.0);
    }

    #[test]
    fn recipes_take_what_is_had_and_buy_the_rest() {
        let d = data();
        let wants = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // (Spaghetti from an empty fridge: a tomato, §3 and half again, §5.)
        let p = plan(&d, &wants(&["Tomato"]), None, &[]).unwrap();
        assert_eq!((p.cost, p.bought.len(), p.points), (5, 1, 0.0));
        // (A perfect tomato of the cook's own first; the fridge's eggs, two of them.)
        let mut mine = Inventory::default();
        mine.0.push(stack("Tomato", 9, 1));
        let fridge = vec![stack("Egg", 3, 2), stack("Tomato", 3, 4)];
        let p = plan(&d, &wants(&["Tomato", "Egg", "Egg"]), Some(&mine), &fridge).unwrap();
        assert_eq!(p.cost, 0);
        assert_eq!(p.had, vec![(Source::Cook, "Tomato".into(), 9), (Source::Fridge, "Egg".into(), 3), (Source::Fridge, "Egg".into(), 3)]);
        assert_eq!(p.points, 30.0);
        // (A third egg wanted: bought.)
        let p = plan(&d, &wants(&["Egg", "Egg", "Egg"]), None, &fridge).unwrap();
        assert_eq!((p.bought.len(), p.cost), (1, 11));
        // (What can't be bought must be had.)
        assert!(plan(&d, &wants(&["Lifefruit"]), None, &fridge).is_none());
        // Used up: gone from where they were.
        let p = plan(&d, &wants(&["Tomato", "Egg"]), Some(&mine), &fridge).unwrap();
        let mut f = Fridge(Some(fridge.clone()));
        use_up(&p, Some(&mut mine), &mut f);
        assert!(mine.0.is_empty());
        assert_eq!(f.stock().iter().map(|s| (s.key.as_str(), s.count)).collect::<Vec<_>>(), vec![("Egg", 1), ("Tomato", 4)]);
    }
}
