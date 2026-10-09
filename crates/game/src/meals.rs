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
            .add_systems(OnEnter(crate::AppState::InGame), |mut l: ResMut<Leftovers>| l.0.clear())
            .add_systems(Update, (cook_prep, cook_prep_done, serve_if_interrupted, take_out_dinner, meal_requests, come_to_meal, release_plates, learn_recipes, cut_cakes).chain().run_if(in_state(PlayMode::Live)));
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

/// Heated, the dinner is taken out of the microwave (then eaten at the table).
fn take_out_dinner(mut commands: Commands, mut sims: Query<(Entity, &MicrowaveDone, &mut ActionQueue)>) {
    for (e, m, mut queue) in &mut sims {
        commands.entity(e).remove::<MicrowaveDone>();
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
        (Entity, &Sim, &crate::sim::Motives, &mut ActionQueue),
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
    for (e, sim, motives, mut queue) in &mut sims {
        if left == 0 || e == c.cook || sim.age.is_little() || motives.0[crate::sim::HUNGER] > 70.0 || queue.0.iter().any(|a| !a.autonomous) {
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

/// A counter or table near a point (the one the meal is set down on).
fn surface_entity_near(objects: &Query<(Entity, &GameObject, &Transform, &UsedBy)>, at: Vec3, within: f32) -> Option<Entity> {
    objects
        .iter()
        .filter(|(_, o, _, _)| o.kind == ObjectKind::Table)
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
        if front.label != "Cook Dinner" || !matches!(front.phase, crate::interact::Phase::Start | crate::interact::Phase::Routing) || front.cancel {
            continue;
        }
        // (Not on its way to the stove yet: there's the fridge and a counter first.)
        front.phase = crate::interact::Phase::Start;
        let front = queue.0.front().unwrap();
        let ActionKind::Object { target: stove, .. } = front.kind else { continue };
        let Ok((_, _, stf, _)) = objects.get(stove) else { continue };
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
fn cook_prep_done(mut commands: Commands, sims: Query<(Entity, &ActionQueue), With<CookPrepped>>) {
    for (e, queue) in &sims {
        if !queue.current().is_some_and(|a| matches!(a.label.as_str(), "Get Ingredients" | "Prepare Food" | "Cook Dinner")) {
            commands.entity(e).remove::<CookPrepped>();
        }
    }
}

/// What a delivered pizza costs.
pub const PIZZA_PRICE: i64 = 40;

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

/// What's being cooked: a recipe (index into the game's recipes).
#[derive(Component)]
pub struct MealPlan(pub usize);

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

/// Ingredients only found, never bought.
const RARE: [&str; 4] = ["Lifefruit", "Deathfish", "Flame Fruit", "Ingredient"];

/// The meal of the hour (`MEAL_*`) and its name.
pub fn meal_time(hour: f32) -> (u8, &'static str) {
    use s3bake::gamedata::{MEAL_BREAKFAST, MEAL_BRUNCH, MEAL_DINNER, MEAL_LUNCH};
    if (4.0..10.5).contains(&hour) {
        (MEAL_BREAKFAST | MEAL_BRUNCH, "Breakfast")
    } else if (10.5..15.0).contains(&hour) {
        (MEAL_LUNCH | MEAL_BRUNCH, "Lunch")
    } else {
        (MEAL_DINNER, "Dinner")
    }
}

/// The recipes a Sim can cook for a meal time (vegetarians, the meatless ones).
pub fn cookable(data: &s3bake::GameDataBaked, sim: &Sim, cooking: u32, known: Option<&KnownRecipes>, meal: u8) -> Vec<usize> {
    let veg = sim.traits.contains(&crate::life::Trait::Vegetarian);
    data.recipes
        .iter()
        .enumerate()
        .filter(|(_, r)| r.meals & meal != 0 && (!veg || r.vegetarian))
        .filter(|(_, r)| knows(r, cooking, known))
        .filter(|(_, r)| !r.ingredients.iter().any(|i| RARE.contains(&i.as_str())))
        .map(|(i, _)| i)
        .collect()
}

/// The food on a plate or platter (its model, a child of the dish).
#[derive(Component)]
struct DishFood(Entity);

/// Sets one of the recipes' food models on a dish, in place of any food already there.
fn set_food(commands: &mut Commands, assets: &mut ObjectAssets, ctx: &mut AssetCtx, dish: Entity, old: Option<&DishFood>, model: Option<s3bake::Key>) {
    if let Some(f) = old {
        commands.entity(f.0).despawn();
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

/// How many servings of leftovers the fridge keeps.
const MAX_LEFTOVERS: usize = 12;

/// Servings of meals put away in the fridge (by recipe, oldest first), to be had another time.
#[derive(Resource, Default, Clone, Debug)]
pub struct Leftovers(pub Vec<String>);

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
        .filter(|(_, o, _, _)| o.kind == ObjectKind::Table)
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
    let tables: Vec<(Vec3, f32, Vec2)> =
        objects.iter().filter(|(_, o, _, _)| o.kind == ObjectKind::Table).map(|(_, o, tf, _)| (o.world_center(tf), tf.translation.y + o.height, o.half)).collect();
    objects
        .iter()
        .filter(|(e, o, _, used)| matches!(o.kind, ObjectKind::Chair | ObjectKind::Stool) && used.0.is_none() && !taken.contains(e))
        .filter_map(|(e, o, tf, _)| {
            let seat = o.world_center(tf);
            let ahead = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
            let front = seat + ahead * 0.55;
            // A table top within reach in front of the seat, at table height.
            let (_, top, _) = tables.iter().find(|(c, top, half)| {
                let d = front.with_y(0.0).distance(c.with_y(0.0));
                d < half.max_element() + 0.35 && (top - tf.translation.y - 0.7).abs() < 0.35
            })?;
            Some((e, Vec3::new(front.x, *top, front.z), seat.distance(near)))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(e, p, _)| (e, p))
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
        (Option<&MealPlan>, Option<&Plateful>, &crate::interact::Skills, Option<&KnownRecipes>, Option<&ServingFrom>),
    )>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    mut meals: Query<(&mut Meal, Option<&Dish>, Option<&DishFood>)>,
    foods: Query<&DishFood>,
    household: Query<&Sim, With<HouseholdMember>>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<Notifications>,
    (ui, clock, mut funds, mut leftovers): (Option<Res<crate::icons::GameUi>>, Res<crate::clock::GameClock>, Option<ResMut<crate::interact::Household>>, ResMut<Leftovers>),
    mut did: MessageWriter<crate::journal::Did>,
) {
    let recipes = ui.as_ref().map(|u| u.data.clone());
    let recipe = |i: usize| recipes.as_ref().and_then(|d| d.recipes.get(i));
    let mut taken: Vec<Entity> = Vec::new();
    for (me, req, tf, mut queue, sim, eating, (plan, plateful, skills, known, serving)) in &mut sims {
        commands.entity(me).remove::<MealRequest>();
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        match *req {
            MealRequest::Serve(stove) => {
                let Ok((_, s, stf, _)) = objects.get(stove) else { continue };
                // (First carried from the stove to the counter or table it's served on.)
                if serving.is_none()
                    && let Some(surface) = surface_entity_near(&objects, s.world_center(stf), 6.0)
                    && let Some(def) = crate::interact::interactions_for(ObjectKind::Table).iter().position(|d| d.special == Special::PlaceMeal)
                {
                    commands.entity(me).insert((ServingFrom(stove), crate::anim::Carrying(PLATTER_CARRY)));
                    queue.0.push_front(Action::new("Set Down Meal", ActionKind::Object { target: surface, def }, true));
                    continue;
                }
                commands.entity(me).remove::<ServingFrom>();
                let at = surface_near(&objects, s.world_center(stf), 6.0).unwrap_or(Vec3::new(s.world_center(stf).x, stf.translation.y + s.height, s.world_center(stf).z));
                let servings = GROUP_MEAL_SERVINGS;
                let yaw = stf.rotation.to_euler(EulerRot::YXZ).0;
                // The recipe chosen, or the best they know for the time of day.
                let (meal, word) = meal_time(clock.hour_f());
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
                commands.entity(me).remove::<MealPlan>();
                let r = dish.and_then(recipe);
                let label = r.map_or("Group Meal".to_string(), |r| r.name.clone());
                let Some(platter) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATTER, ObjectKind::Meal, "Group Meal", at, yaw) else { continue };
                did.write(crate::journal::Did::count(me, crate::journal::Stat::Dishes, 1.0));
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
                // The ingredients.
                if let (Some(r), Some(h)) = (r, funds.as_mut()) {
                    h.funds -= r.cost as i64;
                }
                info!("meal served at {at:.1?}: {label}");
                let dish_word = if r.is_some_and(|r| r.meals == s3bake::gamedata::MEAL_DESSERT) { "Dessert" } else { word };
                notes.push(match r {
                    Some(r) => format!("{} made {} for {servings}. {dish_word} is served!", sim.first, r.name),
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
                            leftovers.0.push(r.key.clone());
                        }
                        // (The fridge holds a dozen; the oldest go out.)
                        let n = leftovers.0.len().saturating_sub(MAX_LEFTOVERS);
                        leftovers.0.drain(..n);
                    }
                    commands.entity(platter).try_despawn();
                }
            }
            MealRequest::Quick => {
                // (Something simple of what they know, for the time of day.)
                let pick = recipes.as_ref().and_then(|d| {
                    let mut options = cookable(d, sim, skills.level("Cooking"), known, meal_time(clock.hour_f()).0);
                    options.sort_by_key(|&i| d.recipes[i].level);
                    options.truncate(3);
                    options.choose(&mut rand::rng()).copied()
                });
                match pick {
                    Some(i) => commands.entity(me).insert(Plateful(i)),
                    None => commands.entity(me).remove::<Plateful>(),
                };
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
                    let ok: Vec<usize> = (0..d.recipes.len()).filter(|&i| !d.recipes[i].ingredients.iter().any(|x| RARE.contains(&x.as_str()))).collect();
                    ok.choose(&mut rand::rng()).copied()
                });
                match pick {
                    Some(i) => commands.entity(me).insert(Plateful(i)),
                    None => commands.entity(me).remove::<Plateful>(),
                };
                match dining_seat(&objects, tf.translation, &taken) {
                    Some((chair, _)) => {
                        taken.push(chair);
                        queue.0.push_front(Action::new("Eat", ActionKind::Object { target: chair, def: CHAIR_EAT }, true));
                    }
                    None => queue.0.push_front(Action::new("Eat", ActionKind::EatHere, true)),
                }
            }
            MealRequest::FromFridge => {
                // (The oldest first.)
                if leftovers.0.is_empty() {
                    continue;
                }
                let key = leftovers.0.remove(0);
                match recipes.as_ref().and_then(|d| d.recipes.iter().position(|r| r.key == key)) {
                    Some(i) => commands.entity(me).insert(Plateful(i)),
                    None => commands.entity(me).remove::<Plateful>(),
                };
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
                let Ok((_, _, ctf, _)) = objects.get(chair) else { continue };
                let Some((_, at)) = dining_seat_for(&objects, chair) else { continue };
                let yaw = ctf.rotation.to_euler(EulerRot::YXZ).0;
                if let Some(plate) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATE, ObjectKind::DirtyDishes, "Dirty Dishes", at, yaw) {
                    set_food(&mut commands, &mut assets, &mut ctx, plate, None, plateful.and_then(|p| recipe(p.0)).and_then(|r| r.single));
                    // (Not to be cleared away while it's being eaten from.)
                    commands.entity(plate).insert(UsedBy(Some(me)));
                    commands.entity(me).insert(EatingPlate(plate));
                }
            }
            MealRequest::Ate => {
                info!("{} finished eating", sim.first);
                // (Their favourite food: an amazing meal.)
                if let Some(r) = plateful.and_then(|p| recipes.as_ref().and_then(|d| d.recipes.get(p.0)))
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
                if let Some(p) = eating {
                    commands.entity(me).remove::<EatingPlate>();
                    commands.entity(p.0).insert(UsedBy(None));
                    // The plate, eaten clean.
                    let empty = plateful.and_then(|p| recipe(p.0)).and_then(|r| r.single_empty);
                    set_food(&mut commands, &mut assets, &mut ctx, p.0, foods.get(p.0).ok(), empty);
                }
                commands.entity(me).remove::<Plateful>();
            }
            MealRequest::AteStanding => {
                // The plate goes on the nearest surface, or the floor.
                let at = surface_near(&objects, tf.translation, 4.0).unwrap_or(tf.translation + tf.rotation * Vec3::new(0.3, 0.0, 0.4));
                if let Some(plate) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATE, ObjectKind::DirtyDishes, "Dirty Dishes", at, 0.0) {
                    set_food(&mut commands, &mut assets, &mut ctx, plate, None, plateful.and_then(|p| recipe(p.0)).and_then(|r| r.single_empty));
                }
                commands.entity(me).remove::<Plateful>();
            }
        }
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
            (d < t.half.max_element() + 0.35).then_some((ttf.translation.y + t.height, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))?
        .0;
    Some((chair, Vec3::new(front.x, top, front.z)))
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
