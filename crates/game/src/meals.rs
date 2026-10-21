//! Group meals, as the game serves them: whoever cooks puts the meal on a counter or table on
//! its serving platter, with a serving for each member of the household. Sims grab a plate, sit
//! down at a dining table to eat (the plate set on the table in front of them, a fork in hand)
//! or eat standing when there's no seat, and leave dirty dishes behind for someone to clear
//! away. The platter itself is left to wash up once the last serving is taken.
//!
//! What's cooked is one of the game's recipes (its `RecipeMasterList`): those a Sim knows (by
//! Cooking skill, or learned from a recipe book) for the time of day — breakfast, lunch or
//! dinner — or a dessert. The food is the game's own food models, set on the serving platter
//! and the plates: the dish full, then scraped clean once it's been eaten.

use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, CHAIR_EAT, GameObject, Notifications, ObjectKind, Special, UsedBy};
use crate::loading::Catalog;
use crate::objects::{AssetCtx, ObjectAssets};
use crate::sim::{Age, HouseholdMember, Sim};

pub struct MealsPlugin;

impl Plugin for MealsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Leftovers>()
            .init_resource::<FridgeKeeps>()
            .add_systems(Update, (fridge_keeps, stuffed).run_if(in_state(crate::PlayMode::Live)))
            .init_resource::<crate::ingredients::Fridge>()
            .add_systems(OnEnter(crate::AppState::InGame), |mut l: ResMut<Leftovers>, mut f: ResMut<crate::ingredients::Fridge>| {
                l.0.clear();
                l.1.clear();
                f.0 = None;
            })
            .add_systems(Update, crate::ingredients::stock_fridge.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, (restore_meals, cook_prep, cook_prep_done, serve_if_interrupted, take_out_dinner, meal_requests, update_platters, update_eating_plates, probe_platters, probe_meal_budget, come_to_meal, release_plates, learn_recipes, cut_cakes).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's walks with the ingredients' tray, and with the frying pan.
pub const FOOD_CARRY: &str = "a2o_foodTray_carry_x";
pub const PAN_CARRY: &str = "a2o_fryingPan_carry_x";
pub const PLATTER_CARRY: &str = "a2o_carryObject_carry_x";

/// A cook carrying the meal from this stove to set it down (then it's served).
#[derive(Component)]
pub struct ServingFrom(pub Entity);

/// A dinner heated in this microwave: to take out.
#[derive(Component)]
pub struct MicrowaveDone(pub Entity);

/// Heated, the dinner is taken out of the microwave (then eaten at the table): a Microwave
/// Meal, unless something else went in.
fn take_out_dinner(mut commands: Commands, mut sims: Query<(Entity, &MicrowaveDone, &mut ActionQueue, Has<MealPlan>)>, ui: Option<Res<crate::icons::GameUi>>) {
    for (e, m, mut queue, planned) in &mut sims {
        commands.entity(e).remove::<MicrowaveDone>();
        if !planned && let Some(r) = ui.as_ref().and_then(|u| u.data.recipes.iter().position(|r| r.snack && r.microwave)) {
            commands.entity(e).insert(MealPlan(r));
        }
        if let Some(def) = crate::interact::interactions_for(ObjectKind::Microwave).iter().position(|d| d.name == "Take Out Dinner") {
            queue.0.push_front(Action::new("Take Out Dinner", ActionKind::Object { target: m.0, def }, true));
        }
    }
}

/// A group meal just served: the household is called to it.
#[derive(Resource)]
pub struct MealCall {
    pub platter: Entity,
    pub cook: Entity,
    pub meal: String,
}

/// Called to a meal, the household comes to eat: whoever's hungry and not busy with something
/// they were told to do drops what they're doing for a plate, while there are servings.
#[allow(clippy::type_complexity)]
fn come_to_meal(
    mut commands: Commands,
    call: Option<Res<MealCall>>,
    mut sims: Query<
        (Entity, &Sim, &crate::sim::Motives, &mut ActionQueue, Option<&crate::visit::OnLot>),
        (With<HouseholdMember>, Without<crate::interact::AtWork>, Without<crate::interact::OffLot>, Without<crate::rabbitholes::AtRabbitHole>),
    >,
    meals: Query<&Meal>,
    mut notes: ResMut<Notifications>,
) {
    let Some(c) = call else { return };
    commands.remove_resource::<MealCall>();
    let Ok(m) = meals.get(c.platter) else { return };
    // (The cook has one.)
    let mut left = m.servings.saturating_sub(1);
    let mut coming = Vec::new();
    // (Those where the cook is: not those out visiting.)
    let lot = sims.get(c.cook).ok().map(|s| s.4.map(|l| l.0));
    for (e, sim, motives, mut queue, on) in &mut sims {
        if left == 0 || e == c.cook || sim.age.is_little() || motives.0[crate::sim::HUNGER] > 70.0 || queue.0.iter().any(|a| !a.autonomous) || lot.is_some_and(|l| l != on.map(|o| o.0)) {
            continue;
        }
        for a in queue.0.iter_mut() {
            a.cancel = true;
        }
        queue.0.push_back(Action::new("Grab a Plate", ActionKind::Object { target: c.platter, def: 0 }, true));
        coming.push(sim.first.clone());
        left -= 1;
    }
    if !coming.is_empty() {
        let cook = sims.get(c.cook).map(|s| s.1.first.clone()).unwrap_or_default();
        notes.push(format!("{cook} called everyone to {}: {} {} coming.", c.meal, coming.join(" and "), if coming.len() == 1 { "is" } else { "are" }));
    }
}

/// A meal carried off and not set down after all (the cook called away) is served anyway.
fn serve_if_interrupted(mut commands: Commands, sims: Query<(Entity, &ServingFrom, &ActionQueue), Without<MealRequest>>) {
    for (e, from, queue) in &sims {
        if !queue.current().is_some_and(|a| a.label == "Set Down Meal") {
            commands.entity(e).insert(MealRequest::Serve(from.0));
        }
    }
}

/// Whether a surface's top is within reach of someone standing at `at` (a floor): not a shelf
/// up on the wall.
fn within_reach(o: &GameObject, tf: &Transform, at: Vec3) -> bool {
    o.kind == ObjectKind::Table && (tf.translation.y - at.y).abs() < 1.5 && (0.3..1.4).contains(&(tf.translation.y + o.height - at.y))
}

/// A counter or table near a point (the one the meal is set down on).
fn surface_entity_near(objects: &Query<(Entity, &GameObject, &Transform, &UsedBy)>, at: Vec3, within: f32) -> Option<Entity> {
    objects
        .iter()
        .filter(|(_, o, tf, _)| within_reach(o, tf, at))
        .map(|(e, o, tf, _)| (e, o.world_center(tf).with_y(tf.translation.y).distance(at.with_y(tf.translation.y))))
        .filter(|(_, d)| *d < within)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e)
}

/// A cook whose meal is being prepared (ingredients from the fridge, chopped at a counter) on
/// the way to the stove.
#[derive(Component)]
pub struct CookPrepped;

/// Setting out to cook dinner at the stove, a cook first fetches the ingredients from the
/// nearest fridge and chops them at the nearest counter, as the game's cooks do.
#[allow(clippy::type_complexity)]
fn cook_prep(
    mut commands: Commands,
    data: Res<Baked>,
    mut sims: Query<(Entity, &Transform, &mut ActionQueue), Without<CookPrepped>>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy), Without<crate::interact::Broken>>,
) {
    for (e, tf, mut queue) in &mut sims {
        let Some(front) = queue.0.front_mut() else { continue };
        if !matches!(front.phase, crate::interact::Phase::Start | crate::interact::Phase::Routing) || front.cancel {
            continue;
        }
        let ActionKind::Object { target: stove, def } = front.kind else { continue };
        let Ok((_, obj, stf, _)) = objects.get(stove) else { continue };
        if obj.kind != ObjectKind::Stove || crate::interact::interactions_for(obj.kind).get(def).is_none_or(|d| d.special != Special::ServeMeal) {
            continue;
        }
        // (Not on its way to the stove yet: there's the fridge and a counter first.)
        front.phase = crate::interact::Phase::Start;
        commands.entity(e).insert(CookPrepped);
        let near = |want: &dyn Fn(&GameObject) -> bool| {
            objects
                .iter()
                .filter(|(_, o, t, used)| want(o) && used.0.is_none_or(|u| u == e) && (t.translation.y - stf.translation.y).abs() < 1.5 && t.translation.distance(stf.translation) < 15.0)
                .min_by(|a, b| a.2.translation.distance(stf.translation).total_cmp(&b.2.translation.distance(stf.translation)))
                .map(|(o, ..)| o)
        };
        let counter = near(&|o: &GameObject| o.kind == ObjectKind::Table && data.0.catalog_entry(&o.objd).is_some_and(|c| c.script.contains("Counter")));
        let fridge = near(&|o: &GameObject| o.kind == ObjectKind::Fridge);
        let def = |kind: ObjectKind, s: Special| crate::interact::interactions_for(kind).iter().position(|d| d.special == s);
        if let (Some(c), Some(d)) = (counter, def(ObjectKind::Table, Special::PrepFood)) {
            queue.0.push_front(Action::new("Prepare Food", ActionKind::Object { target: c, def: d }, true));
        }
        if let (Some(f), Some(d)) = (fridge, def(ObjectKind::Fridge, Special::GetIngredients)) {
            queue.0.push_front(Action::new("Get Ingredients", ActionKind::Object { target: f, def: d }, true));
        }
        let _ = tf;
    }
}

/// Once the cooking's done (or given up), the next meal is prepared afresh.
fn cook_prep_done(mut commands: Commands, sims: Query<(Entity, &ActionQueue), With<CookPrepped>>, objects: Query<&GameObject>) {
    for (e, queue) in &sims {
        if !queue.current().is_some_and(|a| {
            let ActionKind::Object { target, def } = a.kind else { return false };
            !a.cancel && objects.get(target).ok().and_then(|o| crate::interact::interactions_for(o.kind).get(def))
                .is_some_and(|d| matches!(d.special, Special::GetIngredients | Special::PrepFood | Special::ServeMeal))
        }) {
            commands.entity(e).remove::<CookPrepped>();
        }
    }
}

/// What a delivered pizza costs.
pub const PIZZA_PRICE: i64 = 30;

/// A pizza on its way.
#[derive(Resource)]
pub struct PizzaOrder {
    pub arrive_at: f64,
}

/// Where a delivered pizza goes: the game's pizza box, set down on the kitchen counter (by the
/// stove), or the surface nearest the curb, with a slice for everyone. (The delivery brings it:
/// see `services`.)
pub fn pizza_spot(objects: &Query<(Entity, &GameObject, &Transform, &UsedBy)>, curb: Vec3) -> Option<Vec3> {
    let near = objects.iter().find(|(_, o, _, _)| o.kind == ObjectKind::Stove).map(|(_, o, tf, _)| o.world_center(tf)).unwrap_or(curb);
    surface_near(objects, near, 12.0)
}

/// The finished recipe awaiting serving (index into the game's recipes).
/// The game's meal qualities (its `Quality`), worst to best.
pub const QUALITIES: [&str; 11] = ["Horrifying", "Putrid", "Foul", "Bad", "Neutral", "Nice", "Very Nice", "Great", "Excellent", "Outstanding", "Perfect"];
/// The food points a dish must be under for each quality (the Cooking skill's sheet,
/// `PointsToGet_*`; at or over the last, Perfect), and the meal moodlet's strength for each
/// (`Buff_*`; none below Neutral).
const POINTS_TO_GET: [f32; 11] = [-70.0, -50.0, -30.0, -10.0, 15.0, 30.0, 45.0, 60.0, 75.0, 100.0, 101.0];
const MEAL_STRENGTH: [i32; 11] = [0, 0, 0, 0, 10, 15, 20, 25, 30, 35, 50];
/// Food points for the recipe's level (`Level_N_Points`), more each time the cook has made it
/// (`Food_Point_Bonus_Per_Times_Made`), for a Natural Cook's or a Perfectionist's touch
/// (`TraitTuning`), a group meal's share (`Food.kQualityMultiplierForServedFood`), and a burnt
/// dish's (`Food.kNumFoodPointsBurnt`).
const LEVEL_POINTS: [f32; 11] = [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0];
const POINTS_PER_TIME_MADE: f32 = 1.5;
const NATURAL_COOK_POINTS: f32 = 15.0;
const PERFECTIONIST_POINTS: f32 = 15.0;
const GROUP_SHARE: f32 = 0.75;
const BURNT_POINTS: f32 = -150.0;
/// How long a meal's moodlet lasts (the Cooking tuning's `kBuffDuration`), and the strengths
/// it's Good, Great and Amazing at (`kBuffThreshold`).
const MEAL_MOODLET_MINUTES: f64 = 480.0;
const GREAT_MEAL: i32 = 25;
const AMAZING_MEAL: i32 = 40;

/// How good a cooked dish came out: its quality (an index of `QUALITIES`) and the strength of the
/// moodlet it gives.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MealQuality {
    pub quality: usize,
    pub strength: i32,
}

impl MealQuality {
    pub fn label(&self) -> &'static str {
        QUALITIES[self.quality.min(10)]
    }
}

/// What a cook brings to a dish.
pub struct Cook {
    pub times_made: f64,
    pub natural_cook: bool,
    pub perfectionist: bool,
    pub menu_maven: bool,
}

/// A dish's quality as the game's `SetQualityAndFoodPoints` reckons it: the recipe level's food
/// points, the cook's practice with it, what it's made of (`ingredients`: see
/// `ingredients::points`), the stove's own, the cook's touch, less for a group meal, nothing to
/// speak of if burnt; a Menu Maven's a quality better.
pub fn meal_quality(recipe_level: u8, cook: &Cook, ingredients: f32, stove_points: f32, group: bool, burnt: bool) -> MealQuality {
    let points = if burnt {
        BURNT_POINTS
    } else {
        let mut p = LEVEL_POINTS[recipe_level.min(10) as usize] + cook.times_made as f32 * POINTS_PER_TIME_MADE + ingredients + stove_points;
        if cook.natural_cook {
            p += NATURAL_COOK_POINTS;
        }
        if cook.perfectionist {
            p += PERFECTIONIST_POINTS;
        }
        if group { (p * GROUP_SHARE).trunc() } else { p.trunc() }
    };
    let mut quality = POINTS_TO_GET.iter().position(|&k| points < k).unwrap_or(10);
    let strength = MEAL_STRENGTH[quality];
    if !burnt && cook.menu_maven && quality < 10 {
        quality += 1;
    }
    MealQuality { quality, strength }
}

/// The moodlet a meal gives: Good, Great or Amazing by its strength.
pub fn meal_moodlet(strength: i32) -> Option<crate::life::MoodletKind> {
    use crate::life::MoodletKind as K;
    match strength {
        s if s >= AMAZING_MEAL => Some(K::AmazingMeal),
        s if s >= GREAT_MEAL => Some(K::GreatMeal),
        s if s > 0 => Some(K::GoodMeal),
        _ => None,
    }
}

/// A cook whose dish burnt at the stove, carrying it to the table.
#[derive(Component)]
pub struct BurntDinner;

/// What's on the plate a Sim's eating from, as it was cooked.
#[derive(Component, Clone, Copy)]
pub struct OnPlate(pub MealQuality);

/// Choices for queued cooking actions live on `Action::recipe_choice`.
#[derive(Component)]
pub struct MealPlan(pub usize);

/// What's cooking is for the cook alone ("Have Dinner"), not a group meal.
#[derive(Component)]
pub struct ForOne;

/// The recipe a platter holds (and so each plate from it).
#[derive(Component, Clone, Copy)]
pub struct Dish(pub usize);

/// The dish a Sim took a plate of.
#[derive(Component)]
struct Plateful(usize);

/// Recipes a Sim has learned from recipe books (by key).
#[derive(Component, Clone, Default, Debug)]
pub struct KnownRecipes(pub Vec<String>);

impl KnownRecipes {
    /// Returns whether a new recipe was bought; an unaffordable purchase changes nothing.
    fn purchase(&mut self, funds: &mut i64, key: &str, price: i64) -> Result<bool, ()> {
        if self.0.iter().any(|r| r == key) { return Ok(false); }
        if price < 0 || *funds < price { return Err(()); }
        *funds -= price;
        self.0.push(key.to_string());
        Ok(true)
    }
}

/// Buying a recipe book at the bookstore, and reading it there.
pub static BUY_RECIPE: crate::rabbitholes::Activity = crate::rabbitholes::Activity {
    name: "Buy a Recipe Book",
    minutes: 40.0,
    cost: 0,
    per_hour: [-4.0, -4.0, -2.0, 10.0, 0.0, 10.0],
    skill: Some("Cooking"),
    open: 9.0,
    close: 21.0,
};

/// Visit activity numbers for recipe books: this plus the recipe's index.
pub const RECIPE_TASK: usize = 2000;

pub fn recipe_task(activity: usize) -> Option<&'static crate::rabbitholes::Activity> {
    (RECIPE_TASK..RECIPE_TASK + 1000).contains(&activity).then_some(&BUY_RECIPE)
}

/// The recipe book a Sim has gone to buy.
#[derive(Component)]
pub struct BuyingRecipe(pub usize);

/// Back from the bookstore with it.
#[derive(Component)]
pub struct RecipeBookBought;

/// Whether a Sim knows a recipe: by skill, or from its book.
pub fn knows(r: &s3bake::gamedata::RecipeInfo, cooking: u32, known: Option<&KnownRecipes>) -> bool {
    (r.auto && r.level as u32 <= cooking) || known.is_some_and(|k| k.0.contains(&r.key))
}

/// The recipe books a Sim could learn from (those of recipes they don't know, up to their
/// skill), with their prices.
pub fn books_for(data: &s3bake::GameDataBaked, cooking: u32, known: Option<&KnownRecipes>) -> Vec<usize> {
    data.recipes.iter().enumerate().filter(|(_, r)| r.book_price > 0 && r.level as u32 <= cooking && !knows(r, cooking, known)).map(|(i, _)| i).collect()
}

/// A recipe book read: the recipe learned and paid for.
fn learn_recipes(
    mut commands: Commands,
    ui: Option<Res<crate::icons::GameUi>>,
    mut household: Option<ResMut<crate::interact::Household>>,
    mut sims: Query<(Entity, &Sim, &BuyingRecipe, Option<&mut KnownRecipes>, Option<&crate::wishes::Wishes>), With<RecipeBookBought>>,
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, buying, known, wishes) in &mut sims {
        commands.entity(e).remove::<(BuyingRecipe, RecipeBookBought)>();
        let Some(r) = ui.as_ref().and_then(|u| u.data.recipes.get(buying.0)) else { continue };
        let price = crate::wishes::book_price(wishes, r.book_price);
        let Some(h) = household.as_mut() else { continue };
        let mut updated = known.as_deref().cloned().unwrap_or_default();
        match updated.purchase(&mut h.funds, &r.key, price) {
            Ok(true) => { commands.entity(e).insert(updated); }
            Ok(false) => continue,
            Err(()) => {
                notes.push(format!("{} can't afford the recipe book for {} (§{price}).", sim.first, r.name));
                continue;
            }
        }
        notes.push(format!("{} bought a recipe book for §{price} and learned to make {}.", sim.first, r.name));
        let key = r.key.clone();
        commands.queue(move |w: &mut World| {
            w.write_message(crate::dreams::DreamEvent { key: Some(key), ..crate::dreams::DreamEvent::new(e, "kLearnedRecipe") });
        });
    }
}

/// What a birthday cake costs to bake.
pub const CAKE_PRICE: i64 = 20;

/// A birthday cake whose candles have been blown out: cut for everyone, and the household
/// cheers.
#[derive(Component)]
pub struct CakeBlownOut;

fn cut_cakes(
    mut commands: Commands,
    cakes: Query<(Entity, &Transform), With<CakeBlownOut>>,
    sims: Query<(Entity, &Transform, &ActionQueue), (With<HouseholdMember>, Without<crate::aging::GrowUpNow>)>,
) {
    for (cake, ctf) in &cakes {
        commands.entity(cake).remove::<CakeBlownOut>().insert(Meal { servings: 6 }).queue_silenced(|mut w: EntityWorldMut| {
            if let Some(mut g) = w.get_mut::<GameObject>() {
                g.kind = ObjectKind::Meal;
                g.name = "Birthday Cake".into();
            }
        });
        // Everyone close by with nothing else to do cheers.
        for (e, tf, q) in &sims {
            if q.0.is_empty() && tf.translation.distance(ctf.translation) < 8.0 {
                commands.entity(e).insert(crate::anim::ActionClip::new(None, &["a2o_birthdayCake_cheer_x"]));
            }
        }
    }
}

/// Whether a recipe's ingredients can all be bought (not those only found: a Lifefruit, a
/// Deathfish).
fn buyable(data: &s3bake::GameDataBaked, r: &s3bake::gamedata::RecipeInfo) -> bool {
    r.ingredients.iter().all(|i| crate::ingredients::cheapest(data, i).is_some())
}

/// The meal of the hour (`MEAL_*`) and its name, from the installed Food tuning.
/// Weekdays are Monday=0 through Sunday=6, as in GameClock.
pub fn meal_time(hour: f32, weekday: usize) -> (u8, &'static str) {
    use s3bake::gamedata::{MEAL_BREAKFAST, MEAL_BRUNCH, MEAL_DINNER, MEAL_LUNCH};
    if weekday >= 5 && (9.0..15.0).contains(&hour) {
        (MEAL_BRUNCH, "Brunch")
    } else if (1.0..11.0).contains(&hour) {
        (MEAL_BREAKFAST, "Breakfast")
    } else if (11.0..17.0).contains(&hour) {
        (MEAL_LUNCH, "Lunch")
    } else {
        (MEAL_DINNER, "Dinner")
    }
}

/// The recipes a Sim knows for a meal time, whether or not what they take can be bought (the
/// food menus offer those they've the makings of).
pub fn known_for(data: &s3bake::GameDataBaked, sim: &Sim, cooking: u32, known: Option<&KnownRecipes>, meal: u8) -> Vec<usize> {
    let veg = sim.traits.contains(&crate::life::Trait::Vegetarian);
    data.recipes
        .iter()
        .enumerate()
        .filter(|(_, r)| r.meals & meal != 0 && (!veg || r.vegetarian))
        .filter(|(_, r)| knows(r, cooking, known))
        .map(|(i, _)| i)
        .collect()
}

/// The recipes a Sim can cook for a meal time (vegetarians, the meatless ones).
pub fn cookable(data: &s3bake::GameDataBaked, sim: &Sim, cooking: u32, known: Option<&KnownRecipes>, meal: u8) -> Vec<usize> {
    let veg = sim.traits.contains(&crate::life::Trait::Vegetarian);
    data.recipes
        .iter()
        .enumerate()
        .filter(|(_, r)| r.meals & meal != 0 && (!veg || r.vegetarian))
        .filter(|(_, r)| knows(r, cooking, known))
        .filter(|(_, r)| buyable(data, r))
        .map(|(i, _)| i)
        .collect()
}

/// The food on a plate or platter (its model, a child of the dish).
#[derive(Component)]
struct DishFood(Entity);

/// Sets one of the recipes' food models on a dish, in place of any food already there.
fn set_food(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, dish: Entity, old: Option<&DishFood>, model: Option<s3bake::Key>) {
    if let Some(f) = old {
        commands.entity(f.0).try_despawn();
        commands.entity(dish).remove::<DishFood>();
    }
    let Some(model) = model else { return };
    let parts = assets.model(ctx, model);
    if parts.is_empty() {
        return;
    }
    let food = crate::objects::spawn_parts(commands, &parts, Transform::IDENTITY);
    commands.entity(food).insert(ChildOf(dish));
    commands.entity(dish).insert(DishFood(food));
}

/// What a Sim has just done with food (set by the interactions, handled here).
#[derive(Component, Clone, Copy, Debug)]
pub enum MealRequest {
    /// Finished cooking at this stove.
    Serve(Entity),
    /// A quick meal out of the fridge, to eat at the table.
    Quick,
    /// Baked a birthday cake (at this fridge).
    Cake(Entity),
    /// Took a serving from this platter.
    Grabbed(Entity),
    /// Put what's left on this platter in the fridge.
    PutAway(Entity),
    /// Took a plate of leftovers from the fridge.
    FromFridge,
    /// The food replicator made them a plate of something.
    Replicated,
    /// Sat down to eat at this dining chair.
    PlateAt(Entity),
    /// Finished eating at the table.
    Ate,
    /// Finished eating standing.
    AteStanding,
}

/// `Food_0xa1104c038b529738.xml`: kNumServingsForGroupMeal.
const GROUP_MEAL_SERVINGS: u8 = 8;

/// A group meal's servings left.
#[derive(Component)]
pub struct Meal {
    pub servings: u8,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SavedMeal {
    pub recipe: String,
    #[serde(default)]
    pub object: Option<s3bake::Key>,
    #[serde(default)]
    pub name: String,
    pub servings: u8,
    pub position: [f32; 3],
    pub yaw: f32,
    /// Absolute Sim time, so loading does not refresh food left out.
    #[serde(default)]
    pub served_at: Option<f64>,
    #[serde(default)]
    pub spoiled: bool,
}

#[derive(Resource)]
pub struct PendingMeals(pub Vec<SavedMeal>);

pub type SavableMeals<'w, 's> = Query<'w, 's, (Option<&'static Meal>, Option<&'static Dish>, &'static GameObject, &'static Transform, Option<&'static crate::surroundings::ServedAt>, Has<crate::surroundings::Spoiled>), Or<(With<Meal>, With<crate::surroundings::Spoiled>)>>;

pub fn saved_meals(meals: &SavableMeals, data: &s3bake::GameDataBaked) -> Vec<SavedMeal> {
    meals.iter().filter_map(|(meal, dish, object, tf, served, spoiled)| {
        let recipe = dish.and_then(|d| data.recipes.get(d.0));
        let servings = meal.map_or(0, |m| m.servings);
        (servings > 0 || spoiled).then(|| SavedMeal { recipe: recipe.map_or_else(String::new, |r| r.key.clone()), object: Some(object.objd), name: object.name.clone(), servings,
            position: tf.translation.to_array(), yaw: tf.rotation.to_euler(EulerRot::YXZ).0, served_at: served.map(|s| s.0), spoiled })
    }).collect()
}

#[allow(clippy::too_many_arguments)]
fn restore_meals(
    mut commands: Commands, pending: Option<Res<PendingMeals>>, ui: Option<Res<crate::icons::GameUi>>,
    data: Res<Baked>, catalog: Res<Catalog>, mut assets: ResMut<ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(pending), Some(ui)) = (pending, ui) else { return };
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    for saved in &pending.0 {
        if saved.servings == 0 && !saved.spoiled { continue; }
        let recipe = ui.data.recipes.iter().enumerate().find(|(_, r)| r.key == saved.recipe);
        let plate = if let Some(objd) = saved.object {
            let Some(spawned) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, objd, Vec3::from(saved.position), Quat::from_rotation_y(saved.yaw)) else { continue };
            commands.entity(spawned.entity).remove::<crate::nav::Obstacle>();
            spawned.entity
        } else {
            // Older meal saves only stored the recipe and used a standard serving platter.
            if recipe.is_none() { continue; }
            let Some(plate) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATTER, ObjectKind::Meal, "Group Meal", Vec3::from(saved.position), saved.yaw) else { continue };
            plate
        };
        if let Some((i, recipe)) = recipe {
            set_food(&mut commands, &mut assets, &mut ctx, plate, None, recipe.group.or(recipe.single));
            commands.entity(plate).insert(Dish(i));
        }
        if saved.spoiled {
            commands.entity(plate).insert(crate::surroundings::Spoiled);
        } else {
            commands.entity(plate).insert(Meal { servings: saved.servings });
        }
        if let Some(at) = saved.served_at.filter(|at| at.is_finite()) {
            commands.entity(plate).insert(crate::surroundings::ServedAt(at));
        }
        let name = if saved.name.is_empty() { recipe.map_or_else(|| "Group Meal".into(), |(_, r)| r.name.clone()) } else { saved.name.clone() };
        let kind = if saved.spoiled { ObjectKind::DirtyDishes } else { ObjectKind::Meal };
        commands.entity(plate).queue_silenced(move |mut e: EntityWorldMut| {
            if let Some(mut object) = e.get_mut::<GameObject>() { object.name = name; object.kind = kind; }
        });
    }
    commands.remove_resource::<PendingMeals>();
}

impl Meal {
    fn take_serving(&mut self) -> bool {
        let Some(left) = self.servings.checked_sub(1) else { return false };
        self.servings = left;
        true
    }

    fn take_remaining(&mut self) -> u8 {
        std::mem::take(&mut self.servings)
    }
}

#[derive(Component)]
struct HalfPlatter;

#[derive(Component)]
struct HalfPlate;

fn half_eaten(queue: &ActionQueue) -> bool {
    let Some(action) = queue.0.front() else { return false };
    if action.cancel || !matches!(action.kind, ActionKind::Object { def, .. } if def == CHAIR_EAT) { return false; }
    let crate::interact::Phase::Running(elapsed) = action.phase else { return false };
    let duration = crate::interact::interactions_for(ObjectKind::Chair)[CHAIR_EAT].minutes;
    // Food tuning: a full serving has 15 bites; the half-full model starts at 7 left.
    elapsed >= duration * (8.0 / 15.0)
}

#[allow(clippy::too_many_arguments)]
fn update_eating_plates(
    mut commands: Commands, ui: Option<Res<crate::icons::GameUi>>,
    diners: Query<(&EatingPlate, &Plateful, &ActionQueue)>,
    plates: Query<&DishFood, Without<HalfPlate>>,
    data: Res<Baked>, mut assets: ResMut<ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(ui) = ui else { return };
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    for (plate, dish, queue) in &diners {
        if !half_eaten(queue) { continue; }
        let Ok(food) = plates.get(plate.0) else { continue };
        if let Some(half) = ui.data.recipes.get(dish.0).and_then(|r| r.single_half) {
            set_food(&mut commands, &mut assets, &mut ctx, plate.0, Some(food), Some(half));
            commands.entity(plate.0).insert(HalfPlate);
        }
    }
}

/// Update once after all plate requests, so simultaneous diners cannot queue conflicting
/// replacements for the same food child. Food tuning uses four remaining servings.
fn update_platters(
    mut commands: Commands,
    ui: Option<Res<crate::icons::GameUi>>,
    plates: Query<(Entity, &Meal, &Dish, Option<&DishFood>), (Changed<Meal>, Without<HalfPlatter>)>,
    data: Res<Baked>,
    mut assets: ResMut<ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(ui) = ui else { return };
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    for (entity, meal, dish, food) in &plates {
        if !(1..=4).contains(&meal.servings) { continue; }
        if let Some(half) = ui.data.recipes.get(dish.0).and_then(|r| r.group_half) {
            set_food(&mut commands, &mut assets, &mut ctx, entity, food, Some(half));
            commands.entity(entity).insert(HalfPlatter);
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn probe_platters(
    mut commands: Commands,
    time: Res<Time<crate::autotest::InputTimeline>>,
    ui: Option<Res<crate::icons::GameUi>>,
    data: Res<Baked>, catalog: Res<Catalog>, mut assets: ResMut<ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut materials: ResMut<Assets<StandardMaterial>>,
    tables: Query<(&GameObject, &Transform)>,
    mut plates: Query<(&mut Meal, &DishFood, Has<HalfPlatter>), With<DishFood>>,
    generic: Query<(&GameObject, &Meal), Without<DishFood>>,
    entities: Query<Entity>,
    mut state: Local<(u8, Option<Entity>, Option<Entity>)>,
    (mut single, single_food, mut queues): (Local<Option<(Entity, Entity, Entity)>>, Query<(&DishFood, Has<HalfPlate>)>, Query<&mut ActionQueue>),
) {
    if std::env::var_os("HALF_MEAL_TEST").is_none() || state.0 == 4 || time.elapsed_secs() < 6.0 { return; }
    let Some(ui) = ui else { return };
    if std::env::var("HALF_MEAL_TEST").as_deref() == Ok("restore") {
        let (meal, food, half) = plates.iter().find(|(meal, _, _)| meal.servings == 4).expect("saved four-serving platter restored");
        assert_eq!(meal.servings, 4);
        assert!(half && entities.contains(food.0), "restored meal must retain half-depleted geometry");
        let pizza = generic.iter().find(|(o, _)| o.name == "Pizza").expect("non-recipe pizza restored");
        assert_eq!(pizza.1.servings, 3);
        assert_eq!(data.0.catalog_entry(&pizza.0.objd).unwrap().instance_name, "FoodPizza");
        info!("autotest: saved platter PASS — cooked platter and non-recipe pizza retain servings, names and original models");
        state.0 = 4;
        return;
    }
    match state.0 {
        0 => {
            let (i, recipe) = ui.data.recipes.iter().enumerate().find(|(_, r)| r.group.is_some() && r.group_half.is_some() && r.group != r.group_half && r.single.is_some() && r.single_half.is_some() && r.single != r.single_half).expect("original recipe with distinct half models");
            let (table, tf) = tables.iter().find(|(o, _)| o.kind == ObjectKind::Table).expect("fixture dining surface");
            let at = table.world_center(tf).with_y(tf.translation.y + table.height);
            let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
            let plate = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATTER, ObjectKind::Meal, "Platter probe", at, 0.0).expect("platter model");
            set_food(&mut commands, &mut assets, &mut ctx, plate, None, recipe.group);
            commands.entity(plate).insert((Meal { servings: GROUP_MEAL_SERVINGS }, Dish(i)));
            let pizza = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, "FoodPizza", ObjectKind::Meal, "Pizza", at + Vec3::X * 0.5, 0.0).expect("pizza model");
            commands.entity(pizza).insert(Meal { servings: 3 });
            let dish = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATE, ObjectKind::DirtyDishes, "Plate probe", at - Vec3::X * 0.5, 0.0).expect("individual plate model");
            set_food(&mut commands, &mut assets, &mut ctx, dish, None, recipe.single);
            let mut queue = ActionQueue::default();
            let mut action = Action::new("Eat", ActionKind::Object { target: dish, def: CHAIR_EAT }, true);
            action.phase = crate::interact::Phase::Running(0.0);
            queue.0.push_back(action);
            let diner = commands.spawn((EatingPlate(dish), Plateful(i), queue)).id();
            *single = Some((diner, dish, Entity::PLACEHOLDER));
            state.1 = Some(plate);
            state.0 = 1;
        }
        1 => {
            let (mut meal, food, half) = plates.get_mut(state.1.unwrap()).expect("full platter rendered");
            assert!(!half);
            assert!(entities.contains(food.0));
            state.2 = Some(food.0);
            meal.servings = 4;
            let (diner, dish, _) = single.unwrap();
            let (food, half) = single_food.get(dish).expect("full individual plate rendered");
            assert!(!half);
            *single = Some((diner, dish, food.0));
            queues.get_mut(diner).unwrap().0[0].phase = crate::interact::Phase::Running(15.0);
            state.0 = 2;
        }
        2 => {
            let (_, food, half) = plates.get(state.1.unwrap()).unwrap();
            assert!(half, "four servings must select half-depleted geometry");
            assert_ne!(Some(food.0), state.2);
            assert!(!entities.contains(state.2.unwrap()), "old food mesh must be removed");
            assert!(entities.contains(food.0), "half-depleted mesh must exist");
            state.2 = Some(food.0);
            let (diner, dish, old) = single.unwrap();
            let (food, half) = single_food.get(dish).unwrap();
            assert!(half && entities.contains(food.0));
            assert_ne!(food.0, old);
            assert!(!entities.contains(old));
            *single = Some((diner, dish, food.0));
            state.0 = 3;
        }
        3 => {
            let (_, food, half) = plates.get(state.1.unwrap()).unwrap();
            assert!(half);
            assert_eq!(Some(food.0), state.2, "unchanged serving count must retain its mesh");
            let (_, dish, expected) = single.unwrap();
            assert_eq!(single_food.get(dish).unwrap().0.0, expected);
            info!("autotest: half platter PASS — group and individual full/half models rendered, old meshes removed, replacements retained");
            state.0 = 4;
        }
        _ => unreachable!(),
    }
}

fn probe_meal_budget(
    mut commands: Commands, time: Res<Time<crate::autotest::InputTimeline>>,
    ui: Option<Res<crate::icons::GameUi>>, funds: Option<ResMut<crate::interact::Household>>,
    objects: Query<(Entity, &GameObject)>, mut selected: Query<(Entity, &mut ActionQueue), With<crate::sim::Selected>>,
    notes: Res<Notifications>, mut state: Local<(u8, usize)>, fridge: Res<crate::ingredients::Fridge>,
) {
    if std::env::var_os("MEAL_BUDGET_TEST").is_none() || state.0 == 3 || time.elapsed_secs() < 6.0 { return; }
    let (Some(ui), Some(mut funds), Ok((sim, mut queue))) = (ui, funds, selected.single_mut()) else { return };
    let count = objects.iter().filter(|(_, o)| matches!(o.kind, ObjectKind::Meal | ObjectKind::BirthdayCake)).count();
    match state.0 {
        0 => {
            funds.funds = 0;
            queue.0.clear();
            let stove = objects.iter().find(|(_, o)| o.kind == ObjectKind::Stove).expect("fixture stove").0;
            // (One whose ingredients have to be bought.)
            let recipe = ui
                .data
                .recipes
                .iter()
                .position(|r| crate::ingredients::plan(&ui.data, &r.ingredients, None, fridge.stock()).is_some_and(|p| p.cost > 0))
                .expect("paid recipe");
            commands.entity(sim).insert((MealPlan(recipe), ServingFrom(stove), MealRequest::Serve(stove)));
            state.1 = count;
            state.0 = 1;
        }
        1 => {
            assert_eq!(funds.funds, 0);
            assert_eq!(count, state.1, "unaffordable cooking must not create food");
            assert!(notes.0.iter().any(|(n, _)| n.contains("can't afford the ingredients")));
            let fridge = objects.iter().find(|(_, o)| o.kind == ObjectKind::Fridge).expect("fixture fridge").0;
            commands.entity(sim).insert(MealRequest::Cake(fridge));
            state.0 = 2;
        }
        2 => {
            assert_eq!(funds.funds, 0);
            assert_eq!(count, state.1, "unaffordable cake must not be created");
            assert!(notes.0.iter().any(|(n, _)| n.contains("can't afford a birthday cake")));
            info!("autotest: meal budget PASS — cooking and cakes reject insufficient funds without spawning food or creating debt");
            state.0 = 3;
        }
        _ => unreachable!(),
    }
}

/// How many servings of leftovers the fridge keeps.
const MAX_LEFTOVERS: usize = 12;

/// Servings of meals put away in the fridge (by recipe, oldest first), to be had another time,
/// and when each was put away.
#[derive(Resource, Default, Clone, Debug)]
pub struct Leftovers(pub Vec<String>, pub Vec<f64>);

/// How long food keeps out of a fridge (`PreparedFood.kBaseTimeToNextFoodSpoilageStage`, tuned:
/// hours), and in one by how good a fridge it is (each fridge's tuned `SpoilageMultiplier`: the
/// cheap one 7, the moderate 12, the expensive 21, and the reward fridge's food never spoils).
const SPOIL_HOURS: f64 = 10.0;
fn spoilage_multiplier(script: &str) -> f64 {
    let s = script.to_ascii_lowercase();
    if s.ends_with(".fridgecheap") {
        7.0
    } else if s.ends_with(".fridgemoderate") {
        12.0
    } else if s.ends_with(".fridgeexpensive") {
        21.0
    } else if s.ends_with(".fridgereward") {
        f64::INFINITY
    } else {
        10.0
    }
}

/// How long the household's leftovers keep (game minutes): by their best fridge.
#[derive(Resource, Clone, Copy, Debug)]
pub struct FridgeKeeps(pub f64);

impl Default for FridgeKeeps {
    fn default() -> Self {
        Self(SPOIL_HOURS * 60.0 * 10.0)
    }
}

impl Leftovers {
    /// A serving put away.
    pub fn put(&mut self, key: String, at: f64) {
        self.1.resize(self.0.len(), at);
        self.0.push(key);
        self.1.push(at);
    }

    /// A serving taken out (its recipe's key).
    pub fn take(&mut self, i: usize) -> String {
        self.1.resize(self.0.len(), 0.0);
        self.1.remove(i);
        self.0.remove(i)
    }

    /// The oldest let go, past what the fridge holds.
    fn keep_last(&mut self, n: usize) {
        self.1.resize(self.0.len(), 0.0);
        let over = self.0.len().saturating_sub(n);
        self.0.drain(..over);
        self.1.drain(..over);
    }

    /// Whether a serving's gone off (kept longer than the fridge keeps food).
    pub fn spoiled(&self, i: usize, now: f64, keeps: f64) -> bool {
        self.1.get(i).is_some_and(|at| now - at > keeps)
    }

    /// Whether any has.
    pub fn any_spoiled(&self, now: f64, keeps: f64) -> bool {
        (0..self.0.len()).any(|i| self.spoiled(i, now, keeps))
    }

    /// The spoiled ones thrown out: how many.
    pub fn clean_out(&mut self, now: f64, keeps: f64) -> usize {
        self.1.resize(self.0.len(), now);
        let before = self.0.len();
        let keep: Vec<bool> = (0..before).map(|i| !self.spoiled(i, now, keeps)).collect();
        let mut k = keep.iter();
        self.0.retain(|_| *k.next().unwrap_or(&true));
        let mut k = keep.iter();
        self.1.retain(|_| *k.next().unwrap_or(&true));
        before - self.0.len()
    }
}

/// How long the household's leftovers keep, by the best fridge on the lot.
fn fridge_keeps(
    time: Res<Time>,
    mut last: Local<f32>,
    mut keeps: ResMut<FridgeKeeps>,
    fridges: Query<&GameObject, Without<crate::visit::LotObject>>,
    data: Option<Res<crate::baked::Baked>>,
) {
    let now = time.elapsed_secs();
    if now - *last < 5.0 {
        return;
    }
    *last = now;
    let Some(data) = data else { return };
    let best = fridges
        .iter()
        .filter(|o| o.kind == ObjectKind::Fridge)
        .filter_map(|o| data.0.catalog.iter().find(|c| c.objd == o.objd).map(|c| spoilage_multiplier(&c.script)))
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
    keeps.0 = SPOIL_HOURS * 60.0 * best.unwrap_or(10.0);
}

/// The plate in front of a Sim eating at a table.
#[derive(Component)]
struct EatingPlate(Entity);

/// The game's own objects for the platter and plates.
const PLATTER: &str = "PlateServing";
pub const PLATE: &str = "Plate";

/// A surface (table or counter) near a point: its top centre.
fn surface_near(objects: &Query<(Entity, &GameObject, &Transform, &UsedBy)>, at: Vec3, within: f32) -> Option<Vec3> {
    objects
        .iter()
        .filter(|(_, o, tf, _)| within_reach(o, tf, at))
        .map(|(_, o, tf, _)| {
            let c = o.world_center(tf);
            (Vec3::new(c.x, tf.translation.y + o.height, c.z), c.with_y(tf.translation.y).distance(at.with_y(tf.translation.y)))
        })
        .filter(|(_, d)| *d < within)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p)
}

/// A free dining chair (a chair with a table in front of it) near the Sim, and the point on the
/// table where their plate goes.
fn dining_seat(objects: &Query<(Entity, &GameObject, &Transform, &UsedBy)>, near: Vec3, taken: &[Entity]) -> Option<(Entity, Vec3)> {
    // (A picnic table's nearest place too: the place itself is taken on the way to it.)
    let picnic = objects.iter().filter(|(e, o, _, _)| o.kind == ObjectKind::PicnicTable && !taken.contains(e)).filter_map(|(e, o, tf, _)| {
        let (plate, seat) = picnic_plate(o, tf, near)?;
        Some((e, plate, Vec3::new(seat.x, tf.translation.y, seat.y).distance(near)))
    });
    objects
        .iter()
        .filter(|(e, o, _, used)| matches!(o.kind, ObjectKind::Chair | ObjectKind::Stool) && used.0.is_none() && !taken.contains(e))
        .filter_map(|(e, o, tf, _)| {
            let seat = o.world_center(tf);
            let (_, plate) = dining_seat_for(objects, e)?;
            Some((e, plate, seat.distance(near)))
        })
        .chain(picnic)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(e, p, _)| (e, p))
}

/// Where a plate goes at a picnic table, for the place nearest a point: on the table top before
/// it (and the place's seat).
fn picnic_plate(o: &GameObject, tf: &Transform, near: Vec3) -> Option<(Vec3, Vec2)> {
    let near = Vec2::new(near.x, near.z);
    let (stand, seat) = GameObject::picnic_places(tf).into_iter().min_by(|a, b| a.0.distance(near).total_cmp(&b.0.distance(near)))?;
    let p = seat + (seat - stand).normalize_or_zero() * 0.55;
    Some((Vec3::new(p.x, tf.translation.y + o.height, p.y), seat))
}

/// Spawns one of the game's objects (by internal name) as an object of `kind`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_dish(
    commands: &mut Commands,
    assets: &mut ObjectAssets,
    ctx: &mut AssetCtx,
    catalog: &Catalog,
    name: &str,
    kind: ObjectKind,
    label: &'static str,
    at: Vec3,
    yaw: f32,
) -> Option<Entity> {
    let objd = ctx.baked.catalog.iter().find(|c| c.instance_name == name)?.objd;
    let spawned = crate::home::spawn_game_object_rot(commands, assets, ctx, catalog, objd, at, Quat::from_rotation_y(yaw))?;
    let e = spawned.entity;
    commands.entity(e).remove::<crate::nav::Obstacle>().queue_silenced(move |mut w: EntityWorldMut| {
        if let Some(mut g) = w.get_mut::<GameObject>() {
            g.kind = kind;
            g.name = label.to_string();
        }
    });
    Some(e)
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn meal_requests(
    mut commands: Commands,
    mut sims: Query<(
        Entity,
        &MealRequest,
        &Transform,
        &mut ActionQueue,
        &Sim,
        Option<&EatingPlate>,
        (Option<&MealPlan>, Option<&Plateful>, &crate::interact::Skills, Option<&KnownRecipes>, Option<&ServingFrom>, Has<ForOne>),
        (Option<&crate::journal::SkillJournal>, Has<BurntDinner>, Option<&OnPlate>, Option<&crate::life::Moodlets>),
    )>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    mut meals: Query<(&mut Meal, Option<&Dish>, Option<&DishFood>)>,
    foods: Query<&DishFood>,
    household: Query<&Sim, With<HouseholdMember>>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<Notifications>,
    (ui, clock, mut funds, mut leftovers): (Option<Res<crate::icons::GameUi>>, Res<crate::clock::GameClock>, Option<ResMut<crate::interact::Household>>, ResMut<Leftovers>),
    (mut did, mut dreams): (MessageWriter<crate::journal::Did>, MessageWriter<crate::dreams::DreamEvent>),
    (qualities, floors, mut fire, rates, upgrades): (Query<&MealQuality>, Query<&crate::nav::Floor>, MessageWriter<crate::fire::StartFire>, Res<crate::interact::ObjectRates>, Query<&crate::upgrades::Upgrades>),
    (mut inventories, mut fridge): (Query<&mut crate::inventory::Inventory>, ResMut<crate::ingredients::Fridge>),
) {
    use rand::Rng;
    let recipes = ui.as_ref().map(|u| u.data.clone());
    let recipe = |i: usize| recipes.as_ref().and_then(|d| d.recipes.get(i));
    let mut taken: Vec<Entity> = Vec::new();
    for (me, req, tf, mut queue, sim, eating, (plan, plateful, skills, known, serving, one), (journal, burnt_before, on_plate, moodlets)) in &mut sims {
        commands.entity(me).remove::<MealRequest>();
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        // Ambrosia eaten: divine (`ambrosia`).
        if matches!(req, MealRequest::Ate | MealRequest::AteStanding) && plateful.and_then(|p| recipe(p.0)).is_some_and(|r| r.key == crate::ambrosia::AMBROSIA) {
            commands.entity(me).insert(crate::ambrosia::AteAmbrosia);
        }
        // A favourite meal is satisfying whether eaten at a table or standing up.
        if matches!(req, MealRequest::Ate | MealRequest::AteStanding)
            && let Some(r) = plateful.and_then(|p| recipe(p.0))
            && r.key == sim.favorites.food
        {
            let name = r.name.clone();
            commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| {
                let now = e.world().resource::<crate::clock::GameClock>().minutes;
                if let Some(mut m) = e.get_mut::<crate::life::Moodlets>() {
                    m.add(crate::life::MoodletKind::AmazingMeal, now);
                }
                let first = e.get::<Sim>().map(|s| s.first.clone()).unwrap_or_default();
                e.world_scope(|w| w.resource_mut::<crate::interact::Notifications>().push(format!("{first} loves {name}: their favourite!")));
            });
        }
        // (The game's `kAteMeal`: what, and how good.)
        if matches!(req, MealRequest::Ate | MealRequest::AteStanding)
            && let Some(r) = plateful.and_then(|p| recipe(p.0))
        {
            dreams.write(crate::dreams::DreamEvent { key: Some(r.key.clone()), quality: on_plate.map_or(0, |p| p.0.quality as i32 + 1), ..crate::dreams::DreamEvent::new(me, "kAteMeal") });
        }
        match *req {
            MealRequest::Serve(stove) => {
                let Ok((_, s, stf, _)) = objects.get(stove) else { continue };
                // The recipe chosen, or the best they know for the time of day.
                let (meal, word) = meal_time(clock.hour_f(), clock.weekday());
                let grill = s.kind == ObjectKind::Grill;
                let dish = plan.map(|p| p.0).or_else(|| {
                    let d = recipes.as_ref()?;
                    let mut options = cookable(d, sim, skills.level("Cooking"), known, if grill { 0xFF } else { meal });
                    // (A grill cooks its own menu.)
                    if grill {
                        options.retain(|&i| crate::appliances::GRILL_RECIPES.contains(&d.recipes[i].key.as_str()));
                    }
                    options.sort_by_key(|&i| std::cmp::Reverse(d.recipes[i].level));
                    options.truncate(3);
                    options.choose(&mut rand::rng()).copied()
                });
                let stove_tuning = rates.0.get(&s.objd).and_then(|o| o.cooking.as_ref());
                // (At the stove, whether it burns, or sets the stove alight: the game's odds for
                // the cook, the recipe and the stove. A burnt dinner is still served.)
                let mut burnt = burnt_before;
                if serving.is_none() {
                    let natural = sim.traits.contains(&crate::life::Trait::NaturalCook);
                    let level = dish.and_then(recipe).map_or(0, |r| r.level as u32);
                    let mut rng = rand::rng();
                    // (FIRE_ON_COOK=1: every dinner sets the stove alight, a test.)
                    let forced = std::env::var("FIRE_ON_COOK").is_ok();
                    // (Feeling Lucky, never; Unlucky, twice as likely: `traitmoods::luck`.)
                    use crate::traitmoods::{BURN_LUCK, COOKING_FIRE_LUCK, luck};
                    let burns = crate::fire::burn_chance(skills.level("Cooking"), level, natural, stove_tuning.map_or(1.0, |t| t.failure)) * luck(moodlets, BURN_LUCK.0, BURN_LUCK.1);
                    if forced || rng.random_bool(burns.min(1.0)) {
                        // (Never a fireproof stove: "Make Fireproof".)
                        let fireproof = crate::upgrades::has(upgrades.get(stove).ok(), crate::upgrades::Upgrade::Fireproof);
                        let fire_after = stove_tuning.map_or(5.0, |t| t.fire) as f64 / 100.0 * luck(moodlets, COOKING_FIRE_LUCK.0, COOKING_FIRE_LUCK.1);
                        if !fireproof && (forced || rng.random_bool(fire_after.clamp(0.0, 1.0))) {
                            fire.write(crate::fire::StartFire { at: stf.translation + Vec3::Y * s.height * 0.85, level: floors.get(stove).map_or(1, |f| f.0) });
                            notes.push(format!("{} set the stove on fire!", sim.first));
                            commands.entity(me).remove::<(MealPlan, BurntDinner)>();
                            continue;
                        }
                        burnt = true;
                    }
                    // (Kept for when it's set down.)
                    if let Some(d) = dish {
                        commands.entity(me).insert(MealPlan(d));
                    }
                    if burnt {
                        commands.entity(me).insert(BurntDinner);
                    }
                }
                // (First carried from the stove to the counter or table it's served on; a plate
                // for one, to the table.)
                if serving.is_none()
                    && !one
                    && let Some(surface) = surface_entity_near(&objects, s.world_center(stf), 6.0)
                    && let Some(def) = crate::interact::interactions_for(ObjectKind::Table).iter().position(|d| d.special == Special::PlaceMeal)
                {
                    commands.entity(me).insert((ServingFrom(stove), crate::anim::Carrying(PLATTER_CARRY)));
                    queue.0.push_front(Action::new("Set Down Meal", ActionKind::Object { target: surface, def }, true));
                    continue;
                }
                commands.entity(me).remove::<ServingFrom>();
                let at = surface_near(&objects, s.world_center(stf), 6.0).unwrap_or(Vec3::new(s.world_center(stf).x, stf.translation.y + s.height, s.world_center(stf).z));
                let yaw = stf.rotation.to_euler(EulerRot::YXZ).0;
                commands.entity(me).remove::<(MealPlan, BurntDinner)>();
                let r = dish.and_then(recipe);
                // (Ambrosia's set out, a single serving, for whoever's to eat it: `ambrosia`.)
                let set_out = r.is_some_and(|r| r.key == crate::ambrosia::AMBROSIA);
                let servings = if set_out { 1 } else { GROUP_MEAL_SERVINGS };
                let label = r.map_or("Group Meal".to_string(), |r| r.name.clone());
                // How it came out.
                let cook = Cook {
                    times_made: r.and_then(|r| journal.map(|j| j.times_made(&r.key))).unwrap_or(0.0),
                    natural_cook: sim.traits.contains(&crate::life::Trait::NaturalCook),
                    perfectionist: sim.traits.contains(&crate::life::Trait::Perfectionist),
                    menu_maven: crate::journal::earned(journal, "Menu Maven"),
                };
                // The ingredients: the cook's own, the fridge's, the rest bought there and then.
                let ingredients = r.map(|r| recipes.as_deref().and_then(|d| crate::ingredients::plan(d, &r.ingredients, inventories.get(me).ok(), fridge.stock())));
                if let Some(None) = ingredients {
                    notes.push(format!("{} doesn't have the ingredients for {label}.", sim.first));
                    commands.entity(me).remove::<(MealPlan, BurntDinner)>();
                    continue;
                }
                let ingredients = ingredients.flatten();
                // (A stove with "Improve Meal Quality": its meals' bonus points a fifth more.)
                let better = if crate::upgrades::has(upgrades.get(stove).ok(), crate::upgrades::Upgrade::Improved) { crate::upgrades::IMPROVED_MEAL } else { 1.0 };
                let quality = r.map(|r| meal_quality(r.level, &cook, ingredients.as_ref().map_or(0.0, |p| p.points) * better, stove_tuning.map_or(0.0, |t| t.bonus_points) * better, !one, burnt));
                let served = match (r, quality) {
                    (Some(r), Some(_)) if burnt => format!("burnt {}", r.name),
                    (Some(r), Some(q)) => format!("{} {}", q.label(), r.name),
                    _ => label.clone(),
                };
                let cost = ingredients.as_ref().map_or(0, |p| p.cost);
                if cost > 0 && funds.as_ref().is_none_or(|h| h.funds < cost) {
                    notes.push(format!("{} can't afford the ingredients for {label} (§{cost}).", sim.first));
                    continue;
                }
                if let Some(p) = &ingredients {
                    crate::ingredients::use_up(p, inventories.get_mut(me).ok().as_deref_mut(), &mut fridge);
                    if let Some(h) = funds.as_mut() {
                        h.funds -= cost;
                    }
                }
                if set_out {
                    commands.entity(me).remove::<ForOne>();
                }
                if one && !set_out {
                    commands.entity(me).remove::<ForOne>();
                    did.write(crate::journal::Did::count(me, crate::journal::Stat::Dishes, 1.0));
                    if let Some(r) = r {
                        did.write(crate::journal::Did::made(me, r.key.clone()));
                        dreams.write(crate::dreams::DreamEvent { key: Some(r.key.clone()), quality: quality.as_ref().map_or(0, |q| q.quality as i32 + 1), ..crate::dreams::DreamEvent::new(me, "kCookedMeal") });
                    }
                    info!("{} made {served} for one", sim.first);
                    match dish {
                        Some(d) => commands.entity(me).insert(Plateful(d)),
                        None => commands.entity(me).remove::<Plateful>(),
                    };
                    match quality {
                        Some(q) => commands.entity(me).insert(OnPlate(q)),
                        None => commands.entity(me).remove::<OnPlate>(),
                    };
                    commands.entity(me).insert(crate::anim::Carrying(crate::surroundings::DISH_CARRY));
                    match dining_seat(&objects, tf.translation, &taken) {
                        Some((chair, _)) => {
                            taken.push(chair);
                            queue.0.push_front(Action::new("Eat", ActionKind::Object { target: chair, def: CHAIR_EAT }, true));
                        }
                        None => queue.0.push_front(Action::new("Eat", ActionKind::EatHere, true)),
                    }
                    continue;
                }
                let Some(platter) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATTER, ObjectKind::Meal, "Group Meal", at, yaw) else { continue };
                did.write(crate::journal::Did::count(me, crate::journal::Stat::Dishes, 1.0));
                if let Some(r) = r {
                    did.write(crate::journal::Did::made(me, r.key.clone()));
                    // (The game's `kCookedMeal`, served to the group.)
                    dreams.write(crate::dreams::DreamEvent { key: Some(r.key.clone()), quality: quality.as_ref().map_or(0, |q| q.quality as i32 + 1), group: true, ..crate::dreams::DreamEvent::new(me, "kCookedMeal") });
                }
                if let Some(q) = quality {
                    commands.entity(platter).insert(q);
                }
                // (Hot dogs and burgers have no platter of their own: a plate of them.)
                set_food(&mut commands, &mut assets, &mut ctx, platter, None, r.and_then(|r| r.group.or(r.single)));
                commands.entity(platter).insert(Meal { servings });
                if r.is_some() {
                    let name = label.clone();
                    commands.entity(platter).queue_silenced(move |mut w: EntityWorldMut| {
                        if let Some(mut g) = w.get_mut::<GameObject>() {
                            g.name = name;
                        }
                    });
                }
                if let Some(d) = dish {
                    commands.entity(platter).insert(Dish(d));
                }
                info!("meal served at {at:.1?}: {served}");
                let dish_word = if r.is_some_and(|r| r.meals == s3bake::gamedata::MEAL_DESSERT) { "Dessert" } else { word };
                if set_out {
                    notes.push(format!("{} made {served}. It's set out, for whoever is to eat it.", sim.first));
                    continue;
                }
                notes.push(match r {
                    Some(_) => format!("{} made {served} for {servings}. {dish_word} is served!", sim.first),
                    None => format!("{} made {} for {servings}. {dish_word} is served!", sim.first, dish_word.to_lowercase()),
                });
                // The cook eats too; first, with more than their own, calling everyone to it (the
                // game's wave over).
                queue.0.push_front(Action::new("Grab a Plate", ActionKind::Object { target: platter, def: 0 }, true));
                if servings > 1 && household.contains(me) {
                    const CALL: &[&str] = &["a_soc_callOver_x"];
                    queue.0.push_front(Action::new("Call to Meal", ActionKind::Outro { clips: CALL, then: None, secs: 2.5, stand_at: None, target: platter }, true));
                    commands.entity(me).insert(crate::anim::ActionClip::new(Some(CALL[0]), &[]));
                    commands.insert_resource(MealCall { platter, cook: me, meal: dish_word.to_lowercase() });
                }
            }
            MealRequest::Cake(fridge) => {
                // On the nearest counter or table, candles lit.
                let Ok((_, f, ftf, _)) = objects.get(fridge) else { continue };
                if funds.as_ref().is_none_or(|h| h.funds < CAKE_PRICE) {
                    notes.push(format!("{} can't afford a birthday cake (§{CAKE_PRICE}).", sim.first));
                    continue;
                }
                let at = surface_near(&objects, f.world_center(ftf), 8.0).unwrap_or(tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 0.6));
                if spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, "FoodBirthdayCake", ObjectKind::BirthdayCake, "Birthday Cake", at, 0.0).is_some() {
                    if let Some(h) = funds.as_mut() {
                        h.funds -= CAKE_PRICE;
                    }
                    notes.push(format!("{} baked a birthday cake (§{CAKE_PRICE}). Who'll blow out the candles?", sim.first));
                }
            }
            MealRequest::Grabbed(platter) => {
                if let Ok((mut m, dish, food)) = meals.get_mut(platter) {
                    if !m.take_serving() { continue; }
                    match dish {
                        Some(d) => commands.entity(me).insert(Plateful(d.0)),
                        None => commands.entity(me).remove::<Plateful>(),
                    };
                    match qualities.get(platter) {
                        Ok(q) => commands.entity(me).insert(OnPlate(*q)),
                        Err(_) => commands.entity(me).remove::<OnPlate>(),
                    };
                    if m.servings == 0 {
                        // The empty platter waits to be washed up (what's left of the dish on it).
                        let emptied = dish.and_then(|d| recipe(d.0)).and_then(|r| r.group_empty);
                        set_food(&mut commands, &mut assets, &mut ctx, platter, food, emptied);
                        commands.entity(platter).remove::<Meal>().queue_silenced(|mut w: EntityWorldMut| {
                            if let Some(mut g) = w.get_mut::<GameObject>() {
                                g.kind = ObjectKind::DirtyDishes;
                                g.name = "Dirty Dishes".into();
                            }
                        });
                    }
                } else { continue; }
                // (The plate carried to the table.)
                commands.entity(me).insert(crate::anim::Carrying(crate::surroundings::DISH_CARRY));
                match dining_seat(&objects, tf.translation, &taken) {
                    Some((chair, at)) => {
                        info!("{} takes a plate to the table at {:.1?}", sim.first, at);
                        taken.push(chair);
                        queue.0.push_front(Action::new("Eat", ActionKind::Object { target: chair, def: CHAIR_EAT }, true));
                    }
                    None => {
                        info!("{} eats standing at {:.1?}", sim.first, tf.translation);
                        queue.0.push_front(Action::new("Eat", ActionKind::EatHere, true));
                    }
                }
            }
            MealRequest::PutAway(platter) => {
                // What's left goes in the fridge; the platter with it.
                if let Ok((mut m, dish, _)) = meals.get_mut(platter) {
                    // Empty immediately: entity removal is deferred until after all requests.
                    let servings = m.take_remaining();
                    if let Some(r) = dish.and_then(|d| recipe(d.0)) {
                        for _ in 0..servings {
                            leftovers.put(r.key.clone(), clock.minutes);
                        }
                        // (The fridge holds a dozen; the oldest go out.)
                        leftovers.keep_last(MAX_LEFTOVERS);
                    }
                    commands.entity(platter).try_despawn();
                }
            }
            MealRequest::Quick => {
                // (The snack chosen, or one of those out of the fridge; failing those, something
                // simple of what they know, for the time of day.)
                commands.entity(me).remove::<MealPlan>();
                let pick = plan.map(|p| p.0).or_else(|| {
                    let d = recipes.as_ref()?;
                    let snacks: Vec<usize> = (0..d.recipes.len()).filter(|&i| d.recipes[i].snack && !d.recipes[i].microwave).collect();
                    if let Some(&i) = snacks.choose(&mut rand::rng()) {
                        return Some(i);
                    }
                    let mut options = cookable(d, sim, skills.level("Cooking"), known, meal_time(clock.hour_f(), clock.weekday()).0);
                    options.sort_by_key(|&i| d.recipes[i].level);
                    options.truncate(3);
                    options.choose(&mut rand::rng()).copied()
                });
                match pick {
                    Some(i) => commands.entity(me).insert(Plateful(i)),
                    None => commands.entity(me).remove::<Plateful>(),
                };
                commands.entity(me).remove::<OnPlate>();
                commands.entity(me).insert(crate::anim::Carrying(crate::surroundings::DISH_CARRY));
                match dining_seat(&objects, tf.translation, &taken) {
                    Some((chair, _)) => {
                        taken.push(chair);
                        queue.0.push_front(Action::new("Eat", ActionKind::Object { target: chair, def: CHAIR_EAT }, true));
                    }
                    None => queue.0.push_front(Action::new("Eat", ActionKind::EatHere, true)),
                }
            }
            MealRequest::Replicated => {
                // (Any dish there is, but those made of what's only found.)
                use rand::seq::IndexedRandom;
                let pick = recipes.as_ref().and_then(|d| {
                    let ok: Vec<usize> = (0..d.recipes.len()).filter(|&i| !d.recipes[i].snack && buyable(d, &d.recipes[i])).collect();
                    ok.choose(&mut rand::rng()).copied()
                });
                match pick {
                    Some(i) => commands.entity(me).insert(Plateful(i)),
                    None => commands.entity(me).remove::<Plateful>(),
                };
                commands.entity(me).remove::<OnPlate>();
                // (Carried off to the table.)
                commands.entity(me).insert(crate::anim::Carrying(crate::surroundings::DISH_CARRY));
                match dining_seat(&objects, tf.translation, &taken) {
                    Some((chair, _)) => {
                        taken.push(chair);
                        queue.0.push_front(Action::new("Eat", ActionKind::Object { target: chair, def: CHAIR_EAT }, true));
                    }
                    None => queue.0.push_front(Action::new("Eat", ActionKind::EatHere, true)),
                }
            }
            MealRequest::FromFridge => {
                // (The dish chosen; else the oldest first.)
                commands.entity(me).remove::<MealPlan>();
                if leftovers.0.is_empty() {
                    continue;
                }
                let chosen = plan.and_then(|p| recipe(p.0)).and_then(|r| leftovers.0.iter().position(|k| *k == r.key));
                let key = leftovers.take(chosen.unwrap_or(0));
                match recipes.as_ref().and_then(|d| d.recipes.iter().position(|r| r.key == key)) {
                    Some(i) => commands.entity(me).insert(Plateful(i)),
                    None => commands.entity(me).remove::<Plateful>(),
                };
                commands.entity(me).remove::<OnPlate>();
                match dining_seat(&objects, tf.translation, &taken) {
                    Some((chair, at)) => {
                        info!("{} takes leftovers to the table at {:.1?}", sim.first, at);
                        taken.push(chair);
                        queue.0.push_front(Action::new("Eat", ActionKind::Object { target: chair, def: CHAIR_EAT }, true));
                    }
                    None => queue.0.push_front(Action::new("Eat", ActionKind::EatHere, true)),
                }
            }
            MealRequest::PlateAt(chair) => {
                info!("{} sits down to eat", sim.first);
                let Ok((_, co, ctf, _)) = objects.get(chair) else { continue };
                // (At a picnic table, before the place sat at.)
                let picnic = (co.kind == ObjectKind::PicnicTable).then(|| picnic_plate(co, ctf, tf.translation)).flatten();
                let Some(at) = picnic.map(|p| p.0).or_else(|| dining_seat_for(&objects, chair).map(|d| d.1)) else { continue };
                let yaw = match picnic {
                    Some((plate, seat)) => {
                        let to = Vec2::new(plate.x, plate.z) - seat;
                        to.x.atan2(to.y)
                    }
                    None => ctf.rotation.to_euler(EulerRot::YXZ).0,
                };
                if let Some(plate) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATE, ObjectKind::DirtyDishes, "Dirty Dishes", at, yaw) {
                    set_food(&mut commands, &mut assets, &mut ctx, plate, None, plateful.and_then(|p| recipe(p.0)).and_then(|r| r.single));
                    // (Not to be cleared away while it's being eaten from.)
                    commands.entity(plate).insert(UsedBy(Some(me)));
                    commands.entity(me).insert(EatingPlate(plate));
                }
            }
            MealRequest::Ate => {
                info!("{} finished eating", sim.first);
                eat_quality(&mut commands, me, on_plate, plateful.and_then(|p| recipe(p.0)).is_some_and(|r| r.key == sim.favorites.food));
                if let Some(p) = eating {
                    commands.entity(me).remove::<EatingPlate>();
                    commands.entity(p.0).insert(UsedBy(None));
                    // The plate, eaten clean.
                    let empty = plateful.and_then(|p| recipe(p.0)).and_then(|r| r.single_empty);
                    set_food(&mut commands, &mut assets, &mut ctx, p.0, foods.get(p.0).ok(), empty);
                    queue_cleanup(sim, &mut queue, p.0, rand::random());
                }
                commands.entity(me).remove::<Plateful>();
            }
            MealRequest::AteStanding => {
                eat_quality(&mut commands, me, on_plate, plateful.and_then(|p| recipe(p.0)).is_some_and(|r| r.key == sim.favorites.food));
                // The plate goes on the nearest surface, or the floor.
                let at = surface_near(&objects, tf.translation, 4.0).unwrap_or(tf.translation + tf.rotation * Vec3::new(0.3, 0.0, 0.4));
                if let Some(plate) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATE, ObjectKind::DirtyDishes, "Dirty Dishes", at, 0.0) {
                    set_food(&mut commands, &mut assets, &mut ctx, plate, None, plateful.and_then(|p| recipe(p.0)).and_then(|r| r.single_empty));
                    queue_cleanup(sim, &mut queue, plate, rand::random());
                }
                commands.entity(me).remove::<Plateful>();
            }
        }
    }
}

/// The game's `BuffStuffed.MotiveValueForStuffed` (its tuning's 55): eating begun as full as this
/// leaves a Sim Stuffed.
const STUFFED_HUNGER: f32 = 55.0;

/// How hungry a Sim was as they began eating.
#[derive(Component)]
pub struct HungerAtMeal(f32);

/// Eating when not hungry: Stuffed once done (a meal, leftovers, a snack), as the game's
/// `Morsel` adds it.
fn stuffed(mut commands: Commands, clock: Res<crate::clock::GameClock>, mut sims: Query<(Entity, &ActionQueue, &crate::sim::Motives, Option<&HungerAtMeal>, &mut crate::life::Moodlets)>) {
    for (e, q, m, at, mut ml) in &mut sims {
        let eats = |a: &Action| a.label == "Eat" || matches!(a.kind, ActionKind::EatHere | ActionKind::EatItem { .. });
        let eating = q.0.front().is_some_and(|a| matches!(a.phase, crate::interact::Phase::Running(_)) && eats(a));
        // (A meal broken off a moment (someone talking to them) is still the same meal.)
        let queued = q.0.iter().any(eats);
        match (eating, queued, at) {
            (true, _, None) => {
                debug!("eating begun at hunger {:.0}", m.0[crate::sim::HUNGER]);
                commands.entity(e).insert(HungerAtMeal(m.0[crate::sim::HUNGER]));
            }
            (false, false, Some(h)) => {
                if h.0 >= STUFFED_HUNGER {
                    info!("ate when not hungry (hunger {:.0}): Stuffed", h.0);
                    ml.add(crate::life::MoodletKind::Stuffed, clock.minutes);
                }
                commands.entity(e).remove::<HungerAtMeal>();
            }
            _ => {}
        }
    }
}

/// A meal eaten: its moodlet, Good, Great or Amazing as it was cooked (a favourite's Amazing
/// already).
fn eat_quality(commands: &mut Commands, me: Entity, on_plate: Option<&OnPlate>, favourite: bool) {
    commands.entity(me).remove::<OnPlate>();
    let Some(q) = on_plate.map(|p| p.0) else { return };
    let Some(kind) = meal_moodlet(q.strength).filter(|_| !favourite) else { return };
    debug!("a {} meal: {kind:?} +{}", q.label(), q.strength);
    commands.entity(me).queue_silenced(move |mut e: EntityWorldMut| {
        let now = e.world().resource::<crate::clock::GameClock>().minutes;
        if let Some(mut m) = e.get_mut::<crate::life::Moodlets>() {
            use crate::life::MoodletKind as K;
            for k in [K::GoodMeal, K::GreatMeal, K::AmazingMeal] {
                m.remove(k);
            }
            m.add_valued(kind, now, q.strength, MEAL_MOODLET_MINUTES);
        }
    });
}

/// Food tuning's cleanup chance, with trait behavior and player queue precedence.
fn queue_cleanup(sim: &Sim, queue: &mut ActionQueue, plate: Entity, roll: f32) {
    use crate::life::Trait;
    if sim.traits.contains(&Trait::Slob)
        || (!sim.traits.contains(&Trait::Neat) && roll >= 0.5)
        || queue.0.iter().any(|a| !a.autonomous || !matches!(a.kind, ActionKind::Outro { .. }))
    { return; }
    if let Some(def) = crate::interact::interactions_for(ObjectKind::DirtyDishes).iter().position(|d| d.special == Special::CleanUp) {
        queue.0.push_back(Action::new("Clean Up", ActionKind::Object { target: plate, def }, true));
    }
}

/// The plate spot for a particular chair (whether or not it's free).
fn dining_seat_for(objects: &Query<(Entity, &GameObject, &Transform, &UsedBy)>, chair: Entity) -> Option<(Entity, Vec3)> {
    let (_, o, tf, _) = objects.get(chair).ok()?;
    let seat = o.world_center(tf);
    let ahead = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
    let front = seat + ahead * 0.55;
    let top = objects
        .iter()
        .filter(|(_, t, _, _)| t.kind == ObjectKind::Table)
        .filter_map(|(_, t, ttf, _)| {
            let c = t.world_center(ttf);
            let d = front.with_y(0.0).distance(c.with_y(0.0));
            dining_table_top(t, ttf, front, tf.translation.y).map(|top| (top, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))?
        .0;
    Some((chair, Vec3::new(front.x, top, front.z)))
}

fn dining_table_top(table: &GameObject, tf: &Transform, front: Vec3, chair_base: f32) -> Option<f32> {
    let local = tf.compute_affine().inverse().transform_point3(front).xz() - table.center;
    // Allow a small reach beyond the edge, but never use the long dimension as a
    // radius: rectangular and rotated tables must actually be in front of the chair.
    if local.x.abs() > table.half.x + 0.15 || local.y.abs() > table.half.y + 0.15 { return None; }
    let top = tf.transform_point(Vec3::new(table.center.x, table.height, table.center.y)).y;
    ((top - chair_base - 0.7).abs() < 0.35).then_some(top)
}

/// A Sim who stopped eating early leaves their plate to be cleared.
fn release_plates(mut commands: Commands, sims: Query<(Entity, &ActionQueue, &EatingPlate), Without<MealRequest>>) {
    for (me, queue, plate) in &sims {
        let eating = queue.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Object { def, .. } if def == CHAIR_EAT));
        if !eating {
            commands.entity(plate.0).try_insert(UsedBy(None));
            commands.entity(me).remove::<EatingPlate>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leftovers_go_off_by_the_fridge_and_are_cleaned_out() {
        let mut l = Leftovers::default();
        l.put("Pancakes".into(), 0.0);
        l.put("Lasagna".into(), 60.0 * 60.0);
        // (A cheap fridge keeps food seventy hours; the expensive one three times that.)
        let cheap = SPOIL_HOURS * 60.0 * spoilage_multiplier("Sims3.Gameplay.Objects.Appliances.FridgeCheap");
        let dear = SPOIL_HOURS * 60.0 * spoilage_multiplier("Sims3.Gameplay.Objects.Appliances.FridgeExpensive");
        assert_eq!(cheap, 70.0 * 60.0);
        let now = 80.0 * 60.0;
        assert!(l.spoiled(0, now, cheap) && !l.spoiled(1, now, cheap) && !l.any_spoiled(now, dear));
        assert!(spoilage_multiplier("Sims3.Gameplay.Objects.Appliances.FridgeReward").is_infinite());
        assert_eq!(l.clean_out(now, cheap), 1);
        assert_eq!(l.0, vec!["Lasagna".to_string()]);
        assert_eq!(l.take(0), "Lasagna");
        assert!(l.0.is_empty() && l.1.is_empty());
    }

    #[test]
    fn meals_come_out_as_the_game_cooks_them() {
        use crate::life::MoodletKind as K;
        let novice = Cook { times_made: 0.0, natural_cook: false, perfectionist: false, menu_maven: false };
        // (An easy recipe made for the first time, on a plain stove: Neutral, a Good Meal.)
        let q = meal_quality(0, &novice, 0.0, 0.0, false, false);
        assert_eq!((q.label(), q.strength, meal_moodlet(q.strength)), ("Neutral", 10, Some(K::GoodMeal)));
        // (Made of a perfect tomato: better.)
        let fresh = meal_quality(0, &novice, crate::ingredients::points(9), 0.0, false, false);
        assert!(fresh.quality > q.quality, "{}", fresh.label());
        // (The hardest, made twenty times before, on the best stove: Perfect, an Amazing Meal.)
        let practised = Cook { times_made: 20.0, ..novice };
        let best = meal_quality(10, &practised, 0.0, 20.0, false, false);
        assert_eq!((best.label(), meal_moodlet(best.strength)), ("Perfect", Some(K::AmazingMeal)));
        assert_eq!(meal_quality(10, &practised, 0.0, 20.0, true, false).label(), "Outstanding", "a group meal's share is three quarters");
        let burnt = meal_quality(5, &practised, 0.0, 20.0, false, true);
        assert_eq!((burnt.label(), meal_moodlet(burnt.strength)), ("Horrifying", None));
        let maven = meal_quality(0, &Cook { menu_maven: true, ..novice }, 0.0, 0.0, false, false);
        assert_eq!((maven.label(), maven.strength), ("Nice", 10), "a quality better, as strong as it was cooked");
        assert_eq!(meal_moodlet(25), Some(K::GreatMeal));
    }

    #[test]
    fn half_eaten_visual_starts_at_the_original_remaining_bite_threshold() {
        let mut world = World::new();
        let chair = world.spawn_empty().id();
        let mut queue = ActionQueue::default();
        let mut action = Action::new("Eat", ActionKind::Object { target: chair, def: CHAIR_EAT }, true);
        let threshold = crate::interact::interactions_for(ObjectKind::Chair)[CHAIR_EAT].minutes * 8.0 / 15.0;
        action.phase = crate::interact::Phase::Running(threshold - 0.01);
        queue.0.push_back(action);
        assert!(!half_eaten(&queue));
        queue.0[0].phase = crate::interact::Phase::Running(threshold + 0.01);
        assert!(half_eaten(&queue));
        queue.0[0].cancel = true;
        assert!(!half_eaten(&queue));
        queue.0.clear();
        assert!(!half_eaten(&queue));
    }

    #[test]
    fn named_recipe_keeps_preparation_until_cancelled_or_target_disappears() {
        let mut app = App::new();
        app.add_systems(Update, cook_prep_done);
        let stove = app.world_mut().spawn(GameObject { kind: ObjectKind::Stove, name: "Stove".into(), objd: (0, 0, 0), price: 0,
            center: Vec2::ZERO, half: Vec2::ONE, height: 1.0, route: None }).id();
        let def = crate::interact::interactions_for(ObjectKind::Stove).iter().position(|d| d.special == Special::ServeMeal).unwrap();
        let mut queue = ActionQueue::default();
        queue.push_player(Action::new("Cook: Pancakes", ActionKind::Object { target: stove, def }, false));
        let sim = app.world_mut().spawn((CookPrepped, queue)).id();
        app.update();
        assert!(app.world().get::<CookPrepped>(sim).is_some(), "a named recipe must not repeatedly restart ingredient preparation");
        app.world_mut().get_mut::<ActionQueue>(sim).unwrap().0[0].cancel = true;
        app.update();
        assert!(app.world().get::<CookPrepped>(sim).is_none());
        app.world_mut().get_mut::<ActionQueue>(sim).unwrap().0[0].cancel = false;
        app.world_mut().entity_mut(sim).insert(CookPrepped);
        app.world_mut().despawn(stove);
        app.update();
        assert!(app.world().get::<CookPrepped>(sim).is_none());
    }

    #[test]
    fn dining_reach_uses_rotated_table_bounds_instead_of_longest_radius() {
        let table = GameObject { kind: ObjectKind::Table, name: "Long table".into(), objd: (0, 0, 0), price: 0,
            center: Vec2::new(0.3, -0.2), half: Vec2::new(1.5, 0.4), height: 0.75, route: None };
        for yaw in [0.0, std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_4, -std::f32::consts::FRAC_PI_4] {
            let tf = Transform::from_xyz(2.0, 3.0, 4.0).with_rotation(Quat::from_rotation_y(yaw));
            let over = tf.transform_point(Vec3::new(1.3, 0.0, -0.2));
            assert_eq!(dining_table_top(&table, &tf, over, 3.0), Some(3.75));
            let beside = tf.transform_point(Vec3::new(0.3, 0.0, 0.8));
            assert!(dining_table_top(&table, &tf, beside, 3.0).is_none(), "empty space beside the narrow edge is not tabletop (yaw {yaw})");
            assert!(dining_table_top(&table, &tf, over, 0.0).is_none());
        }
    }

    #[test]
    fn saved_platters_preserve_recipe_identity_servings_and_transform() {
        let mut data = s3bake::GameDataBaked::default();
        data.recipes.push(s3bake::gamedata::RecipeInfo { key: "Pancakes".into(), ..default() });
        let mut world = World::new();
        let object = |name: &str, key| GameObject { kind: ObjectKind::Meal, name: name.into(), objd: key, price: 0,
            center: Vec2::ZERO, half: Vec2::splat(0.3), height: 0.1, route: None };
        world.spawn((Meal { servings: 4 }, Dish(0), object("Pancakes", (1, 2, 3)), Transform::from_xyz(1.0, 3.75, 2.0).with_rotation(Quat::from_rotation_y(0.5)), crate::surroundings::ServedAt(1234.567)));
        world.spawn((Meal { servings: 0 }, Dish(0), object("Empty", (1, 2, 3)), Transform::default()));
        world.spawn((Meal { servings: 3 }, object("Pizza", (4, 5, 6)), Transform::default()));
        world.spawn((crate::surroundings::Spoiled, Dish(0), object("Spoiled Food", (7, 8, 9)), Transform::default(), crate::surroundings::ServedAt(10.0)));
        let mut state = bevy::ecs::system::SystemState::<SavableMeals>::new(&mut world);
        let saved = saved_meals(&state.get(&world).unwrap(), &data);
        let restored: Vec<SavedMeal> = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(restored.len(), 3, "empty platters are omitted, but spoiled food remains");
        let spoiled = restored.iter().find(|m| m.spoiled).unwrap();
        assert_eq!(spoiled.servings, 0);
        assert_eq!(spoiled.served_at, Some(10.0));
        assert_eq!(spoiled.object, Some((7, 8, 9)));
        let pancakes = restored.iter().find(|m| m.recipe == "Pancakes" && !m.spoiled).unwrap();
        assert_eq!(pancakes.servings, 4);
        assert_eq!(pancakes.served_at, Some(1234.567));
        assert_eq!(pancakes.position, [1.0, 3.75, 2.0]);
        assert!((pancakes.yaw - 0.5).abs() < 0.0001);
        let pizza = restored.iter().find(|m| m.name == "Pizza").unwrap();
        assert_eq!(pizza.object, Some((4, 5, 6)));
        assert_eq!(pizza.servings, 3);
        assert_eq!(pizza.served_at, None);
        assert!(pizza.recipe.is_empty());
        let old: SavedMeal = serde_json::from_str(r#"{"recipe":"Pancakes","servings":4,"position":[1,2,3],"yaw":0}"#).unwrap();
        assert!(old.object.is_none() && old.name.is_empty(), "older meal saves retain their default serving platter");
        assert_eq!(old.served_at, None);
        assert!(!old.spoiled);
    }

    #[test]
    fn post_meal_cleanup_respects_traits_chance_and_player_commands() {
        let mut world = World::new();
        let plate = world.spawn_empty().id();
        let mut sim = crate::sim::random_sim(&mut rand::rng(), "Test", Some(false), Age::Adult);
        sim.traits.clear();
        let mut queue = ActionQueue::default();
        queue_cleanup(&sim, &mut queue, plate, 0.5);
        assert!(queue.0.is_empty());
        queue_cleanup(&sim, &mut queue, plate, 0.49);
        assert_eq!(queue.0.len(), 1);
        assert!(matches!(queue.0[0].kind, ActionKind::Object { target, .. } if target == plate));
        queue.0.clear();
        sim.traits = vec![crate::life::Trait::Slob];
        queue_cleanup(&sim, &mut queue, plate, 0.0);
        assert!(queue.0.is_empty());
        sim.traits = vec![crate::life::Trait::Neat];
        queue_cleanup(&sim, &mut queue, plate, 0.99);
        assert_eq!(queue.0.len(), 1);
        queue.0.clear();
        queue.0.push_back(Action::new("Player action", ActionKind::EatHere, false));
        queue_cleanup(&sim, &mut queue, plate, 0.0);
        assert_eq!(queue.0.len(), 1);
        assert_eq!(queue.0[0].label, "Player action");
    }

    #[test]
    fn meals_and_seated_plates_use_reachable_surface_heights() {
        let mut world = World::new();
        let object = |kind, height| GameObject { kind, name: "Fixture".into(), objd: (0, 0, 0), price: 0,
            center: Vec2::ZERO, half: Vec2::splat(0.5), height, route: None };
        world.spawn((object(ObjectKind::Table, 0.75), Transform::from_xyz(0.0, 0.0, 0.55), UsedBy(None)));
        // (A shelf up on the wall, nearer: out of reach.)
        world.spawn((object(ObjectKind::Table, 1.3), Transform::from_xyz(0.0, 4.5, 0.6), UsedBy(None)));
        let upper = world.spawn((object(ObjectKind::Table, 0.75), Transform::from_xyz(0.0, 3.0, 0.7), UsedBy(None))).id();
        let chair = world.spawn((object(ObjectKind::Chair, 1.0), Transform::from_xyz(0.0, 3.0, 0.0), UsedBy(None))).id();
        let mut state = bevy::ecs::system::SystemState::<Query<(Entity, &GameObject, &Transform, &UsedBy)>>::new(&mut world);
        {
            let objects = state.get(&world).unwrap();
            assert_eq!(surface_entity_near(&objects, Vec3::new(0.0, 3.0, 0.55), 6.0), Some(upper));
            assert_eq!(surface_near(&objects, Vec3::new(0.0, 3.0, 0.55), 6.0).unwrap().y, 3.75);
            assert_eq!(dining_seat_for(&objects, chair).unwrap().1.y, 3.75);
        }
        world.despawn(upper);
        let objects = state.get(&world).unwrap();
        assert!(surface_near(&objects, Vec3::new(0.0, 3.0, 0.55), 6.0).is_none());
        assert!(surface_entity_near(&objects, Vec3::new(0.0, 3.0, 0.55), 6.0).is_none());
        assert!(dining_seat_for(&objects, chair).is_none(), "a downstairs table is not within reach of an upstairs chair");
    }

    #[test]
    fn a_picnic_table_is_eaten_at_from_its_nearest_place() {
        let mut world = World::new();
        let object = |kind, height| GameObject { kind, name: "Fixture".into(), objd: (0, 0, 0), price: 0,
            center: Vec2::ZERO, half: Vec2::new(0.9, 0.8), height, route: None };
        let picnic = world.spawn((object(ObjectKind::PicnicTable, 0.8), Transform::from_xyz(10.0, 0.0, 0.0), UsedBy(None))).id();
        let mut state = bevy::ecs::system::SystemState::<Query<(Entity, &GameObject, &Transform, &UsedBy)>>::new(&mut world);
        let objects = state.get(&world).unwrap();
        // (From the -z side: the place there, its plate on the table top before it.)
        let (seat, plate) = dining_seat(&objects, Vec3::new(9.5, 0.0, -3.0), &[]).unwrap();
        assert_eq!(seat, picnic);
        assert!((plate.y - 0.8).abs() < 1e-4 && plate.z < 0.0 && plate.z > -0.45, "{plate:?}");
        assert!(dining_seat(&objects, Vec3::new(9.5, 0.0, -3.0), &[picnic]).is_none());
    }

    #[test]
    fn meal_menu_obeys_original_hours_and_weekend_brunch() {
        use s3bake::gamedata::{MEAL_BREAKFAST, MEAL_BRUNCH, MEAL_DINNER, MEAL_LUNCH};
        for day in 0..7 {
            assert_eq!(meal_time(0.99, day), (MEAL_DINNER, "Dinner"));
            assert_eq!(meal_time(1.0, day), (MEAL_BREAKFAST, "Breakfast"));
            assert_eq!(meal_time(8.99, day), (MEAL_BREAKFAST, "Breakfast"));
            assert_eq!(meal_time(15.0, day), (MEAL_LUNCH, "Lunch"));
            assert_eq!(meal_time(16.99, day), (MEAL_LUNCH, "Lunch"));
            assert_eq!(meal_time(17.0, day), (MEAL_DINNER, "Dinner"));
            if day >= 5 {
                assert_eq!(meal_time(9.0, day), (MEAL_BRUNCH, "Brunch"));
                assert_eq!(meal_time(11.0, day), (MEAL_BRUNCH, "Brunch"));
                assert_eq!(meal_time(14.99, day), (MEAL_BRUNCH, "Brunch"));
            } else {
                assert_eq!(meal_time(9.0, day), (MEAL_BREAKFAST, "Breakfast"));
                assert_eq!(meal_time(10.99, day), (MEAL_BREAKFAST, "Breakfast"));
                assert_eq!(meal_time(11.0, day), (MEAL_LUNCH, "Lunch"));
                assert_eq!(meal_time(14.99, day), (MEAL_LUNCH, "Lunch"));
            }
        }
    }

    #[test]
    fn recipe_purchases_are_affordable_and_only_charge_once() {
        let mut known = KnownRecipes::default();
        let mut funds = 100;
        assert_eq!(known.purchase(&mut funds, "Ratatouille", 150), Err(()));
        assert_eq!(funds, 100);
        assert!(known.0.is_empty());
        assert_eq!(known.purchase(&mut funds, "Ratatouille", 100), Ok(true));
        assert_eq!(funds, 0);
        assert_eq!(known.purchase(&mut funds, "Ratatouille", 100), Ok(false));
        assert_eq!(known.0, ["Ratatouille"]);
        assert_eq!(funds, 0);
        assert_eq!(known.purchase(&mut funds, "Cookies", 1), Err(()));
        assert_eq!(known.0.len(), 1);
    }

    #[test]
    fn competing_meal_requests_cannot_duplicate_servings() {
        // Two Sims reach the last plate before deferred entity cleanup has run.
        let mut meal = Meal { servings: 1 };
        assert!(meal.take_serving());
        assert!(!meal.take_serving());
        assert_eq!(meal.take_remaining(), 0);

        // Putting away a platter must claim its servings before the next request.
        let mut meal = Meal { servings: 4 };
        assert!(meal.take_serving());
        assert_eq!(meal.take_remaining(), 3);
        assert_eq!(meal.take_remaining(), 0);
        assert!(!meal.take_serving());
        assert_eq!(meal.servings, 0);
    }

    /// The baked recipes' keys (`SIMS3_CACHE=<baked> cargo test -p sims3 list_recipes -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn list_recipes() {
        let data = s3bake::gamedata::load_gamedata(&s3bake::default_root()).expect("gamedata");
        println!("{}", data.recipes.iter().map(|r| r.key.clone()).collect::<Vec<_>>().join(" "));
    }
}
