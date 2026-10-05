//! Group meals, as the game serves them: whoever cooks puts the meal on a counter or table on
//! its serving platter, with a serving for each member of the household. Sims grab a plate, sit
//! down at a dining table to eat (the plate set on the table in front of them, a fork in hand)
//! or eat standing when there's no seat, and leave dirty dishes behind for someone to clear
//! away. The platter itself is left to wash up once the last serving is taken.

use bevy::prelude::*;

use crate::PlayMode;
use crate::baked::Baked;
use crate::interact::{Action, ActionKind, ActionQueue, CHAIR_EAT, GameObject, Notifications, ObjectKind, UsedBy};
use crate::loading::Catalog;
use crate::objects::{AssetCtx, ObjectAssets};
use crate::sim::{Age, HouseholdMember, Sim};

pub struct MealsPlugin;

impl Plugin for MealsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (deliver_pizza, meal_requests, release_plates).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// What a delivered pizza costs.
pub const PIZZA_PRICE: i64 = 40;

/// A pizza on its way.
#[derive(Resource)]
pub struct PizzaOrder {
    pub arrive_at: f64,
}

/// The pizza arrives: the game's pizza box, set down on the kitchen counter (by the stove),
/// with a slice for everyone.
#[allow(clippy::too_many_arguments)]
fn deliver_pizza(
    mut commands: Commands,
    order: Option<Res<PizzaOrder>>,
    clock: Res<crate::clock::GameClock>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    exit: Option<Res<crate::interact::LotExit>>,
    mut notes: ResMut<Notifications>,
) {
    let Some(order) = order else { return };
    if clock.minutes < order.arrive_at {
        return;
    }
    commands.remove_resource::<PizzaOrder>();
    let near = objects
        .iter()
        .find(|(_, o, _, _)| o.kind == ObjectKind::Stove)
        .map(|(_, o, tf, _)| o.world_center(tf))
        .or_else(|| exit.as_ref().map(|x| Vec3::new(x.0.x, 0.0, x.0.y)))
        .unwrap_or_default();
    let Some(at) = surface_near(&objects, near, 12.0) else {
        notes.push("The pizza came, but there was nowhere to put it.");
        return;
    };
    let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    if let Some(p) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, "FoodPizza", ObjectKind::Meal, "Pizza", at, 0.0) {
        commands.entity(p).insert(Meal { servings: 6 });
        notes.push("The pizza has arrived! It's on the kitchen counter.");
    }
}

/// What a Sim has just done with food (set by the interactions, handled here).
#[derive(Component, Clone, Copy, Debug)]
pub enum MealRequest {
    /// Finished cooking at this stove.
    Serve(Entity),
    /// Took a serving from this platter.
    Grabbed(Entity),
    /// Sat down to eat at this dining chair.
    PlateAt(Entity),
    /// Finished eating at the table.
    Ate,
    /// Finished eating standing.
    AteStanding,
}

/// A group meal's servings left.
#[derive(Component)]
pub struct Meal {
    pub servings: u8,
}

/// The plate in front of a Sim eating at a table.
#[derive(Component)]
struct EatingPlate(Entity);

/// The game's own objects for the platter and plates.
const PLATTER: &str = "PlateServing";
const PLATE: &str = "Plate";

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
fn spawn_dish(
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
    mut sims: Query<(Entity, &MealRequest, &Transform, &mut ActionQueue, &Sim, Option<&EatingPlate>)>,
    objects: Query<(Entity, &GameObject, &Transform, &UsedBy)>,
    mut meals: Query<&mut Meal>,
    household: Query<&Sim, With<HouseholdMember>>,
    (data, catalog, mut assets): (Res<Baked>, Res<Catalog>, ResMut<ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut notes: ResMut<Notifications>,
) {
    let mut taken: Vec<Entity> = Vec::new();
    for (me, req, tf, mut queue, sim, eating) in &mut sims {
        commands.entity(me).remove::<MealRequest>();
        let mut ctx = AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        match *req {
            MealRequest::Serve(stove) => {
                let Ok((_, s, stf, _)) = objects.get(stove) else { continue };
                let at = surface_near(&objects, s.world_center(stf), 6.0).unwrap_or(Vec3::new(s.world_center(stf).x, stf.translation.y + s.height, s.world_center(stf).z));
                let servings = household.iter().filter(|h| h.age != Age::Baby).count().clamp(2, 8) as u8;
                let yaw = stf.rotation.to_euler(EulerRot::YXZ).0;
                let Some(platter) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATTER, ObjectKind::Meal, "Group Meal", at, yaw) else { continue };
                commands.entity(platter).insert(Meal { servings });
                info!("meal served at {at:.1?}");
                notes.push(format!("{} made dinner for {servings}. Dinner is served!", sim.first));
                // The cook eats too.
                queue.0.push_front(Action::new("Grab a Plate", ActionKind::Object { target: platter, def: 0 }, true));
            }
            MealRequest::Grabbed(platter) => {
                if let Ok(mut m) = meals.get_mut(platter) {
                    m.servings = m.servings.saturating_sub(1);
                    if m.servings == 0 {
                        // The empty platter waits to be washed up.
                        commands.entity(platter).remove::<Meal>().queue_silenced(|mut w: EntityWorldMut| {
                            if let Some(mut g) = w.get_mut::<GameObject>() {
                                g.kind = ObjectKind::DirtyDishes;
                                g.name = "Dirty Dishes".into();
                            }
                        });
                    }
                }
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
            MealRequest::PlateAt(chair) => {
                info!("{} sits down to eat", sim.first);
                let Ok((_, _, ctf, _)) = objects.get(chair) else { continue };
                let Some((_, at)) = dining_seat_for(&objects, chair) else { continue };
                let yaw = ctf.rotation.to_euler(EulerRot::YXZ).0;
                if let Some(plate) = spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATE, ObjectKind::DirtyDishes, "Dirty Dishes", at, yaw) {
                    // (Not to be cleared away while it's being eaten from.)
                    commands.entity(plate).insert(UsedBy(Some(me)));
                    commands.entity(me).insert(EatingPlate(plate));
                }
            }
            MealRequest::Ate => {
                info!("{} finished eating", sim.first);
                if let Some(p) = eating {
                    commands.entity(p.0).insert(UsedBy(None));
                    commands.entity(me).remove::<EatingPlate>();
                }
            }
            MealRequest::AteStanding => {
                // The plate goes on the nearest surface, or the floor.
                let at = surface_near(&objects, tf.translation, 4.0).unwrap_or(tf.translation + tf.rotation * Vec3::new(0.3, 0.0, 0.4));
                spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, PLATE, ObjectKind::DirtyDishes, "Dirty Dishes", at, 0.0);
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
