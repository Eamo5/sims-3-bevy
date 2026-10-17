//! Gems mailed off to be cut and metals to be smelted, as the game's (`Gem.GetCut`,
//! `Metal.GetSmelt`): picked from the Sim's inventory ("Get Cut..." to a cut, "Get Smelted"),
//! paid for and posted at the household's mailbox, and back with the next morning's mail: the
//! gem cut ("Pear Cut Ruby", worth its cut's `Multiplier` more), the metal an ingot ("Gold
//! Ingot", the game's `kSmeltMultiplier` 1.75), now and then more than one (`kChanceOfExtraIngot`
//! 0.2 each time, up to `kMaxNumberOfIngots` 5). The finer cuts come with practice: each wants
//! that many gems the Sim has had cut before (its `Cuts`).

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use s3bake::gamedata::CollectKind;

use crate::PlayMode;
use crate::interact::{Household, Notifications};
use crate::inventory::{Inventory, ItemKind, Stack};
use crate::sim::{HouseholdMember, Sim};

pub struct RgmMailPlugin;

impl Plugin for RgmMailPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Posted>()
            .add_systems(Update, (post_off, mail_back).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(crate::AppState::InGame), |mut p: ResMut<Posted>| p.0.clear());
    }
}

/// A cut from the game's `GemCuts` table: its name, what it multiplies a gem's worth by, how many
/// gems the Sim must have had cut first, and its price.
pub struct Cut {
    pub name: &'static str,
    pub multiplier: f32,
    pub cuts: u32,
    pub cost: i64,
}

const fn cut(name: &'static str, multiplier: f32, cuts: u32, cost: i64) -> Cut {
    Cut { name, multiplier, cuts, cost }
}

/// The base game's cuts (the Heart's, its `sToughestCut`, is a Gem Collector's alone).
pub static CUTS: [Cut; 8] = [
    cut("Emerald", 1.25, 0, 10),
    cut("Oval", 1.5, 4, 20),
    cut("Pear", 1.75, 8, 35),
    cut("Plumbbob", 2.0, 16, 50),
    cut("Marquis", 2.3, 30, 75),
    cut("Crystal Ball", 2.6, 45, 100),
    cut("Brilliant", 3.5, 60, 250),
    cut("Star", 4.0, 75, 400),
];

/// `GetSmelt`'s tuning: `kCostToSmelt` 40, `kChanceOfExtraIngot`, `kMaxNumberOfIngots`; and the
/// metal's `kSmeltMultiplier`.
pub const SMELT_COST: i64 = 40;
const SMELT_MULTIPLIER: f32 = 1.75;
const EXTRA_INGOT: f64 = 0.2;
const MOST_INGOTS: u32 = 5;

/// The hour the mail brings them back, the day after.
const BACK_HOUR: f32 = 10.0;

/// How the game names its menus.
pub const GET_CUT: &str = "Get Cut";

/// What's mailed off: to be cut (which), or smelted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Post {
    Cut(u8),
    Smelt,
}

/// What a Sim's on their way to the mailbox to post: which stack of their inventory (one of it
/// or all), and to be what.
#[derive(Component, Clone, Debug)]
pub struct ToPost {
    pub key: String,
    pub all: bool,
    pub what: Post,
}

/// Posted (the mailbox reached): for `post_off`.
#[derive(Component)]
pub struct PostedNow;

/// What's away being cut or smelted: whose (their Sim's id), what comes back, the day it does,
/// and whether it's gems.
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct Posted(pub Vec<Away>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Away {
    pub sim: u64,
    pub back: Stack,
    pub day: u32,
    pub gem: bool,
}

/// A find's kind (a gem, a metal...), by its key.
pub fn find_kind(data: &s3bake::GameDataBaked, s: &Stack) -> Option<CollectKind> {
    (s.kind == ItemKind::Find).then(|| data.collectibles.iter().find(|c| c.key == s.key).map(|c| c.kind)).flatten()
}

/// A gem stack's quality once cut (1 and on, by its cut), a metal's once smelted (1): uncut and
/// raw ones are 0.
fn quality(what: Post) -> u8 {
    match what {
        Post::Cut(i) => i + 1,
        Post::Smelt => 1,
    }
}

/// What can be had done to a stack of finds by mail, as its menu offers it: a raw metal smelted
/// ("Get Smelted (§40)", "Smelt All §80"), an uncut gem cut ("Get Cut..." the cuts the Sim has
/// cut enough gems for: "Pear Cut (§35)"), each paid for the stack's gems when it's "Cut all...".
pub fn options(data: &s3bake::GameDataBaked, s: &Stack, gems_cut: u32) -> (Vec<(String, Post, bool)>, Vec<(String, Post, bool)>) {
    let (mut flat, mut cuts) = (Vec::new(), Vec::new());
    if s.quality != 0 {
        return (flat, cuts);
    }
    match find_kind(data, s) {
        Some(CollectKind::Metal) => {
            flat.push((format!("Get Smelted (§{SMELT_COST})"), Post::Smelt, false));
            if s.count > 1 {
                flat.push((format!("Smelt All §{}", SMELT_COST * s.count as i64), Post::Smelt, true));
            }
        }
        Some(CollectKind::Gem) => {
            for (i, c) in CUTS.iter().enumerate().filter(|(_, c)| c.cuts <= gems_cut) {
                cuts.push((format!("{} Cut (§{})", c.name, c.cost), Post::Cut(i as u8), false));
                if s.count > 1 {
                    cuts.push((format!("{} Cut, all (§{})", c.name, c.cost * s.count as i64), Post::Cut(i as u8), true));
                }
            }
        }
        _ => {}
    }
    (flat, cuts)
}

/// What one costs.
fn cost(what: Post) -> i64 {
    match what {
        Post::Cut(i) => CUTS[i as usize].cost,
        Post::Smelt => SMELT_COST,
    }
}

/// Posted at the mailbox: out of the Sim's inventory and paid for, away till the next morning's
/// mail (a gem cut kept count of, for the finer cuts).
fn post_off(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &ToPost, &mut Inventory), With<PostedNow>>,
    mut household: Option<ResMut<Household>>,
    mut posted: ResMut<Posted>,
    clock: Res<crate::clock::GameClock>,
    mut notes: ResMut<Notifications>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    let mut rng = rand::rng();
    for (e, sim, post, mut inv) in &mut sims {
        commands.entity(e).remove::<(ToPost, PostedNow)>();
        let Some(k) = inv.0.iter().position(|s| s.kind == ItemKind::Find && s.key == post.key && s.quality == 0) else { continue };
        let n = if post.all { inv.0[k].count } else { 1 };
        let price = cost(post.what) * n as i64;
        let Some(h) = household.as_deref_mut() else { continue };
        if h.funds < price {
            notes.push(format!("{} can't afford to mail it off (§{price}).", sim.first));
            continue;
        }
        h.funds -= price;
        let name = inv.0[k].name.clone();
        let mut worth = 0;
        for _ in 0..n {
            worth += inv.take_one(k).unwrap_or(0);
        }
        let (back, gem) = match post.what {
            Post::Cut(i) => {
                let c = &CUTS[i as usize];
                did.write(crate::journal::Did::count(e, crate::journal::Stat::GemsCut, n as f64));
                notes.push(format!("{} mailed the {name} off to the gem cutter.", sim.first));
                (Stack { kind: ItemKind::Find, key: post.key.clone(), name: format!("{} Cut {name}", c.name), quality: quality(post.what), count: n, worth: (worth as f32 * c.multiplier).round() as i64 }, true)
            }
            Post::Smelt => {
                // (Each metal one ingot, or more by chance.)
                let each = (worth as f32 / n.max(1) as f32 * SMELT_MULTIPLIER).round() as i64;
                let mut ingots = 0;
                for _ in 0..n {
                    let mut got = 1;
                    while got < MOST_INGOTS && rng.random_bool(EXTRA_INGOT) {
                        got += 1;
                    }
                    ingots += got;
                }
                notes.push(format!("{} mailed the {name} off to be smelted.", sim.first));
                (Stack { kind: ItemKind::Find, key: post.key.clone(), name: format!("{name} Ingot"), quality: quality(post.what), count: ingots, worth: each * ingots as i64 }, false)
            }
        };
        posted.0.push(Away { sim: sim.id, back, day: clock.day() + 1, gem });
    }
}

/// The next morning's mail: what was posted, back in its Sim's inventory (or, theirs gone, the
/// household's first), as the game's notices have it.
fn mail_back(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut posted: ResMut<Posted>,
    mut members: Query<(Entity, &Sim, Option<&mut Inventory>), With<HouseholdMember>>,
    mut notes: ResMut<Notifications>,
) {
    if posted.0.is_empty() || clock.hour_f() < BACK_HOUR {
        return;
    }
    let day = clock.day();
    let (due, still): (Vec<Away>, Vec<Away>) = std::mem::take(&mut posted.0).into_iter().partition(|a| a.day <= day);
    posted.0 = still;
    for a in due {
        let Some(who) = members.iter().find(|m| m.1.id == a.sim).or_else(|| members.iter().next()).map(|m| m.0) else { continue };
        let Ok((e, sim, inv)) = members.get_mut(who) else { continue };
        let what = if a.back.count > 1 { format!("{} × {}", a.back.count, a.back.name) } else { a.back.name.clone() };
        notes.push(if a.gem { format!("{} received a shiny {what} from the gem cutter!", sim.first) } else { format!("{} received {what} from the local smelting facility.", sim.first) });
        match inv {
            Some(mut inv) => inv.put(a.back),
            None => {
                commands.entity(e).insert(Inventory(vec![a.back]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_finer_cuts_come_with_gems_cut() {
        let open = |n: u32| CUTS.iter().filter(|c| c.cuts <= n).count();
        assert_eq!(open(0), 1);
        assert_eq!(open(8), 3);
        assert_eq!(open(75), 8);
    }

    #[test]
    fn what_comes_back_is_kept_apart_from_the_uncut() {
        assert_eq!(quality(Post::Cut(0)), 1);
        assert_eq!(quality(Post::Smelt), 1);
        assert_eq!(cost(Post::Cut(2)), 35);
        assert_eq!(cost(Post::Smelt), 40);
    }
}
