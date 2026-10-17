//! The outdoor trash can, as the game's `TrashcanOutside`. It's knocked over (`KnockOver`, a
//! bit of fun) by the Insane anywhere, by the Mean-Spirited, the Evil and the Inappropriate off
//! their own lot, and by an enemy of the family living there; it lies on its side with its lid
//! off (its clip's `knock_over_pose` and the `BinOnly` geometry) till one of that family Picks
//! it Up, as the Neat are keen to. Teens and up Rummage through a family's can (`Rummage`:
//! never their own unless they're Insane, nor the Neat with a job; once a day each, played):
//! something's turned up every eight minutes, seven things at most
//! (`kRevealItemEveryXMinutes`, `kRummageMaxItems`), from what the game lists by how well off
//! the family is (worth §30,000 a fifth of the time the middling list, §95,000 now and then the
//! best), fifteen times in a hundred a rock, gem or metal or a seed instead
//! (`kChanceNonListItem`). Smelly work. Any of the family who see it insult them; anyone else is
//! put off them (`kRummageSTCInsultingChange`). Knocked over is kept in saves.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{Action, ActionKind, ActionQueue, GameObject, Notifications, ObjectKind, Phase, Special};
use crate::life::Trait;
use crate::sim::{Age, HouseholdMember, Sim};

pub struct TrashcansPlugin;

impl Plugin for TrashcansPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CanOwners>()
            .init_resource::<Rummaged>()
            .add_systems(Update, (owners, can_done, rummaging, show_can).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The can's clip pose, lying on its side.
const TIPPED_POSE: &str = "a2o_trashCan_knock_over_pose_trashcan";
/// The can's geometry without its lid.
const BIN_ONLY: &str = "BinOnly";
/// `kRevealItemEveryXMinutes` and `kRummageMaxItems` (the can's tuning).
const FIND_EVERY: f64 = 8.0;
pub const MAX_FINDS: usize = 7;
/// What the family's worth for the middling and the best finds (`kNetWorthMedium`,
/// `kNetWorthHigh`), and the chances of them (percent: `kNetWorthMedChanceForMedList`,
/// `kNetWorthHighChanceForMedList`, `kNetWorthHighChanceForHighList`).
const WORTH_MEDIUM: i64 = 30_000;
const WORTH_HIGH: i64 = 95_000;
const MEDIUM_CHANCE_MEDIUM: f64 = 0.2;
const HIGH_CHANCE_MEDIUM: f64 = 0.3;
const HIGH_CHANCE_HIGH: f64 = 0.03;
/// `kChanceNonListItem`: a rock, gem or metal, or a seed.
const NON_LIST_CHANCE: f64 = 0.15;
/// What turns up, by how well off the family is (the game's `ListCheap`, `ListMedium`,
/// `ListHigh`: catalogue objects).
const CHEAP: [&str; 3] = ["TrashPileIndoor", "TrashpileOutdoor", "Newspaper"];
const MEDIUM: [&str; 4] = ["RubberDucky", "BubbleBath", "CandleSingle", "TeddyBear"];
const HIGH: [&str; 2] = ["Laptop", "VRGoggles"];
/// Seeing it: how near (the broadcaster's `PulseRadius`), how often looked for (its
/// `PulseRepeatTime`), and how insulting it is (`kRummageSTCInsultingChange`).
const SEEN_WITHIN: f32 = 10.0;
const LOOK_EVERY: f64 = 5.0;
const INSULTING: f32 = 20.0;
/// How keen the Neat are to set a can up again (its `TraitNeat` 200), and anyone else (its
/// `Dirtiness` 15).
const NEAT_PULL: f32 = 60.0;
const TIDY_PULL: f32 = 6.0;

/// Knocked over (kept in saves).
#[derive(Component)]
pub struct TippedOver;

/// How a can's drawn now: whole, or with its lid off.
#[derive(Component, Clone, Copy, PartialEq)]
enum CanLook {
    Whole,
    LidOff,
}

/// Whose each outdoor can is: the lot it's on (none: the household's own), who lives there, and
/// what they're worth, home and belongings.
#[derive(Resource, Default)]
pub struct CanOwners(pub HashMap<Entity, CanOwner>);

#[derive(Clone, Default)]
pub struct CanOwner {
    pub lot: Option<usize>,
    pub family: Vec<Entity>,
    pub lived_in: bool,
    pub worth: i64,
}

/// The cans rummaged through today (by where they stand), as played: the game's
/// `HasBeenRummagedByPlayer`, reset at midnight.
#[derive(Resource, Default)]
pub struct Rummaged {
    day: u32,
    cans: HashSet<(i32, i32)>,
}

impl Rummaged {
    fn spot(p: Vec3) -> (i32, i32) {
        ((p.x * 10.0).round() as i32, (p.z * 10.0).round() as i32)
    }

    pub fn today(&self, day: u32, p: Vec3) -> bool {
        self.day == day && self.cans.contains(&Self::spot(p))
    }

    fn mark(&mut self, day: u32, p: Vec3) {
        if self.day != day {
            self.day = day;
            self.cans.clear();
        }
        self.cans.insert(Self::spot(p));
    }
}

/// A Sim done with a can: knocked it over or picked it up.
#[derive(Component)]
pub enum CanDone {
    KnockedOver(Entity),
    PickedUp(Entity),
}

/// Rummaging through a can: when something next turns up, what has, and who's seen them at it.
#[derive(Component)]
pub struct Rummaging {
    can: Entity,
    next_find: f64,
    next_look: f64,
    found: Vec<String>,
    seen: Vec<Entity>,
}

/// Where a Sim lives, as it bears on a can: the household's lot (`Some(None)`), the visited lot
/// for a family living there (`Some(Some(lot))`), or neither.
pub fn home_of(household: bool, resident: bool, visited: Option<usize>) -> Option<Option<usize>> {
    if household {
        Some(None)
    } else if resident {
        visited.map(Some)
    } else {
        None
    }
}

/// Whether they'd knock it over (the game's test).
pub fn may_knock_over(sim: &Sim, at_home: bool, enemy_of_family: bool) -> bool {
    let has = |t: Trait| sim.traits.contains(&t);
    has(Trait::Insane) || (!at_home && (has(Trait::MeanSpirited) || has(Trait::Evil) || has(Trait::Inappropriate))) || enemy_of_family
}

/// Whether they're an enemy of anyone living there.
pub fn enemy_of(rels: &crate::social::Relationships, owner: &CanOwner) -> bool {
    owner.family.iter().any(|f| rels.0.get(f).is_some_and(|r| r.enemy))
}

/// Whether they'd rummage through it (the game's test; and teens and up).
pub fn may_rummage(sim: &Sim, at_home: bool, lived_in: bool, employed: bool, done_today: bool) -> bool {
    let grown = matches!(sim.age, Age::Teen | Age::YoungAdult | Age::Adult | Age::Elder);
    let has = |t: Trait| sim.traits.contains(&t);
    grown && lived_in && (!at_home || has(Trait::Insane)) && !(has(Trait::Neat) && employed) && !done_today
}

/// Whether they'd set it up again: one of the family, teen and up.
pub fn may_pick_up(sim: &Sim, at_home: bool) -> bool {
    at_home && matches!(sim.age, Age::Teen | Age::YoungAdult | Age::Adult | Age::Elder)
}

/// How keen they are to set it up again, by free will.
pub fn pick_up_pull(sim: &Sim) -> f32 {
    if sim.traits.contains(&Trait::Neat) { NEAT_PULL } else { TIDY_PULL }
}

/// Which list a find comes from (0 cheap, 1 middling, 2 the best), by what the family's worth.
fn tier(worth: i64, rng: &mut impl Rng) -> usize {
    if worth < WORTH_MEDIUM {
        0
    } else if worth < WORTH_HIGH {
        rng.random_bool(MEDIUM_CHANCE_MEDIUM) as usize
    } else if rng.random_bool(HIGH_CHANCE_HIGH) {
        2
    } else {
        rng.random_bool(HIGH_CHANCE_MEDIUM) as usize
    }
}

/// Whose each can is, kept up to date.
#[allow(clippy::type_complexity)]
fn owners(
    time: Res<Time>,
    mut last: Local<f32>,
    mut out: ResMut<CanOwners>,
    objects: Query<(Entity, &GameObject, Option<&crate::visit::LotObject>)>,
    family: Query<Entity, Or<(With<HouseholdMember>, With<crate::homes::Resident>)>>,
    members: Query<(), With<HouseholdMember>>,
    household: Option<Res<crate::interact::Household>>,
    visited: Option<Res<crate::visit::VisitedLot>>,
    world: Res<crate::loading::CurrentWorld>,
    (town, story, dormant): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>),
) {
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    out.0.clear();
    let worth_of = |lot: Option<usize>| objects.iter().filter(|(_, _, l)| l.map(|l| l.0) == lot).map(|(_, o, _)| o.price.max(0) as i64).sum::<i64>();
    let home_family: Vec<Entity> = family.iter().filter(|e| members.contains(*e)).collect();
    let away_family: Vec<Entity> = family.iter().filter(|e| !members.contains(*e)).collect();
    let visited_family = visited.as_ref().and_then(|v| crate::homes::family_at(&world, town.as_deref(), &story, &dormant, household.as_ref().map(|h| h.lot_index), v.lot));
    let home = CanOwner { lot: None, family: home_family, lived_in: true, worth: household.as_ref().map_or(0, |h| h.funds) + worth_of(None) };
    let away = visited.as_ref().map(|v| CanOwner {
        lot: Some(v.lot),
        family: away_family,
        lived_in: visited_family.is_some(),
        worth: visited_family.map_or(0, |f| f.funds) + worth_of(Some(v.lot)),
    });
    for (e, o, lot) in &objects {
        if o.kind != ObjectKind::OutdoorTrashCan {
            continue;
        }
        let owner = match lot {
            None => home.clone(),
            Some(l) => away.clone().filter(|a| a.lot == Some(l.0)).unwrap_or(CanOwner { lot: Some(l.0), ..default() }),
        };
        out.0.insert(e, owner);
    }
}

/// A can knocked over or set up again.
fn can_done(mut commands: Commands, sims: Query<(Entity, &CanDone)>, cans: Query<(), With<GameObject>>) {
    for (e, done) in &sims {
        commands.entity(e).remove::<CanDone>();
        match *done {
            CanDone::KnockedOver(can) if cans.contains(can) => {
                commands.entity(can).insert(TippedOver);
            }
            CanDone::PickedUp(can) if cans.contains(can) => {
                commands.entity(can).remove::<TippedOver>();
            }
            _ => {}
        }
    }
}

/// Rummaging: something turned up every eight minutes; anyone seeing it put off; what was found
/// said at the end.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn rummaging(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, Option<&mut Rummaging>, &Transform)>,
    lookers: Query<(Entity, &Transform, &Visibility), With<Sim>>,
    objects: Query<(&GameObject, &Transform)>,
    owners: Res<CanOwners>,
    mut rummaged: ResMut<Rummaged>,
    (data, ui, mut garden, mut collection): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>, ResMut<crate::gardening::Garden>, ResMut<crate::collecting::Collection>),
    mut notes: ResMut<Notifications>,
) {
    let now = clock.minutes;
    let mut rng = rand::rng();
    let insult = crate::social::social_index("Insult");
    for (e, sim, mut queue, rummage, tf) in &mut sims {
        let at = queue.0.front().filter(|a| matches!(a.phase, Phase::Running(_)) && !a.cancel).and_then(|a| match a.kind {
            ActionKind::Object { target, def } => objects
                .get(target)
                .ok()
                .and_then(|(o, _)| crate::interact::interactions_for(o.kind).get(def))
                .filter(|d| d.special == Special::Rummage)
                .map(|_| target),
            _ => None,
        });
        match (at, rummage) {
            (Some(can), None) => {
                if let Ok((_, ctf)) = objects.get(can) {
                    rummaged.mark(clock.day(), ctf.translation);
                }
                commands.entity(e).insert(Rummaging { can, next_find: now + FIND_EVERY, next_look: now, found: Vec::new(), seen: Vec::new() });
            }
            (Some(can), Some(mut r)) => {
                let owner = owners.0.get(&can).cloned().unwrap_or_default();
                if now >= r.next_find {
                    r.next_find += FIND_EVERY;
                    if let Some(what) = find(&mut commands, e, owner.worth, data.as_deref(), ui.as_deref(), &mut garden, &mut collection, &mut rng) {
                        r.found.push(what);
                    }
                    if r.found.len() >= MAX_FINDS
                        && let Some(a) = queue.0.front_mut()
                    {
                        a.cancel = true;
                    }
                }
                // (Seen at it: the family insult them; anyone else's put off them.)
                if now >= r.next_look {
                    r.next_look = now + LOOK_EVERY;
                    for (o, otf, vis) in &lookers {
                        if o == e || *vis == Visibility::Hidden || r.seen.contains(&o) {
                            continue;
                        }
                        if otf.translation.distance(tf.translation) > SEEN_WITHIN || (otf.translation.y - tf.translation.y).abs() > 1.8 {
                            continue;
                        }
                        r.seen.push(o);
                        if owner.family.contains(&o) {
                            let Some(si) = insult else { continue };
                            commands.entity(o).queue_silenced(move |mut w: EntityWorldMut| {
                                if let Some(mut q) = w.get_mut::<ActionQueue>() {
                                    q.0.push_back(Action::new("Insult", ActionKind::Social { target: e, social: si }, true));
                                }
                            });
                        } else {
                            commands.entity(o).queue_silenced(move |mut w: EntityWorldMut| {
                                if let Some(mut rels) = w.get_mut::<crate::social::Relationships>() {
                                    let x = rels.entry(e);
                                    x.stc.update(crate::conversation::Commodity::Insulting, INSULTING);
                                    let d = crate::conversation::liking_change(&x.stc, x.friendship);
                                    rels.add(e, d, 0.0);
                                }
                            });
                        }
                    }
                }
            }
            (None, Some(r)) => {
                if r.found.is_empty() {
                    notes.push(format!("{} rummaged through the trash and found nothing worth keeping.", sim.first));
                } else {
                    notes.push(format!("{} rummaged through the trash and found {}. It's all in {} inventory.", sim.first, list(&r.found), if sim.female { "her" } else { "his" }));
                }
                commands.entity(e).remove::<Rummaging>();
            }
            (None, None) => {}
        }
    }
}

/// What was found, said: "some trash, Newspaper ×2 and a Teddy Bear".
fn list(items: &[String]) -> String {
    let mut counted: Vec<(&str, usize)> = Vec::new();
    for i in items {
        match counted.iter_mut().find(|(n, _)| *n == i.as_str()) {
            Some(c) => c.1 += 1,
            None => counted.push((i.as_str(), 1)),
        }
    }
    let said: Vec<String> = counted
        .into_iter()
        .map(|(n, c)| match (n, c) {
            ("a Trash", _) => "some trash".to_string(),
            (n, 1) => n.to_string(),
            (n, c) => format!("{} ×{c}", n.strip_prefix("an ").or_else(|| n.strip_prefix("a ")).unwrap_or(n)),
        })
        .collect();
    match said.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Something turned up, into their inventory: what it is, as said ("a Rubber Ducky").
#[allow(clippy::too_many_arguments)]
fn find(
    commands: &mut Commands,
    sim: Entity,
    worth: i64,
    data: Option<&crate::baked::Baked>,
    ui: Option<&crate::icons::GameUi>,
    garden: &mut crate::gardening::Garden,
    collection: &mut crate::collecting::Collection,
    rng: &mut impl Rng,
) -> Option<String> {
    let a = |name: &str| if name.starts_with(['A', 'E', 'I', 'O', 'U']) { format!("an {name}") } else { format!("a {name}") };
    if rng.random_bool(NON_LIST_CHANCE) {
        let ui = ui?;
        // (A third of the time a seed, of any rarity but the special; else a rock, gem or metal:
        // the game's flop of a book, too, as there are no written books to find.)
        if rng.random_range(0..3) == 2 {
            let rarity = crate::gardening::RARITIES[rng.random_range(0..3)];
            return crate::gardening::find_seed(garden, &ui.data, rarity, rng);
        }
        use s3bake::gamedata::CollectKind;
        let finds: Vec<_> = ui.data.collectibles.iter().filter(|c| matches!(c.kind, CollectKind::Gem | CollectKind::Metal | CollectKind::SpaceRock)).collect();
        let c = *finds.get(rng.random_range(0..finds.len().max(1)))?;
        let value = rng.random_range(c.min_price..=c.max_price.max(c.min_price)) as i64;
        collection.add(&c.key);
        crate::inventory::give(commands, sim, crate::inventory::ItemKind::Find, c.key.clone(), c.name.clone(), 0, value, 1);
        return Some(if c.kind == CollectKind::Metal { c.name.clone() } else { a(&c.name) });
    }
    let list: &[&str] = match tier(worth, rng) {
        0 => &CHEAP,
        1 => &MEDIUM,
        _ => &HIGH,
    };
    let want = list[rng.random_range(0..list.len())].to_ascii_lowercase();
    let catalog = &data?.0.catalog;
    let c = catalog
        .iter()
        .find(|c| c.instance_name.eq_ignore_ascii_case(&want))
        .or_else(|| catalog.iter().find(|c| c.script.to_ascii_lowercase().ends_with(&format!(".{want}"))))?;
    let name = if c.name.trim().is_empty() { c.instance_name.clone() } else { c.name.trim().to_string() };
    crate::inventory::give(commands, sim, crate::inventory::ItemKind::Object, c.instance_name.clone(), name.clone(), 0, c.price.max(0) as i64, 1);
    Some(a(&name))
}

/// Each can drawn as it is: lying on its side lid off when knocked over, its lid off while it's
/// rummaged through, whole otherwise.
#[allow(clippy::type_complexity)]
fn show_can(
    mut commands: Commands,
    cans: Query<(Entity, &GameObject, Has<TippedOver>, Option<&CanLook>, Has<crate::objanim::HeldPose>, Option<&crate::objects::Design>)>,
    rummagers: Query<&Rummaging>,
    data: Option<Res<crate::baked::Baked>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(data) = data else { return };
    let open: HashSet<Entity> = rummagers.iter().map(|r| r.can).collect();
    for (e, o, tipped, look, held, design) in &cans {
        if o.kind != ObjectKind::OutdoorTrashCan {
            continue;
        }
        let want = if tipped || open.contains(&e) { CanLook::LidOff } else { CanLook::Whole };
        if look != Some(&want) {
            // (As it was first drawn, it's whole.)
            if look.is_some() || want != CanLook::Whole {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let state = (want == CanLook::LidOff).then(|| s3pkg::fnv32(BIN_ONLY));
                let parts = assets.object_state_design(&mut ctx, o.objd, state, design.map(|d| d.0));
                crate::objects::reshape_parts(&mut commands, e, &parts);
            }
            commands.entity(e).insert(want);
        }
        if tipped && !held {
            commands.entity(e).insert(crate::objanim::HeldPose(TIPPED_POSE));
        } else if !tipped && held {
            commands.entity(e).remove::<crate::objanim::HeldPose>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knocked_over_by_the_insane_the_nasty_away_from_home_and_enemies() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), Age::Adult);
        s.traits = vec![Trait::Friendly];
        assert!(!may_knock_over(&s, false, false));
        assert!(may_knock_over(&s, false, true));
        s.traits = vec![Trait::MeanSpirited];
        assert!(may_knock_over(&s, false, false) && !may_knock_over(&s, true, false));
        s.traits = vec![Trait::Insane];
        assert!(may_knock_over(&s, true, false));
    }

    #[test]
    fn rummaged_through_away_from_home_by_teens_and_up_once_a_day() {
        let mut s = crate::sim::random_sim(&mut rand::rng(), "T", Some(true), Age::Adult);
        s.traits = vec![Trait::Friendly];
        assert!(may_rummage(&s, false, true, false, false));
        assert!(!may_rummage(&s, true, true, false, false), "not their own");
        assert!(!may_rummage(&s, false, false, false, false), "nobody lives there");
        assert!(!may_rummage(&s, false, true, false, true), "once a day");
        s.traits = vec![Trait::Neat];
        assert!(!may_rummage(&s, false, true, true, false) && may_rummage(&s, false, true, false, false));
        s.traits = vec![Trait::Insane];
        assert!(may_rummage(&s, true, true, false, false));
        s.age = Age::Child;
        assert!(!may_rummage(&s, false, true, false, false));
    }

    #[test]
    fn the_well_off_throw_out_better_things() {
        let mut rng = rand::rng();
        assert!((0..200).all(|_| tier(10_000, &mut rng) == 0));
        let mid: Vec<usize> = (0..2000).map(|_| tier(50_000, &mut rng)).collect();
        assert!(mid.iter().all(|t| *t < 2) && mid.contains(&1) && mid.contains(&0));
        assert!((0..4000).map(|_| tier(200_000, &mut rng)).any(|t| t == 2));
        let mut r = Rummaged::default();
        r.mark(3, Vec3::new(1.0, 0.0, 2.0));
        assert!(r.today(3, Vec3::new(1.01, 0.0, 2.0)) && !r.today(4, Vec3::new(1.0, 0.0, 2.0)));
        let found: Vec<String> = ["a Trash", "a Newspaper", "a Newspaper", "a Trash", "an Unknown Rare Seed"].map(String::from).to_vec();
        assert_eq!(list(&found), "some trash, Newspaper ×2 and an Unknown Rare Seed");
    }
}
