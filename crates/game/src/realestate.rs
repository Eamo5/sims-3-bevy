//! Real estate in the town's rabbit holes, as the game's `RealEstateManager` and its
//! `InvestInRabbithole`: at a rabbit hole the game lets be invested in, "Become a Partner (§N)",
//! then "Buy Out Property (§N)", each half an hour inside and paid on the way out (the place's
//! own tuning: a bistro §12,500 and §25,000 more, a science facility §40,000 and §100,000).
//! From then on it's the household's: its share of the profits mounts up each Monday at nine
//! (all of them for an owner), there for the collecting ("Collect Money (§N)"); and it can be
//! sold back for what was paid and what's uncollected ("Sell Property", which the game does from
//! its real estate dialog). A partner or owner eats free at their diner or bistro (the game's
//! `Restaurant`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{Household, Notifications};
use crate::loading::WorldInfo;
use crate::rabbitholes::Activity;
use crate::sim::Sim;

pub struct RealEstatePlugin;

impl Plugin for RealEstatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RealEstate>()
            .add_systems(Update, (deals_done, profits).run_if(in_state(PlayMode::Live)))
            .add_systems(OnEnter(crate::AppState::CreateHousehold), |mut r: ResMut<RealEstate>| *r = RealEstate::default());
    }
}

/// A visit's deal: becoming a partner, buying out, collecting, selling.
pub const INVEST_TASK: usize = 7000;
pub const BUYOUT_TASK: usize = 7001;
pub const COLLECT_TASK: usize = 7002;
pub const SELL_TASK: usize = 7003;

const fn deal(name: &'static str, minutes: f32) -> Activity {
    Activity { name, minutes, cost: 0, per_hour: [0.0; 6], skill: None, open: 0.0, close: 24.0 }
}
/// Half an hour inside for the papers (the game's `InvestInRabbithole`), a few minutes the rest.
pub static INVEST: Activity = deal("Become a Partner", 30.0);
pub static BUYOUT: Activity = deal("Buy Out Property", 30.0);
pub static COLLECT: Activity = deal("Collect Money", 10.0);
pub static SELL: Activity = deal("Sell Property", 30.0);

/// The rabbit holes the game lets be invested in, by their class, with their tuning (the class
/// XML's `kInvestCost`, `kBuyoutCost`, `kInvestmentReturnPerWeek`, `kBuyoutReturnPerWeek`).
const TUNING: [(&str, [i64; 4]); 11] = [
    ("Bistro", [12500, 25000, 2100, 3000]),
    ("Bookstore", [7000, 20000, 1000, 2000]),
    ("BusinessAndJournalismRabbitHole", [18000, 40000, 3000, 6000]),
    ("DaySpa", [15000, 35000, 2500, 4000]),
    ("Diner", [6000, 18000, 850, 1800]),
    ("Grocery", [7000, 20000, 1000, 2000]),
    ("Hideout", [15000, 35000, 2500, 4000]),
    ("Hospital", [30000, 75000, 6000, 12000]),
    ("ScienceLab", [40000, 100000, 7900, 15000]),
    ("Stadium", [35000, 80000, 6800, 13000]),
    ("Theatre", [18000, 40000, 3000, 6000]),
];
/// The notices' keys, a man's and a woman's (`OwnerTNS`, `PartnerTNS`).
const OWNER_TNS: (u64, u64) = (0x152a_cbec_391a_2a1b, 0x42f6_9f31_5fa8_5402);
const PARTNER_TNS: (u64, u64) = (0x923d_523c_86af_8ea6, 0x3ecb_3612_e345_b29d);
/// Monday, nine in the morning: when the profits come in (`RealEstateManager`).
const PAYDAY_HOUR: f32 = 9.0;

/// A place the household has a stake in: its lot, whether it's theirs outright, what was paid
/// for it, and the profits waiting there.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Holding {
    pub lot: usize,
    pub full: bool,
    pub stored: i64,
    pub funds: i64,
}

/// The household's real estate, and the last week its profits came in.
#[derive(Resource, Clone, Debug, Default, Serialize, Deserialize)]
pub struct RealEstate {
    pub holdings: Vec<Holding>,
    #[serde(default)]
    pub paid_week: u32,
}

impl RealEstate {
    pub fn holding(&self, lot: usize) -> Option<&Holding> {
        self.holdings.iter().find(|h| h.lot == lot)
    }

    /// Whether a meal out here is on the house (a diner or bistro they've a stake in).
    pub fn free_meal(&self, lot: usize, activity: usize) -> bool {
        self.holding(lot).is_some()
            && activity.checked_sub(crate::venues::VENUE_TASK).and_then(|i| crate::venues::ACTS.get(i)).is_some_and(|a| matches!(a.effect, crate::venues::Effect::Meal(_)))
    }
}

/// Whether a stake can be bought in a lot (its rabbit hole can be invested in).
pub fn purchasable(world: &WorldInfo, lot: usize) -> bool {
    tuning(world, lot).is_some()
}

/// The rabbit hole on a lot that can be invested in: its class's tuning.
fn tuning(world: &WorldInfo, lot: usize) -> Option<[i64; 4]> {
    let b = world.buildings.get(&lot)?;
    b.objects.iter().find_map(|o| {
        let class = o.script.split("RabbitHoles.").nth(1)?;
        TUNING.iter().find(|(c, _)| class == *c).map(|t| t.1)
    })
}

/// What's to be done here for a grown-up of the household, as their stake has it: become a
/// partner (if they can afford it), buy out the rest, collect what's waiting, or sell.
pub fn options(world: &WorldInfo, lot: usize, sim: &Sim, funds: i64, re: &RealEstate) -> Vec<(String, usize)> {
    let Some([invest, buyout, ..]) = tuning(world, lot) else { return Vec::new() };
    if !sim.age.is_grown() || sim.age == crate::sim::Age::Child {
        return Vec::new();
    }
    let mut out = Vec::new();
    match re.holding(lot) {
        None if funds >= invest => out.push((format!("Become a Partner (§{invest})"), INVEST_TASK)),
        None => {}
        Some(h) => {
            if !h.full && funds >= buyout {
                out.push((format!("Buy Out Property (§{buyout})"), BUYOUT_TASK));
            }
            if h.funds > 0 {
                out.push((format!("Collect Money (§{})", h.funds), COLLECT_TASK));
            }
            out.push((format!("Sell Property (§{})", h.stored + h.funds), SELL_TASK));
        }
    }
    out
}

/// A visit's deal, from its task.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum Deal {
    Invest,
    Buyout,
    Collect,
    Sell,
}

impl Deal {
    pub fn of(task: usize) -> Option<Self> {
        Some(match task {
            INVEST_TASK => Self::Invest,
            BUYOUT_TASK => Self::Buyout,
            COLLECT_TASK => Self::Collect,
            SELL_TASK => Self::Sell,
            _ => return None,
        })
    }

    pub fn activity(self) -> &'static Activity {
        match self {
            Self::Invest => &INVEST,
            Self::Buyout => &BUYOUT,
            Self::Collect => &COLLECT,
            Self::Sell => &SELL,
        }
    }
}

pub fn is_deal(a: &Activity) -> bool {
    [&INVEST, &BUYOUT, &COLLECT, &SELL].iter().any(|d| std::ptr::eq(a, *d))
}

/// Back from it (at that lot).
#[derive(Component)]
pub struct DealDone(pub usize);

/// The deal done on the way out: paid for (if they still can), collected or sold.
#[allow(clippy::too_many_arguments)]
fn deals_done(
    mut commands: Commands,
    sims: Query<(Entity, &Sim, &Deal, &DealDone)>,
    mut re: ResMut<RealEstate>,
    mut household: Option<ResMut<Household>>,
    world: Res<crate::loading::CurrentWorld>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    clock: Res<crate::clock::GameClock>,
) {
    for (e, sim, deal, done) in &sims {
        commands.entity(e).remove::<(Deal, DealDone)>();
        let lot = done.0;
        let Some([invest, buyout, ..]) = tuning(&world.data, lot) else { continue };
        let Some(h) = household.as_deref_mut() else { continue };
        let place = world.data.lots.get(lot).map(|l| crate::rabbitholes::lot_title(l, world.data.lot_names.get(lot).map_or("", |s| s.as_str()))).unwrap_or_default();
        let tns = |keys: (u64, u64)| {
            ui.as_deref()
                .and_then(|u| u.localize_key(if sim.female { keys.1 } else { keys.0 }))
                .map(|t| t.replace("{0.SimFirstName}", &sim.first).replace("{1.String}", &place))
        };
        match deal {
            Deal::Invest | Deal::Buyout => {
                let (cost, full) = if *deal == Deal::Buyout { (buyout, true) } else { (invest, false) };
                // (Not if it's no longer what it was, or they can't pay now: `NotEnoughMoneyTNS`.)
                let ok = match re.holding(lot) {
                    None => !full,
                    Some(x) => full && !x.full,
                };
                if !ok {
                    continue;
                }
                if h.funds < cost {
                    notes.push(format!("{} doesn't have enough money for that.", sim.first));
                    continue;
                }
                h.funds -= cost;
                match re.holdings.iter_mut().find(|x| x.lot == lot) {
                    Some(x) => {
                        x.full = true;
                        x.stored += cost;
                    }
                    None => {
                        // (The first stake: profits from the next payday on, not this one past.)
                        if re.holdings.is_empty() {
                            re.paid_week = payday_week(&clock);
                        }
                        re.holdings.push(Holding { lot, full: false, stored: cost, funds: 0 });
                    }
                }
                info!("{} {} {place} for §{cost}", sim.first, if full { "bought out" } else { "became a partner at" });
                notes.push(tns(if full { OWNER_TNS } else { PARTNER_TNS }).unwrap_or_else(|| format!("{} is now {} {place}!", sim.first, if full { "the owner of" } else { "a partner at" })));
                play.write(crate::sound::PlaySound::ui("sting_career_positive"));
            }
            Deal::Collect => {
                let Some(x) = re.holdings.iter_mut().find(|x| x.lot == lot) else { continue };
                if x.funds > 0 {
                    h.funds += x.funds;
                    notes.push(format!("{} collected §{} from {place}.", sim.first, x.funds));
                    x.funds = 0;
                }
            }
            Deal::Sell => {
                let Some(i) = re.holdings.iter().position(|x| x.lot == lot) else { continue };
                let x = re.holdings.remove(i);
                h.funds += x.stored + x.funds;
                notes.push(format!("{} sold the household's stake in {place} for §{}.", sim.first, x.stored + x.funds));
            }
        }
    }
}

/// The week of the last payday that's come (Monday at nine; the week before, till then).
fn payday_week(clock: &crate::clock::GameClock) -> u32 {
    let week = clock.day() / 7 + 1;
    if clock.weekday() == 0 && clock.hour_f() < PAYDAY_HOUR { week - 1 } else { week }
}

/// Monday at nine: each stake's share of the week's profits, waiting to be collected (all of
/// them for an owner, `kBuyoutReturnPerWeek`; a partner's share, `kInvestmentReturnPerWeek`).
fn profits(clock: Res<crate::clock::GameClock>, mut re: ResMut<RealEstate>, world: Res<crate::loading::CurrentWorld>) {
    if re.holdings.is_empty() {
        return;
    }
    let week = payday_week(&clock);
    if re.paid_week >= week {
        return;
    }
    re.paid_week = week;
    for i in 0..re.holdings.len() {
        let lot = re.holdings[i].lot;
        let Some([_, _, partner, owner]) = tuning(&world.data, lot) else { continue };
        let h = &mut re.holdings[i];
        h.funds += if h.full { owner } else { partner };
    }
    info!("the week's profits are in: {:?}", re.holdings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deals_are_by_task_and_meals_free_where_theirs() {
        assert_eq!(Deal::of(INVEST_TASK), Some(Deal::Invest));
        assert_eq!(Deal::of(SELL_TASK).map(|d| d.activity().name), Some("Sell Property"));
        assert!(Deal::of(42).is_none());
        let mut re = RealEstate::default();
        re.holdings.push(Holding { lot: 3, full: false, stored: 6000, funds: 0 });
        let eat = crate::venues::ACTS.iter().position(|a| matches!(a.effect, crate::venues::Effect::Meal(_))).unwrap() + crate::venues::VENUE_TASK;
        assert!(re.free_meal(3, eat));
        assert!(!re.free_meal(4, eat), "not where they've no stake");
        let drinks = crate::venues::ACTS.iter().position(|a| matches!(a.effect, crate::venues::Effect::Drinks)).unwrap() + crate::venues::VENUE_TASK;
        assert!(!re.free_meal(3, drinks), "drinks are paid for");
    }

    #[test]
    fn profits_come_on_monday_mornings() {
        let at = |day: f64, hour: f64| {
            let mut c = crate::clock::GameClock::default();
            c.minutes = day * 1440.0 + hour * 60.0;
            c
        };
        assert_eq!(payday_week(&at(0.0, 8.0)), 0, "not yet the first Monday's nine o'clock");
        assert_eq!(payday_week(&at(0.0, 9.5)), 1);
        assert_eq!(payday_week(&at(3.0, 2.0)), 1);
        assert_eq!(payday_week(&at(7.0, 8.9)), 1);
        assert_eq!(payday_week(&at(7.0, 9.0)), 2);
    }
}
