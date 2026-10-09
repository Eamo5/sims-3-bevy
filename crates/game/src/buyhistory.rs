//! Buy-mode transactions retain entities so undo restores contents, upgrades and animation rigs.
//! Objects removed by a transaction are parked until history is discarded on return to Live.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::ecs::system::RunSystemOnce;

use crate::buy::{BuyMode, HeldObject};
use crate::building::{ActiveBuilding, BuildingSnapshot, LotPaint};
use crate::interact::{GameObject, Household, Notifications};
use crate::nav::{Floor, Obstacle};
use crate::save::{Bought, RemovedLotObjects, SavedObject};

#[derive(Component)]
pub struct HistoryHidden;

/// Purchase wishes settle when shopping finishes, so buying and undoing cannot earn rewards.
#[derive(Component)]
pub struct PendingPurchase {
    pub sim: Entity,
    pub price: i32,
}

pub fn settle_purchases(
    mut commands: Commands,
    buy: Res<BuyMode>,
    pending: Query<(Entity, &PendingPurchase), With<GameObject>>,
    mut events: MessageWriter<crate::life::LifeEvent>,
) {
    if buy.active { return; }
    for (entity, purchase) in &pending {
        events.write(crate::life::LifeEvent::new(purchase.sim, crate::life::LifeEventKind::Bought { price: purchase.price }));
        commands.entity(entity).remove::<PendingPurchase>();
    }
}

#[derive(Clone)]
struct Snapshot {
    object: GameObject,
    transform: Transform,
    floor: Option<Floor>,
    level: Option<u8>,
    obstacle: Option<Obstacle>,
    design: Option<s3bake::Key>,
    bought: bool,
    materials: Vec<(Entity, Handle<StandardMaterial>)>,
}

impl Snapshot {
    fn capture(world: &World, entity: Entity) -> Option<Self> {
        Some(Self {
            object: world.get::<GameObject>(entity)?.clone(),
            transform: *world.get::<Transform>(entity)?,
            floor: world.get::<Floor>(entity).copied(),
            level: world.get::<crate::building::BuildingPiece>(entity).map(|p| p.level),
            obstacle: world.get::<Obstacle>(entity).copied(),
            design: world.get::<crate::objects::Design>(entity).map(|d| d.0),
            bought: world.get::<Bought>(entity).is_some(),
            materials: world.get::<Children>(entity).map(|children| children.iter().filter_map(|e| {
                world.get::<MeshMaterial3d<StandardMaterial>>(e).map(|m| (e, m.0.clone()))
            }).collect()).unwrap_or_default(),
        })
    }

    fn restore(&self, world: &mut World, entity: Entity) {
        let mut e = world.entity_mut(entity);
        e.remove::<(HistoryHidden, HeldObject, Obstacle, Floor, Bought, crate::objects::Design, crate::building::BuildingPiece)>();
        e.insert((self.object.clone(), self.transform, Visibility::Inherited));
        if let Some(floor) = self.floor { e.insert(floor); }
        if let Some(level) = self.level { e.insert(crate::building::BuildingPiece { level }); }
        if let Some(obstacle) = self.obstacle { e.insert(obstacle); }
        if let Some(design) = self.design { e.insert(crate::objects::Design(design)); }
        if self.bought { e.insert(Bought); }
        for (child, material) in &self.materials {
            if let Ok(mut child) = world.get_entity_mut(*child) {
                child.insert(MeshMaterial3d(material.clone()));
            }
        }
    }
}

struct ObjectChange {
    entity: Entity,
    before: Option<Snapshot>,
    after: Option<Snapshot>,
}

struct Transaction {
    objects: Vec<ObjectChange>,
    building: Option<(BuildingSnapshot, BuildingSnapshot)>,
    terrain: Option<crate::terrain_paint::TerrainEdit>,
    /// Signed amount added to the household's funds by the original operation.
    funds: i64,
    removed_before: Vec<SavedObject>,
    removed_after: Vec<SavedObject>,
}

#[derive(Clone, Copy)]
pub enum Request { Undo, Redo }

#[derive(Resource, Default)]
pub struct BuyHistory {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    picked: HashMap<Entity, Snapshot>,
    pub request: Option<Request>,
}

impl BuyHistory {
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }
}

pub fn remember_pickup(commands: &mut Commands, entity: Entity) {
    commands.queue(move |world: &mut World| {
        if let Some(snapshot) = Snapshot::capture(world, entity)
            && let Some(mut history) = world.get_resource_mut::<BuyHistory>()
        {
            history.picked.insert(entity, snapshot);
        }
    });
}

/// Removes the object from gameplay without destroying its private component state.
pub fn park(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<(GameObject, Obstacle)>().insert((HistoryHidden, HeldObject, Visibility::Hidden));
}

/// Call after the placement/sale commands, so the after-image includes floor and design changes.
pub fn record(commands: &mut Commands, entity: Entity, existing: bool, funds: i64, removed_before: Vec<SavedObject>) {
    record_with_building(commands, entity, existing, funds, removed_before, None);
}

pub fn record_with_building(commands: &mut Commands, entity: Entity, existing: bool, funds: i64, removed_before: Vec<SavedObject>, building_before: Option<BuildingSnapshot>) {
    commands.queue(move |world: &mut World| {
        finish_terrain(world);
        let after = Snapshot::capture(world, entity);
        let removed_after = world.resource::<RemovedLotObjects>().0.clone();
        let enabled = world.resource::<BuyMode>().active;
        let building = building_before.map(|before| {
            let after = BuildingSnapshot::capture(world.resource::<ActiveBuilding>(), world.get_resource::<LotPaint>());
            (before, after)
        });
        let mut history = world.resource_mut::<BuyHistory>();
        let before = existing.then(|| history.picked.remove(&entity)).flatten();
        if enabled && (!existing || before.is_some()) {
            history.redo.clear();
            history.undo.push(Transaction { objects: vec![ObjectChange { entity, before, after }], building, terrain: None, funds, removed_before, removed_after });
            if history.undo.len() > 100 { history.undo.remove(0); }
        }
        collect_unused(world, false);
    });
}

/// One construction gesture, including any doors/windows sold by a demolition.
pub fn record_construction(
    commands: &mut Commands, before: BuildingSnapshot, b: &ActiveBuilding, ops: &[crate::building::PaintOp],
    funds: i64, removed_before: Vec<SavedObject>, sold: Vec<Entity>,
) {
    let mut after = BuildingSnapshot::capture(b, Some(&before.paint));
    after.paint.0.extend_from_slice(ops);
    commands.queue(move |world: &mut World| {
        finish_terrain(world);
        let removed_after = world.resource::<RemovedLotObjects>().0.clone();
        let mut history = world.resource_mut::<BuyHistory>();
        let objects = sold.into_iter().map(|entity| ObjectChange { entity, before: history.picked.remove(&entity), after: None }).collect();
        history.redo.clear();
        history.undo.push(Transaction { objects, building: Some((before, after)), terrain: None, funds, removed_before, removed_after });
        if history.undo.len() > 100 { history.undo.remove(0); }
        collect_unused(world, false);
    });
}

fn collect_unused(world: &mut World, all: bool) {
    let history = world.resource::<BuyHistory>();
    let keep: HashSet<Entity> = if all { HashSet::new() } else { history.undo.iter().chain(&history.redo).flat_map(|t| t.objects.iter().map(|o| o.entity)).collect() };
    let garbage: Vec<Entity> = world.query_filtered::<Entity, With<HistoryHidden>>().iter(world).filter(|e| !keep.contains(e)).collect();
    for entity in garbage { world.despawn(entity); }
}

fn apply(world: &mut World, request: Request) {
    let transaction = {
        let mut history = world.resource_mut::<BuyHistory>();
        match request { Request::Undo => history.undo.pop(), Request::Redo => history.redo.pop() }
    };
    let Some(transaction) = transaction else { return };
    let (funds, removed) = match request {
        Request::Undo => (-transaction.funds, &transaction.removed_before),
        Request::Redo => (transaction.funds, &transaction.removed_after),
    };
    if transaction.objects.iter().any(|o| world.get_entity(o.entity).is_err())
        || transaction.building.is_some() && !world.contains_resource::<ActiveBuilding>()
    {
        clear(world);
        return;
    }
    if world.get_resource::<Household>().is_none_or(|h| h.funds + funds < 0) {
        world.resource_mut::<Notifications>().push("You can't afford to reverse that transaction.");
        let mut history = world.resource_mut::<BuyHistory>();
        match request { Request::Undo => history.undo.push(transaction), Request::Redo => history.redo.push(transaction) }
        return;
    }
    for object in &transaction.objects {
        let snapshot = match request { Request::Undo => &object.before, Request::Redo => &object.after };
        match snapshot {
            Some(snapshot) => snapshot.restore(world, object.entity),
            None => {
                world.entity_mut(object.entity).remove::<(GameObject, Obstacle)>().insert((HistoryHidden, HeldObject, Visibility::Hidden));
            }
        }
    }
    if let Some((before, after)) = &transaction.building {
        let snapshot = match request { Request::Undo => before, Request::Redo => after };
        world.run_system_once_with(crate::building::restore_history, snapshot.clone()).expect("construction history resources");
    }
    if let Some(terrain) = &transaction.terrain {
        world.run_system_once_with(crate::terrain_paint::restore_history, (terrain.clone(), matches!(request, Request::Undo))).expect("terrain history resources");
    }
    world.resource_mut::<Household>().funds += funds;
    if transaction.terrain.is_none() { world.resource_mut::<RemovedLotObjects>().0 = removed.clone(); }
    if let Some(mut grid) = world.get_resource_mut::<crate::nav::NavGrid>() { grid.dirty = true; }
    let mut history = world.resource_mut::<BuyHistory>();
    match request { Request::Undo => history.redo.push(transaction), Request::Redo => history.undo.push(transaction) }
}

fn clear(world: &mut World) {
    collect_unused(world, true);
    *world.resource_mut::<BuyHistory>() = BuyHistory::default();
}

pub fn discard(commands: &mut Commands) {
    commands.queue(clear);
}

fn finish_terrain(world: &mut World) {
    if !world.contains_resource::<crate::terrain_paint::TerrainHistory>() { return; }
    world.resource_scope(|world, mut pending: Mut<crate::terrain_paint::TerrainHistory>| {
        if let Some(edit) = pending.take_edit(world.resource::<crate::terrain_paint::Strokes>(), world.resource::<crate::terrain_paint::Sculpted>()) {
            let mut history = world.resource_mut::<BuyHistory>();
            history.redo.clear();
            history.undo.push(Transaction { objects: Vec::new(), building: None, terrain: Some(edit), funds: 0, removed_before: Vec::new(), removed_after: Vec::new() });
            if history.undo.len() > 100 { history.undo.remove(0); }
        }
    });
}

/// Runs after editing and both original HUDs. Buy and Build share one chronological history.
pub fn update(world: &mut World) {
    let active = world.resource::<BuyMode>().active;
    let brush = matches!(world.resource::<BuyMode>().tool, Some(crate::build::BuildTool::Terrain | crate::build::BuildTool::Sculpt));
    let held = world.get_resource::<ButtonInput<MouseButton>>().is_some_and(|m| m.pressed(MouseButton::Left));
    // Finish a held brush on release, a tool change, or leaving shopping.
    if !active || !brush || !held {
        finish_terrain(world);
        collect_unused(world, false);
    }
    let buy = world.resource::<BuyMode>();
    if !active {
        clear(world);
        return;
    }
    let holding_owned = buy.placing.as_ref().is_some_and(|p| p.owned);
    let blocked = world.resource::<crate::options::GameMenu>().is_open()
        || world.query_filtered::<Entity, With<crate::dialog::Modal>>().iter(world).next().is_some();
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let keyboard = if ctrl && (keys.just_pressed(KeyCode::KeyY) || shift && keys.just_pressed(KeyCode::KeyZ)) {
        Some(Request::Redo)
    } else if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        Some(Request::Undo)
    } else { None };
    let request = world.resource_mut::<BuyHistory>().request.take().or(keyboard);
    if blocked || holding_owned || brush && held { return; }
    if let Some(request) = request {
        let history = world.resource::<BuyHistory>();
        if !match request { Request::Undo => history.can_undo(), Request::Redo => history.can_redo() } { return; }
        world.resource_scope(|world, mut buy: Mut<BuyMode>| buy.drop_tools(&mut world.commands()));
        world.flush();
        apply(world, request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<BuyHistory>();
        world.init_resource::<BuyMode>();
        world.resource_mut::<BuyMode>().active = true;
        world.init_resource::<RemovedLotObjects>();
        world.init_resource::<Notifications>();
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<crate::options::GameMenu>();
        world.insert_resource(Household { name: "Test".into(), funds: 900, lot_index: 0, last_bill_day: 0, bills: Vec::new() });
        world
    }

    fn object(world: &mut World, bought: bool) -> Entity {
        let e = world.spawn((
            GameObject { kind: crate::interact::ObjectKind::Chair, name: "Chair".into(), objd: (0, 0, 1), price: 100, center: Vec2::ZERO, half: Vec2::splat(0.5), height: 1.0, route: None },
            Transform::from_xyz(1.0, 2.0, 3.0).with_scale(Vec3::splat(1.2)),
            Visibility::Inherited, Floor(2),
            Obstacle { half: Vec2::splat(0.5), center_offset: Vec2::ZERO },
            crate::building::BuildingPiece { level: 2 },
            crate::upgrades::Upgrades(3), crate::interact::Broken,
        )).id();
        if bought { world.entity_mut(e).insert(Bought); }
        e
    }

    fn record_now(world: &mut World, entity: Entity, existing: bool, funds: i64, removed: Vec<SavedObject>) {
        record(&mut world.commands(), entity, existing, funds, removed);
        world.flush();
    }

    #[test]
    fn furniture_history_survives_switching_between_buy_and_build() {
        let mut world = world();
        let entity = object(&mut world, true);
        record_now(&mut world, entity, false, -100, Vec::new());
        world.resource_mut::<BuyMode>().show(crate::buy::BUILD_TAB);
        update(&mut world);
        assert!(world.resource::<BuyHistory>().can_undo());
        world.resource_mut::<BuyHistory>().request = Some(Request::Undo);
        update(&mut world);
        assert_eq!(world.resource::<Household>().funds, 1000);
        assert!(world.get::<HistoryHidden>(entity).is_some());
        world.resource_mut::<BuyMode>().show(0);
        world.resource_mut::<BuyHistory>().request = Some(Request::Redo);
        update(&mut world);
        assert_eq!(world.resource::<Household>().funds, 900);
        assert!(world.get::<GameObject>(entity).is_some());
    }

    #[test]
    fn purchase_undo_redo_refunds_without_recreating_the_object() {
        let mut world = world();
        let entity = object(&mut world, true);
        let child = world.spawn(ChildOf(entity)).id();
        record_now(&mut world, entity, false, -100, Vec::new());
        apply(&mut world, Request::Undo);
        assert_eq!(world.resource::<Household>().funds, 1000);
        assert!(world.get::<HistoryHidden>(entity).is_some());
        assert!(world.get::<GameObject>(entity).is_none());
        assert!(world.get::<Obstacle>(entity).is_none());
        apply(&mut world, Request::Redo);
        assert_eq!(world.resource::<Household>().funds, 900);
        assert!(world.get::<HistoryHidden>(entity).is_none());
        assert_eq!(world.get::<Floor>(entity).unwrap().0, 2);
        assert_eq!(world.get::<ChildOf>(child).unwrap().parent(), entity);
        assert_eq!(world.get::<crate::upgrades::Upgrades>(entity).unwrap().0, 3);
        assert!(world.get::<crate::interact::Broken>(entity).is_some());
    }

    #[test]
    fn move_and_sale_restore_positions_styles_and_premade_save_records() {
        let mut world = world();
        let entity = object(&mut world, false);
        let before = *world.get::<Transform>(entity).unwrap();
        let original_material = Handle::<StandardMaterial>::default();
        let child = world.spawn((ChildOf(entity), MeshMaterial3d(original_material.clone()))).id();
        remember_pickup(&mut world.commands(), entity);
        world.flush();
        let object = world.get::<GameObject>(entity).unwrap().clone();
        crate::save::note_removed(&mut world.resource_mut::<RemovedLotObjects>(), &object, &before);
        world.entity_mut(entity).insert((Transform::from_xyz(9.0, 5.0, 7.0), Floor(3), Bought, crate::objects::Design((1, 2, 3))));
        record_now(&mut world, entity, true, 0, Vec::new());
        apply(&mut world, Request::Undo);
        assert_eq!(*world.get::<Transform>(entity).unwrap(), before);
        assert_eq!(world.get::<Floor>(entity).unwrap().0, 2);
        assert!(world.get::<Bought>(entity).is_none());
        assert!(world.get::<crate::objects::Design>(entity).is_none());
        assert_eq!(world.get::<MeshMaterial3d<StandardMaterial>>(child).unwrap().0, original_material);
        assert!(world.resource::<RemovedLotObjects>().0.is_empty());
        apply(&mut world, Request::Redo);
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 9.0);
        assert_eq!(world.get::<crate::objects::Design>(entity).unwrap().0, (1, 2, 3));
        assert_eq!(world.resource::<RemovedLotObjects>().0.len(), 1);
        remember_pickup(&mut world.commands(), entity);
        world.flush();
        let removed = world.resource::<RemovedLotObjects>().0.clone();
        park(&mut world.commands(), entity);
        world.resource_mut::<Household>().funds += 100;
        record_now(&mut world, entity, true, 100, removed);
        apply(&mut world, Request::Undo);
        assert!(world.get::<GameObject>(entity).is_some());
        assert_eq!(world.resource::<Household>().funds, 900);
        apply(&mut world, Request::Undo);
        assert_eq!(*world.get::<Transform>(entity).unwrap(), before);
        assert!(world.resource::<RemovedLotObjects>().0.is_empty());
    }

    #[test]
    fn new_purchase_discards_redo_and_releases_abandoned_objects() {
        let mut world = world();
        let old = object(&mut world, true);
        let child = world.spawn(ChildOf(old)).id();
        record_now(&mut world, old, false, -100, Vec::new());
        apply(&mut world, Request::Undo);
        let new = object(&mut world, true);
        world.resource_mut::<Household>().funds -= 100;
        record_now(&mut world, new, false, -100, Vec::new());
        assert!(!world.resource::<BuyHistory>().can_redo());
        assert!(world.get_entity(old).is_err());
        assert!(world.get_entity(child).is_err());
        assert!(world.get_entity(new).is_ok());
        clear(&mut world);
        assert!(world.get_entity(new).is_ok());
        assert!(!world.resource::<BuyHistory>().can_undo());
    }

    #[test]
    fn unaffordable_redo_keeps_history_and_funds_intact() {
        let mut world = world();
        let entity = object(&mut world, true);
        record_now(&mut world, entity, false, -100, Vec::new());
        apply(&mut world, Request::Undo);
        world.resource_mut::<Household>().funds = 50;
        apply(&mut world, Request::Redo);
        assert_eq!(world.resource::<Household>().funds, 50);
        assert!(world.resource::<BuyHistory>().can_redo());
        assert!(world.get::<HistoryHidden>(entity).is_some());
        clear(&mut world);
        assert!(world.get_entity(entity).is_err());
    }

    #[test]
    fn keyboard_history_shortcuts_and_live_mode_cleanup() {
        let mut world = world();
        let entity = object(&mut world, true);
        record_now(&mut world, entity, false, -100, Vec::new());
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::KeyZ);
        }
        update(&mut world);
        assert_eq!(world.resource::<Household>().funds, 1000);
        assert!(world.get::<HistoryHidden>(entity).is_some());
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::ControlRight);
            keys.press(KeyCode::ShiftRight);
            keys.press(KeyCode::KeyZ);
        }
        update(&mut world);
        assert_eq!(world.resource::<Household>().funds, 900);
        assert!(world.get::<GameObject>(entity).is_some());
        world.resource_mut::<BuyMode>().active = false;
        update(&mut world);
        assert!(!world.resource::<BuyHistory>().can_undo());
        assert!(world.get::<GameObject>(entity).is_some());
    }

    #[test]
    fn purchase_rewards_settle_once_and_ignore_reversed_purchases() {
        let mut app = App::new();
        app.init_resource::<BuyMode>()
            .add_message::<crate::life::LifeEvent>()
            .add_systems(Update, settle_purchases);
        let sim = app.world_mut().spawn_empty().id();
        let kept = object(app.world_mut(), true);
        let reversed = object(app.world_mut(), true);
        for entity in [kept, reversed] {
            app.world_mut().entity_mut(entity).insert(PendingPurchase { sim, price: 100 });
        }
        app.world_mut().entity_mut(reversed).remove::<GameObject>().insert(HistoryHidden);
        app.world_mut().resource_mut::<BuyMode>().active = true;
        app.update();
        assert_eq!(app.world().resource::<Messages<crate::life::LifeEvent>>().len(), 0);
        app.world_mut().resource_mut::<BuyMode>().active = false;
        app.update();
        assert_eq!(app.world_mut().resource_mut::<Messages<crate::life::LifeEvent>>().drain().count(), 1);
        assert!(app.world().get::<PendingPurchase>(kept).is_none());
        app.update();
        assert_eq!(app.world().resource::<Messages<crate::life::LifeEvent>>().len(), 0);
    }
}
