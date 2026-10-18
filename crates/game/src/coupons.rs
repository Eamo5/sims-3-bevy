//! A Frugal Sim's coupons, as the game's `ClipCoupon` and `ShoppingCoupon`: each day's paper has
//! one to clip ("Clip Coupon", the Frugal's alone, while the town has a grocery or a bookstore),
//! for something one of those sells, at a discount by its price (the class tuning's: 40% off up
//! to §150, 30% up to §600, 25% up to §3,000, 20% beyond; one in ten a Blowout Special at 75, 70,
//! 60 or 50% off), good to the end of the third day. It's kept in their inventory; bought at that
//! store, one of the item comes at the coupon's price, and the coupon's used up (`shop`).

use std::collections::HashSet;

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::inventory::{Inventory, ItemKind};
use crate::life::Trait;
use crate::sim::Sim;

pub struct CouponsPlugin;

impl Plugin for CouponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Coupons>().add_systems(Update, (stores, clip, expire).run_if(in_state(PlayMode::Live)));
    }
}

/// Which store a coupon's for.
pub const GROCERY: &str = "grocery";
pub const BOOKSTORE: &str = "bookstore";

/// The class tuning's (`Coupon`): the price tiers, their discounts and the Blowout Special's
/// (per cent off), its chance, and the days a coupon's good for.
const TIERS: [i64; 3] = [150, 600, 3000];
const DISCOUNTS: [i64; 4] = [40, 30, 25, 20];
const BLOWOUT: [i64; 4] = [75, 70, 60, 50];
const BLOWOUT_CHANCE: f64 = 0.1;
const DAYS: f64 = 3.0;

/// The papers whose coupon's been clipped, and which stores the town has (grocery, bookstore).
#[derive(Resource, Default)]
pub struct Coupons {
    pub clipped: HashSet<Entity>,
    stores: Option<(bool, bool)>,
}

impl Coupons {
    /// Whether "Clip Coupon" is a Sim's at this paper.
    pub fn offers(&self, sim: &Sim, paper: Entity) -> bool {
        sim.traits.contains(&Trait::Frugal) && !sim.age.is_little() && !self.clipped.contains(&paper) && self.stores.is_some_and(|(g, b)| g || b)
    }
}

/// A coupon to be clipped from this paper (its action done).
#[derive(Component)]
pub struct ClipNow(pub Entity);

/// The discount for an item at this price (per cent off).
pub fn discount(price: i64, blowout: bool) -> i64 {
    let tier = TIERS.iter().position(|t| price <= *t).unwrap_or(TIERS.len());
    if blowout { BLOWOUT[tier] } else { DISCOUNTS[tier] }
}

/// What an item comes to with that discount (rounded up, as the game's `GetPrice`).
pub fn coupon_price(price: i64, discount: i64) -> i64 {
    (price as f64 * (1.0 - discount as f64 / 100.0)).ceil() as i64
}

/// A coupon's inventory key: its store, its item and when it runs out (game minutes).
fn key(store: &str, item: &str, until: f64) -> String {
    format!("{store}|{item}|{}", until as i64)
}

fn parse(key: &str) -> Option<(&str, &str, f64)> {
    let mut it = key.splitn(3, '|');
    let (store, item, until) = (it.next()?, it.next()?, it.next()?.parse::<f64>().ok()?);
    Some((store, item, until))
}

/// The coupon a shopper holds for an item of a store, if any: its place in their inventory, and
/// what the item comes to with it.
pub fn held(inv: &Inventory, store: &str, item: &str) -> Option<(usize, i64)> {
    inv.0.iter().position(|s| s.kind == ItemKind::Coupon && s.count > 0 && parse(&s.key).is_some_and(|(st, it, _)| st == store && it == item)).map(|i| (i, inv.0[i].each()))
}

/// Which stores the town has, as the town changes.
fn stores(mut coupons: ResMut<Coupons>, world: Res<crate::loading::CurrentWorld>) {
    if coupons.stores.is_some() && !world.is_changed() {
        return;
    }
    let has = |t: &str| (0..world.data.lots.len()).any(|l| crate::opportunities::lot_types(&world.data, l).contains(&t));
    coupons.stores = Some((has("Grocery"), has("Bookstore")));
}

/// Clipped: something the store sells, at its discount, into the Sim's inventory; and the paper
/// has no coupon now.
fn clip(mut commands: Commands, mut coupons: ResMut<Coupons>, clock: Res<crate::clock::GameClock>, sims: Query<(Entity, &ClipNow, &Sim)>, ui: Option<Res<crate::icons::GameUi>>, mut notes: ResMut<crate::interact::Notifications>) {
    let Some(ui) = ui else { return };
    let mut rng = rand::rng();
    for (me, now, sim) in &sims {
        commands.entity(me).remove::<ClipNow>();
        if !coupons.offers(sim, now.0) {
            continue;
        }
        let (grocery, bookstore) = coupons.stores.unwrap_or_default();
        let d = &ui.data;
        let mut stock: Vec<(&str, String, String, i64)> = Vec::new();
        if grocery {
            stock.extend(d.ingredients.iter().filter(|i| !i.kind && i.can_buy && i.price > 0).map(|i| (GROCERY, i.key.clone(), i.name.clone(), crate::ingredients::store_price(i))));
        }
        if bookstore {
            stock.extend(d.novels.iter().filter(|b| b.in_store && b.price > 0).map(|b| (BOOKSTORE, b.key.clone(), b.name.clone(), b.price)));
            stock.extend(d.skill_books.iter().filter(|b| b.price > 0).map(|b| (BOOKSTORE, b.key.clone(), b.name.clone(), b.price)));
            stock.extend(d.recipes.iter().filter(|r| r.book_price > 0).map(|r| (BOOKSTORE, r.key.clone(), r.name.clone(), r.book_price as i64)));
        }
        // (A store first, as the game picks one, then something of it.)
        let kinds: Vec<&str> = [(grocery, GROCERY), (bookstore, BOOKSTORE)].into_iter().filter(|(o, s)| *o && stock.iter().any(|x| x.0 == *s)).map(|x| x.1).collect();
        let Some(&store) = kinds.choose(&mut rng) else { continue };
        let of: Vec<&(&str, String, String, i64)> = stock.iter().filter(|x| x.0 == store).collect();
        let Some(&&(_, ref item, ref name, price)) = of.choose(&mut rng) else { continue };
        let blowout = rng.random_bool(BLOWOUT_CHANCE);
        let off = discount(price, blowout);
        let cost = coupon_price(price, off);
        // (Good to the end of the third day, the game's `kNumDaysToExpire`.)
        let until = (clock.minutes / 1440.0).floor() * 1440.0 + DAYS * 1440.0;
        crate::inventory::give(&mut commands, me, ItemKind::Coupon, key(store, item, until), format!("Coupon for {name}"), off as u8, cost, 1);
        coupons.clipped.insert(now.0);
        let shop = if store == GROCERY { "grocery store" } else { "bookstore" };
        info!("{} clipped a coupon: {name}, §{cost} ({off}% off)", sim.first);
        notes.push(format!("{}Coupon for {name} at any {shop} for §{cost} ({off}% off!). Expires in 3 days.", if blowout { "Blowout Special\n" } else { "" }));
    }
    coupons.clipped.retain(|p| commands.get_entity(*p).is_ok());
}

/// Coupons past their day, gone (the game's `OnExpired`).
fn expire(clock: Res<crate::clock::GameClock>, mut last: Local<f64>, mut sims: Query<&mut Inventory>) {
    if clock.minutes - *last < 60.0 {
        return;
    }
    *last = clock.minutes;
    for mut inv in &mut sims {
        if inv.0.iter().any(|s| s.kind == ItemKind::Coupon && parse(&s.key).is_some_and(|c| c.2 <= clock.minutes)) {
            inv.0.retain(|s| !(s.kind == ItemKind::Coupon && parse(&s.key).is_some_and(|c| c.2 <= clock.minutes)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupons_are_dearer_items_smaller_discounts() {
        assert_eq!(discount(100, false), 40);
        assert_eq!(discount(150, false), 40);
        assert_eq!(discount(151, false), 30);
        assert_eq!(discount(3000, false), 25);
        assert_eq!(discount(5000, false), 20);
        assert_eq!(discount(100, true), 75);
        assert_eq!(coupon_price(100, 40), 60);
        assert_eq!(coupon_price(7, 40), 5, "rounded up");
        let k = key(GROCERY, "Tomato", 4320.0);
        assert_eq!(parse(&k), Some((GROCERY, "Tomato", 4320.0)));
        let mut inv = Inventory::default();
        inv.add(ItemKind::Coupon, &k, "Coupon for Tomato", 40, 3);
        assert_eq!(held(&inv, GROCERY, "Tomato").map(|h| h.1), Some(3));
        assert!(held(&inv, BOOKSTORE, "Tomato").is_none());
    }
}
