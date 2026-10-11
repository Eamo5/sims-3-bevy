//! The household's family inventory, as the game's (`BuyModel.FamilyInventory`): furniture the
//! household owns but hasn't set out. In Buy mode an object picked up off the lot goes in by the
//! Family Inventory button (or a click on the inventory's grid), comes out again free from the
//! inventory's own catalogue, and can be sold from there, one or all. It's kept with the game.

use bevy::prelude::*;

use crate::save::SavedObject;

pub struct FamilyInventoryPlugin;

impl Plugin for FamilyInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FamilyInventory>().add_systems(OnExit(crate::AppState::InGame), |mut inv: ResMut<FamilyInventory>| inv.0.clear());
    }
}

/// What's in it: each object as the save keeps it (its design, and a painting's picture), where
/// it stood not mattering.
#[derive(Resource, Default, Clone, Debug)]
pub struct FamilyInventory(pub Vec<SavedObject>);

/// An object as it goes into the inventory.
pub fn stored(objd: s3bake::Key, design: Option<s3bake::Key>, item: Option<crate::inventory::Stack>) -> SavedObject {
    SavedObject { objd, position: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], design: None, design_key: design, item }
}

/// What an object in the inventory sells for: a painting's worth, or its catalogue price.
pub fn worth(o: &SavedObject, catalog: &crate::loading::Catalog) -> i64 {
    o.item.as_ref().map_or_else(|| catalog.by_key(&o.objd).map_or(0, |e| e.price.max(0) as i64), |i| i.worth)
}
