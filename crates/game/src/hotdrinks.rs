//! Hot beverages, as the game's `HotBeverageMachine`: each Sim has a favourite drink, made up as
//! its `CustomDrinkRecipe` makes them ("Drink Double Banana Lite Latte with Chocolate Swirl"; the
//! Evil's "Evil", the Good's "Delicious", the Daredevils' "Extreme"), "Try Something Else..."
//! offers six more, and "Serve Hot Beverages" sets a tray of four cups of whatever it comes up
//! with out on the counter by the machine (`ServeHotBeverages`, `kServeMakesRandomDrinks`), for
//! anyone to take one from (`BarTray.GetDrink`) till it's empty, when it's cleaned up as dishes
//! are.

use bevy::prelude::*;
use rand::Rng;
use s3bake::gamedata::DrinkNames;

use crate::life::Trait;

/// The game's `kTryAnotherRecipeCount`, and the tray's cups (its four containment slots).
pub const TRY_ANOTHER: usize = 6;
pub const TRAY_CUPS: usize = 4;
/// The bar tray's slots for its cups (`ContainmentSlot_0`..., model space).
pub const CUP_SLOTS: [[f32; 3]; 4] = [[-0.09, 0.008, 0.0], [0.0, 0.008, -0.09], [0.09, 0.008, 0.0], [0.0, 0.008, 0.09]];
/// The game's objects for them.
pub const TRAY: &str = "barTray";
pub const CUP: &str = "CoffeeCup";

/// A drink's makings (`CustomDrinkRecipe`): its pattern, mix-in, cream, two flavours and tuple.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    style: usize,
    mixin: usize,
    cream: usize,
    flavor1: usize,
    flavor2: usize,
    tuple: usize,
}

impl Recipe {
    /// One made up, as its constructor does (each part's index inclusive of its last: the
    /// second flavour never none, nor the first).
    pub fn random(rng: &mut impl Rng, n: &DrinkNames) -> Self {
        let pick = |rng: &mut dyn rand::RngCore, len: usize| if len == 0 { 0 } else { rng.random_range(0..len) };
        let flavors = n.flavors.len().max(2);
        let flavor1 = pick(rng, flavors);
        let mut flavor2;
        loop {
            flavor2 = rng.random_range(1..flavors);
            if flavor2 != flavor1 {
                break;
            }
        }
        Self { style: pick(rng, n.patterns.len()), mixin: pick(rng, n.mixins.len()), cream: pick(rng, n.creams.len()), flavor1, flavor2, tuple: pick(rng, n.tuples.len()) }
    }

    /// Its name, as the game words it (`@` for a part left out, and no double spaces).
    pub fn name(&self, n: &DrinkNames) -> String {
        let part = |v: &[String], i: usize| v.get(i).cloned().unwrap_or_default();
        let fill = |s: &str, args: &[String]| {
            let mut s = s.to_string();
            for (i, a) in args.iter().enumerate() {
                s = s.replace(&format!("{{{i}.String}}"), a);
            }
            s
        };
        let mixin = fill(&part(&n.mixins, self.mixin), &[part(&n.flavors, self.flavor2)]);
        let args = [part(&n.tuples, self.tuple), part(&n.flavors, self.flavor1), part(&n.creams, self.cream), mixin];
        let name = fill(&part(&n.patterns, self.style), &args).replace('@', "");
        name.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// A Sim's favourite drink: theirs for good (made up from who they are).
pub fn favorite(n: &DrinkNames, sim_id: u64) -> String {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(sim_id ^ 0x486f_7442_6576_0001);
    Recipe::random(&mut rng, n).name(n)
}

/// Others to try: so many, none their favourite, none twice.
pub fn others(n: &DrinkNames, sim_id: u64, count: usize, rng: &mut impl Rng) -> Vec<String> {
    let fav = favorite(n, sim_id);
    let mut out: Vec<String> = Vec::new();
    for _ in 0..count * 20 {
        if out.len() >= count {
            break;
        }
        let name = Recipe::random(rng, n).name(n);
        if name != fav && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// Which of the interactions' wordings a Sim's: the Evil's, the Good's, the Daredevils', or plain.
fn wording(traits: &[Trait]) -> usize {
    if traits.contains(&Trait::Evil) {
        1
    } else if traits.contains(&Trait::Good) {
        2
    } else if traits.contains(&Trait::Daredevil) {
        3
    } else {
        0
    }
}

/// "Drink <name>", as the Sim would put it.
pub fn drink_label(n: &DrinkNames, traits: &[Trait], name: &str) -> String {
    let w = &n.drink[wording(traits)];
    if w.is_empty() { format!("Drink {name}") } else { w.replace("{0.String}", name) }
}

/// "Serve Hot Beverages", as the Sim would put it.
pub fn serve_label(n: &DrinkNames, traits: &[Trait]) -> String {
    let w = &n.serve[wording(traits)];
    if w.is_empty() { "Serve Hot Beverages".to_string() } else { w.clone() }
}

/// A tray of drinks set out: the cups left on it (their names, and their models).
#[derive(Component, Clone, Debug, Default)]
pub struct DrinkTray {
    pub cups: Vec<(String, Entity)>,
}

pub struct HotDrinksPlugin;

impl Plugin for HotDrinksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, trays.after(crate::interact::run_actions).run_if(in_state(crate::PlayMode::Live)));
    }
}

/// Hot beverages served at a machine: the tray to be set out by it.
#[derive(Component)]
pub struct ServedAt(pub Entity);

/// A drink taken from a tray.
#[derive(Component)]
pub struct TookDrink(pub Entity);

/// Whether a point's on top of a counter (or table): the one, and its top's height.
fn counter_under(objects: &Query<(Entity, &crate::interact::GameObject, &Transform), Without<DrinkTray>>, p: Vec3) -> Option<f32> {
    objects
        .iter()
        .filter(|(_, o, tf)| o.kind == crate::interact::ObjectKind::Table && (tf.translation.y + o.height - p.y).abs() < 0.3)
        .find(|(_, o, tf)| {
            let local = tf.compute_affine().inverse().transform_point3(p);
            (local.x - o.center.x).abs() <= o.half.x && (local.z - o.center.y).abs() <= o.half.y
        })
        .map(|(_, o, tf)| tf.translation.y + o.height)
}

/// Trays set out (four cups of whatever the machine comes up with, on the counter beside it),
/// cups taken from them, and the empty ones left for cleaning up.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn trays(
    mut commands: Commands,
    mut sims: Query<(Entity, Option<&ServedAt>, Option<&TookDrink>, &mut crate::interact::ActionQueue)>,
    sims_traits: Query<&crate::sim::Sim>,
    objects: Query<(Entity, &crate::interact::GameObject, &Transform), Without<DrinkTray>>,
    mut trays: Query<(&mut DrinkTray, &mut crate::interact::GameObject), Without<crate::sim::Sim>>,
    ui: Option<Res<crate::icons::GameUi>>,
    (data, catalog, mut assets): (Option<Res<crate::baked::Baked>>, Option<Res<crate::loading::Catalog>>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let mut emptied: Vec<Entity> = Vec::new();
    for (me, served, took, mut queue) in &mut sims {
        if let Some(t) = took {
            commands.entity(me).remove::<TookDrink>();
            match trays.get_mut(t.0) {
                Ok((mut tray, mut g)) if !tray.cups.is_empty() => {
                    let (name, cup) = tray.cups.remove(0);
                    commands.entity(cup).try_despawn();
                    info!("a cup of {name} taken from the tray");
                    // (Empty: to be cleaned up, as dishes are.)
                    if tray.cups.is_empty() {
                        g.kind = crate::interact::ObjectKind::DirtyDishes;
                        g.name = "Bar Tray".to_string();
                        emptied.push(t.0);
                    }
                }
                // (None left by the time they got there.)
                _ => {
                    if let Some(a) = queue.0.front_mut() {
                        a.cancel = true;
                    }
                }
            }
        }
        let Some(s) = served else { continue };
        commands.entity(me).remove::<ServedAt>();
        let (Some(ui), Some(data), Some(catalog)) = (ui.as_ref(), data.as_ref(), catalog.as_ref()) else { continue };
        let Ok((_, _, mtf)) = objects.get(s.0) else { continue };
        // (Beside the machine on the counter, either side, or before it: the tray's outer edge
        // on a counter too.)
        let right = mtf.rotation * Vec3::X;
        let ahead = mtf.rotation * Vec3::Z;
        let spots = [(right, 0.42), (-right, 0.42), (ahead, 0.2)];
        let Some((at, top)) = spots
            .iter()
            .map(|(dir, d)| (mtf.translation + *dir * *d, *dir))
            .find_map(|(p, dir)| counter_under(&objects, p).filter(|_| counter_under(&objects, p + dir * 0.14).is_some()).map(|top| (p, top)))
        else {
            info!("no room on the counter for the tray");
            continue;
        };
        let at = at.with_y(top);
        let yaw = mtf.rotation.to_euler(EulerRot::YXZ).0;
        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        let Some(tray) = crate::meals::spawn_dish(&mut commands, &mut assets, &mut ctx, catalog, TRAY, crate::interact::ObjectKind::DrinkTray, "Bar Tray", at, yaw) else { continue };
        let mut rng = rand::rng();
        let mut cups = Vec::new();
        for slot in CUP_SLOTS.iter().take(TRAY_CUPS) {
            let p = at + Quat::from_rotation_y(yaw) * Vec3::from(*slot);
            let name = Recipe::random(&mut rng, &ui.data.drinks).name(&ui.data.drinks);
            if let Some(cup) = crate::meals::spawn_dish(&mut commands, &mut assets, &mut ctx, catalog, CUP, crate::interact::ObjectKind::Decoration, "Coffee Cup", p, yaw) {
                commands.entity(cup).remove::<crate::interact::GameObject>();
                cups.push((name, cup));
            }
        }
        info!("hot beverages served: {:?}", cups.iter().map(|c| &c.0).collect::<Vec<_>>());
        // (The server has the first, as the game's has them.)
        if let Some((first, _)) = cups.first() {
            let traits = sims_traits.get(me).map(|s| s.traits.clone()).unwrap_or_default();
            let drink = crate::interact::Action::new(drink_label(&ui.data.drinks, &traits, first), crate::interact::ActionKind::Object { target: tray, def: 0 }, false);
            // (After what's under way, the tray taken from the machine too.)
            let at = queue.0.iter().take_while(|a| matches!(a.phase, crate::interact::Phase::Running(_)) || matches!(a.kind, crate::interact::ActionKind::Outro { .. })).count();
            queue.0.insert(at, drink);
        }
        commands.entity(tray).insert(DrinkTray { cups });
    }
    // (Those on their way to a drink from a tray that's run out: not now.)
    if !emptied.is_empty() {
        for (_, _, _, mut queue) in &mut sims {
            for a in queue.0.iter_mut().filter(|a| matches!(a.kind, crate::interact::ActionKind::Object { target, .. } if emptied.contains(&target)) && a.label.starts_with("Drink")) {
                a.cancel = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> DrinkNames {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        DrinkNames {
            patterns: v(&["{1.String} {2.String} Cappucino {3.String}", "{0.String} {1.String} {2.String} Latte {3.String}", "{2.String} Chai {3.String}"]),
            mixins: v(&["@", "with {0.String} Sprinkles", "with {0.String} Swirl"]),
            flavors: v(&["@", "White Chocolate", "Banana", "Strawberry", "Caramel", "Chocolate"]),
            creams: v(&["@", "Non-Fat", "Soy", "Lite"]),
            tuples: v(&["@", "Double", "Triple", "Half-Caf", ""]),
            plain: "Plain Coffee".into(),
            try_else: "Try Something Else...".into(),
            drink: ["Drink {0.String}".into(), "Drink Evil {0.String}".into(), "Drink Delicious {0.String}".into(), "Drink Extreme {0.String}".into()],
            serve: ["Serve Hot Beverages".into(), "Serve Evil Hot Beverages".into(), "Serve Delightful Hot Beverages".into(), "Serve Extremely Hot Beverages".into()],
        }
    }

    #[test]
    fn drinks_named_as_the_game_puts_them_together() {
        let n = names();
        let r = Recipe { style: 1, mixin: 2, cream: 3, flavor1: 2, flavor2: 5, tuple: 1 };
        assert_eq!(r.name(&n), "Double Banana Lite Latte with Chocolate Swirl");
        let plain = Recipe { style: 2, mixin: 0, cream: 0, flavor1: 0, flavor2: 1, tuple: 0 };
        assert_eq!(plain.name(&n), "Chai");
        assert_eq!(favorite(&n, 7), favorite(&n, 7));
        let more = others(&n, 7, TRY_ANOTHER, &mut rand::rng());
        assert_eq!(more.len(), TRY_ANOTHER);
        assert!(!more.contains(&favorite(&n, 7)));
        assert_eq!(drink_label(&n, &[Trait::Evil], "Chai"), "Drink Evil Chai");
        assert_eq!(serve_label(&n, &[Trait::Good]), "Serve Delightful Hot Beverages");
    }
}
