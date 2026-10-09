//! Sims' inventories, as the game's: what each Sim carries — the produce they've picked, the
//! fish they've caught, the gems, metals, space rocks and insects they've found — in stacks of
//! a kind, shown on the Sim panel's Inventory tab (with the objects' catalogue pictures) to
//! sell, or, for produce, to eat. The collection journal counts what the household holds.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::hud::BlocksWorld;
use crate::interact::{Action, ActionKind, ActionQueue, Household, Notifications};
use crate::menu::{BTN_HOVER, BTN_NORMAL, PLUMBOB_GREEN, text};
use crate::sim::{HouseholdMember, Selected, Sim};
use crate::PlayMode;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Chosen>().add_systems(Update, (inventory_buttons, migrate_held).run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Produce,
    Fish,
    /// Gems, metals and space rocks.
    Find,
    /// Butterflies and beetles.
    Insect,
    /// A painting the Sim painted.
    Painting,
    /// A lifetime reward object, to place on the lot.
    Reward,
}

/// Items of one kind (and, for produce, one quality).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stack {
    pub kind: ItemKind,
    /// The collectible's key, or the produce's name.
    pub key: String,
    /// What's shown: "Perfect Tomato", "Ruby".
    pub name: String,
    /// Produce's quality (a tier of the gardening qualities).
    pub quality: u8,
    pub count: u32,
    /// What they're all worth.
    pub worth: i64,
}

impl Stack {
    /// What one of them is worth.
    pub fn each(&self) -> i64 {
        (self.worth as f64 / self.count.max(1) as f64).round() as i64
    }
}

/// What a Sim carries.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inventory(pub Vec<Stack>);

impl Inventory {
    pub fn add(&mut self, kind: ItemKind, key: &str, name: &str, quality: u8, value: i64) {
        match self.0.iter_mut().find(|s| s.kind == kind && s.key == key && s.quality == quality) {
            Some(s) => {
                s.count += 1;
                s.worth += value;
            }
            None => self.0.push(Stack { kind, key: key.to_string(), name: name.to_string(), quality, count: 1, worth: value }),
        }
    }

    /// Takes one item from a stack: what it's worth.
    pub fn take_one(&mut self, i: usize) -> Option<i64> {
        let s = self.0.get_mut(i)?;
        let v = s.each();
        s.count -= 1;
        s.worth -= v;
        if s.count == 0 {
            self.0.remove(i);
        }
        Some(v)
    }

    /// Reserve a snack before eating begins, so cancelling cannot reuse the same food.
    pub fn consume_produce(&mut self, key: &str, quality: u8) -> bool {
        let Some(i) = self.0.iter().position(|s| s.kind == ItemKind::Produce && s.key == key && s.quality == quality && s.count > 0) else { return false };
        self.take_one(i).is_some()
    }

    /// How many of a collectible they hold.
    pub fn held(&self, key: &str) -> u32 {
        self.0.iter().filter(|s| matches!(s.kind, ItemKind::Fish | ItemKind::Find | ItemKind::Insect) && s.key == key).map(|s| s.count).sum()
    }

    /// Their collectibles (finds, fish and insects): count and worth.
    pub fn collectibles(&self) -> (u32, i64) {
        self.0.iter().filter(|s| matches!(s.kind, ItemKind::Fish | ItemKind::Find | ItemKind::Insect)).fold((0, 0), |(n, w), s| (n + s.count, w + s.worth))
    }
}

/// Puts an item in a Sim's inventory (giving them one if they have none).
pub fn give(commands: &mut Commands, sim: Entity, kind: ItemKind, key: String, name: String, quality: u8, value: i64, count: u32) {
    commands.entity(sim).queue_silenced(move |mut e: EntityWorldMut| {
        if e.get::<Inventory>().is_none() {
            e.insert(Inventory::default());
        }
        if let Some(mut inv) = e.get_mut::<Inventory>() {
            for _ in 0..count {
                inv.add(kind, &key, &name, quality, value);
            }
        }
    });
}

/// The stack picked on the Inventory tab.
#[derive(Resource, Default)]
pub struct Chosen(pub Option<usize>);

/// An action on the chosen stack asked for from its pie menu (the game's inventory panel).
#[derive(Resource)]
pub struct DoItem(pub ItemButton);

/// A stack's picture: a painting's own, a reward's or find's catalogue picture, produce's model.
pub fn stack_picture(
    s: &Stack,
    ui: Option<&mut crate::icons::GameUi>,
    pictures: &mut crate::paintings::PaintingImages,
    images: &mut Assets<Image>,
    baked: Option<&crate::baked::Baked>,
    thumbs: &mut crate::thumbs::ModelThumbs,
) -> Option<(Handle<Image>, Option<Rect>)> {
    if s.kind == ItemKind::Painting {
        return crate::paintings::image(pictures, images, &baked?.0, s).map(|(h, r)| (h, Some(r)));
    }
    let ui = ui?;
    if s.kind == ItemKind::Reward {
        let objd = baked?.0.catalog.iter().find(|c| c.instance_name == s.key)?.objd;
        return ui.icon(images, &s3bake::gamedata::thumb_name(objd.2)).map(|h| (h, None));
    }
    if s.kind == ItemKind::Produce {
        let model = ui.data.plants.iter().find(|p| p.produce == s.key)?.produce_model?;
        return Some((thumbs.get(images, model), None));
    }
    let model = ui.data.collectibles.iter().find(|c| c.key == s.key)?.model.clone();
    let objd = baked?.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(&model))?.objd;
    ui.icon(images, &s3bake::gamedata::thumb_name(objd.2)).map(|h| (h, None))
}

/// What can be done with a stack (its pie menu): selling one or all, eating produce, hanging a
/// painting, placing a reward.
pub fn stack_actions(sim: &Sim, s: &Stack) -> Vec<(String, ItemButton)> {
    let mut out = Vec::new();
    if s.kind != ItemKind::Reward {
        out.push((format!("Sell (§{})", s.each()), ItemButton::SellOne));
        if s.count > 1 {
            out.push((format!("Sell All (§{})", s.worth), ItemButton::SellAll));
        }
    }
    if s.kind == ItemKind::Produce && !sim.age.is_little() {
        out.push((format!("Eat {}", s.name), ItemButton::Eat));
    }
    if s.kind == ItemKind::Painting {
        out.push(("Hang on a Wall".to_string(), ItemButton::Hang));
    }
    if s.kind == ItemKind::Reward {
        out.push(("Place on the Lot".to_string(), ItemButton::Hang));
    }
    out
}

#[derive(Component)]
pub struct ItemTile(usize);

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemButton {
    SellOne,
    SellAll,
    Eat,
    /// Hang a painting on a wall.
    Hang,
}

/// The tile's colour for produce, by quality (grey for the worst, gold for perfect).
fn quality_color(q: u8) -> Color {
    let t = q as f32 / 9.0;
    Color::srgb(0.35 + t * 0.55, 0.45 + t * 0.35, 0.55 - t * 0.35)
}

/// The Inventory tab: a tile for each stack (the object's picture and how many), and what can
/// be done with the one picked.
pub fn draw_tab(
    p: &mut ChildSpawnerCommands,
    sim: &Sim,
    inv: Option<&Inventory>,
    chosen: Option<usize>,
    mut picture: impl FnMut(&Stack) -> Option<(Handle<Image>, Option<Rect>)>,
) {
    let stacks = inv.map(|i| i.0.as_slice()).unwrap_or_default();
    if stacks.is_empty() {
        p.spawn(text(
            format!("{}'s inventory is empty. Produce picked, fish caught and things found go here.", sim.first),
            13.0,
            Color::srgb(0.8, 0.85, 0.95),
        ));
        return;
    }
    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), max_width: Val::Px(450.0), ..default() }).with_children(|grid| {
        for (i, s) in stacks.iter().enumerate() {
            let bg = match s.kind {
                ItemKind::Produce => quality_color(s.quality),
                ItemKind::Painting => Color::srgb(0.85, 0.75, 0.55),
                _ => Color::srgba(1.0, 1.0, 1.0, 0.85),
            };
            grid.spawn((
                Button,
                ItemTile(i),
                BlocksWorld,
                Node {
                    width: Val::Px(52.0),
                    height: Val::Px(52.0),
                    border: UiRect::all(Val::Px(if chosen == Some(i) { 3.0 } else { 1.0 })),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(if chosen == Some(i) { PLUMBOB_GREEN } else { Color::srgba(0.0, 0.0, 0.0, 0.4) }),
                BackgroundColor(bg),
            ))
            .with_children(|t| {
                match picture(s) {
                    // (A painting's picture, in its own shape.)
                    Some((h, Some(r))) => {
                        let k = 46.0 / r.width().max(r.height()).max(1.0);
                        t.spawn((
                            ImageNode { image: h, rect: Some(r), ..default() },
                            Node { width: Val::Px(r.width() * k), height: Val::Px(r.height() * k), ..default() },
                            Pickable::IGNORE,
                        ));
                    }
                    Some((h, None)) => {
                        t.spawn((crate::icons::icon_bundle(h, 44.0), Pickable::IGNORE));
                    }
                    None => {
                        // (Produce and paintings: their name.)
                        let word = if s.kind == ItemKind::Painting { s.name.split_whitespace().next().unwrap_or("") } else { s.key.split_whitespace().next().unwrap_or("") };
                        let word = word.chars().take(9).collect::<String>();
                        t.spawn((text(word, 11.0, Color::BLACK), Pickable::IGNORE));
                    }
                }
                if s.count > 1 {
                    t.spawn((
                        Node { position_type: PositionType::Absolute, right: Val::Px(2.0), bottom: Val::Px(0.0), ..default() },
                        text(format!("{}", s.count), 12.0, Color::BLACK),
                        Pickable::IGNORE,
                    ));
                }
            });
        }
    });
    let total: i64 = stacks.iter().map(|s| s.worth).sum();
    let line = match chosen.and_then(|i| stacks.get(i)) {
        Some(s) => format!("{}{} · §{} each, §{} in all", s.name, if s.count > 1 { format!(" ×{}", s.count) } else { String::new() }, s.each(), s.worth),
        None => format!("{} items worth §{total}. Pick one to sell it{}.", stacks.iter().map(|s| s.count).sum::<u32>(), if stacks.iter().any(|s| s.kind == ItemKind::Produce) { " or eat it" } else { "" }),
    };
    p.spawn(text(line, 13.0, Color::WHITE));
    if let Some(s) = chosen.and_then(|i| stacks.get(i)) {
        p.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|row| {
            let mut buttons = Vec::new();
            // (Rewards aren't sold.)
            if s.kind != ItemKind::Reward {
                buttons.push((ItemButton::SellOne, format!("Sell (§{})", s.each())));
                if s.count > 1 {
                    buttons.push((ItemButton::SellAll, format!("Sell All (§{})", s.worth)));
                }
            }
            if s.kind == ItemKind::Produce && !sim.age.is_little() {
                buttons.push((ItemButton::Eat, "Eat".to_string()));
            }
            if s.kind == ItemKind::Painting {
                buttons.push((ItemButton::Hang, "Hang on a Wall".to_string()));
            }
            if s.kind == ItemKind::Reward {
                buttons.push((ItemButton::Hang, "Place on the Lot".to_string()));
            }
            for (b, label) in buttons {
                row.spawn((
                    Button,
                    b,
                    BlocksWorld,
                    Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), border_radius: BorderRadius::all(Val::Px(8.0)), ..default() },
                    BackgroundColor(BTN_NORMAL),
                ))
                .with_children(|b| {
                    b.spawn((text(label, 13.0, Color::WHITE), Pickable::IGNORE));
                });
            }
        });
    }
}

#[allow(clippy::type_complexity)]
fn inventory_buttons(
    mut commands: Commands,
    baked: Option<Res<crate::baked::Baked>>,
    tiles: Query<(&Interaction, &ItemTile), Changed<Interaction>>,
    mut buttons: Query<(&Interaction, &ItemButton, &mut BackgroundColor), Changed<Interaction>>,
    mut chosen: ResMut<Chosen>,
    mut sel: Query<(Entity, &Sim, &mut Inventory, &mut ActionQueue), With<Selected>>,
    mut household: Option<ResMut<Household>>,
    mut notes: ResMut<Notifications>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    selected: Query<Entity, Changed<Selected>>,
    asked: Option<Res<DoItem>>,
) {
    // (A different Sim: nothing picked.)
    if !selected.is_empty() {
        chosen.0 = None;
    }
    for (i, t) in &tiles {
        if *i == Interaction::Pressed {
            chosen.0 = if chosen.0 == Some(t.0) { None } else { Some(t.0) };
        }
    }
    // (Asked for from the stack's pie menu.)
    let from_pie = asked.map(|a| a.0);
    if from_pie.is_some() {
        commands.remove_resource::<DoItem>();
    }
    let pressed: Vec<ItemButton> = buttons
        .iter_mut()
        .filter_map(|(i, b, mut bg)| {
            bg.0 = if *i == Interaction::Hovered { BTN_HOVER } else { BTN_NORMAL };
            (*i == Interaction::Pressed).then_some(*b)
        })
        .chain(from_pie)
        .collect();
    for b in &pressed {
        let (Ok((me, sim, mut inv, mut queue)), Some(k)) = (sel.single_mut(), chosen.0) else { continue };
        let Some(s) = inv.0.get(k).cloned() else { continue };
        match b {
            ItemButton::SellOne | ItemButton::SellAll => {
                let n = if *b == ItemButton::SellAll { s.count } else { 1 };
                let mut got = 0;
                for _ in 0..n {
                    got += inv.take_one(k).unwrap_or(0);
                }
                if let Some(h) = household.as_mut() {
                    h.funds += got;
                }
                notes.push(format!("{} sold {} for §{got}.", sim.first, if n > 1 { format!("{n} × {}", s.name) } else { s.name.clone() }));
                play.write(crate::sound::PlaySound::ui("ui_object_sell"));
                if inv.0.get(k).is_none_or(|x| x.key != s.key || x.quality != s.quality) {
                    chosen.0 = None;
                }
            }
            ItemButton::Eat => {
                queue.push_player(Action::new(format!("Eat {}", s.name), ActionKind::EatItem { key: s.key.clone(), quality: s.quality }, false));
            }
            ItemButton::Hang => {
                // Out of the inventory and up to the walls (or onto the lot), in Buy mode (back if
                // it isn't put down).
                let object = baked.as_ref().and_then(|b| match s.kind {
                    ItemKind::Reward => b.0.catalog.iter().find(|c| c.instance_name == s.key).map(|c| (c.objd, None)),
                    _ => crate::paintings::object(&b.0.paintings, &s).map(|(o, d)| (o, Some(d))),
                });
                let Some((objd, design)) = object else { continue };
                let Some(each) = inv.take_one(k) else { continue };
                let item = Stack { count: 1, worth: each, ..s.clone() };
                commands.insert_resource(crate::buy::HoldRequest { objd, design, item, from: me });
                chosen.0 = None;
            }
        }
    }
}

#[cfg(test)]
mod button_tests {
    use super::*;

    #[test]
    fn eating_consumes_only_the_requested_produce_and_rejects_exhausted_requests() {
        let mut inv = Inventory::default();
        inv.add(ItemKind::Produce, "Tomato", "Normal Tomato", 3, 10);
        inv.add(ItemKind::Produce, "Tomato", "Perfect Tomato", 9, 40);
        inv.add(ItemKind::Find, "Tomato", "Unrelated item", 3, 100);
        assert!(inv.consume_produce("Tomato", 3));
        assert!(!inv.consume_produce("Tomato", 3), "a second queued snack cannot use an exhausted stack or unrelated item");
        assert_eq!(inv.0.len(), 2);
        assert_eq!(inv.0[0].quality, 9);
        assert_eq!(inv.0[0].worth, 40);
        assert_eq!(inv.0[1].kind, ItemKind::Find);
        assert!(inv.consume_produce("Tomato", 9));
        assert_eq!(inv.0.len(), 1);
    }

    #[test]
    fn holding_sell_one_sells_only_once_until_released_and_pressed_again() {
        let mut app = App::new();
        app.init_resource::<Chosen>()
            .init_resource::<Notifications>()
            .add_message::<crate::sound::PlaySound>()
            .insert_resource(Household { name: "Test".into(), funds: 100, lot_index: 0, last_bill_day: 0, bills: Vec::new() })
            .add_systems(Update, inventory_buttons);
        let sim = crate::sim::random_sim(&mut rand::rng(), "Test", Some(false), crate::sim::Age::Adult);
        let mut inv = Inventory::default();
        for _ in 0..3 { inv.add(ItemKind::Produce, "Tomato", "Tomato", 3, 10); }
        let me = app.world_mut().spawn((sim, Selected, inv, ActionQueue::default())).id();
        app.update();
        app.world_mut().resource_mut::<Chosen>().0 = Some(0);
        let button = app.world_mut().spawn((Interaction::Pressed, ItemButton::SellOne, BackgroundColor(BTN_NORMAL))).id();
        app.update();
        for _ in 0..5 { app.update(); }
        assert_eq!(app.world().get::<Inventory>(me).unwrap().0[0].count, 2);
        assert_eq!(app.world().resource::<Household>().funds, 110);
        assert_eq!(app.world().resource::<Notifications>().0.len(), 1);
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Hovered;
        app.update();
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
        app.update();
        assert_eq!(app.world().get::<Inventory>(me).unwrap().0[0].count, 1);
        assert_eq!(app.world().resource::<Household>().funds, 120);
    }
}

/// Finds held in the old household-wide collection go to a household member's inventory.
fn migrate_held(
    mut commands: Commands,
    mut collection: ResMut<crate::collecting::Collection>,
    members: Query<Entity, With<HouseholdMember>>,
    selected: Query<Entity, (With<HouseholdMember>, With<Selected>)>,
    ui: Option<Res<crate::icons::GameUi>>,
) {
    if collection.held.is_empty() {
        return;
    }
    let (Some(who), Some(ui)) = (selected.iter().next().or_else(|| members.iter().next()), ui) else { return };
    for (key, (n, worth)) in std::mem::take(&mut collection.held) {
        let Some(c) = ui.data.collectibles.iter().find(|c| c.key == key) else { continue };
        let kind = match c.kind {
            s3bake::gamedata::CollectKind::Fish => ItemKind::Fish,
            s3bake::gamedata::CollectKind::Butterfly | s3bake::gamedata::CollectKind::Beetle => ItemKind::Insect,
            _ => ItemKind::Find,
        };
        let each = worth / n.max(1) as i64;
        give(&mut commands, who, kind, key, c.name.clone(), 0, each, n);
    }
}
