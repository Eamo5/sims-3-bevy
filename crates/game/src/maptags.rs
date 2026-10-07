//! Map View tags: zoomed out over town, every lot gets the game's map tag (its tag base, coloured
//! by kind, with the venue's glyph from the game's map-tag atlas): the household's home with a
//! star, other homes, empty lots, and each community venue. Hovering shows the lot's name;
//! clicking a venue offers its activities, clicking a home flies the camera there.

use bevy::prelude::*;

use crate::PlayMode;
use crate::camera::SimsCamera;
use crate::interact::{ActionKind, Household, ObjectKind};
use crate::loading::CurrentWorld;

pub struct MapTagsPlugin;

impl Plugin for MapTagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_tags, collectible_tags, place_tags, tag_clicks).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), |mut c: Commands, q: Query<Entity, With<MapTag>>| {
                for e in &q {
                    c.entity(e).despawn();
                }
            });
    }
}

/// Above this camera distance the town shows its map tags.
const MAP_DISTANCE: f32 = 140.0;
const SIZE: f32 = 40.0;

#[derive(Component)]
struct MapTag {
    lot: usize,
    at: Vec3,
}

/// A collectible's tag (shown by the Collection Helper): the gem, metal or space rock lying on
/// the lot being visited, or where they turn up on another lot (its rock spawner).
#[derive(Component)]
enum CollectibleTag {
    Find(Entity),
    Spawner(usize),
}

/// With a Collection Helper at home, the collectibles about town are tagged in Map View: those
/// lying on the lot being visited (a click sends the Sim to collect one), and where they turn
/// up on other lots (a click goes there); without one, they're not.
#[allow(clippy::too_many_arguments)]
fn collectible_tags(
    mut commands: Commands,
    objects: Query<(Entity, &crate::interact::GameObject, &Transform)>,
    tags: Query<(Entity, &CollectibleTag)>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    (world, visited): (Res<CurrentWorld>, Option<Res<crate::visit::VisitedLot>>),
    time: Res<Time>,
    mut last: Local<f32>,
) {
    let Some(mut ui) = ui else { return };
    if time.elapsed_secs() - *last < 1.0 {
        return;
    }
    *last = time.elapsed_secs();
    let helper = objects.iter().any(|(_, o, _)| o.kind == ObjectKind::CollectionHelper);
    let here = visited.as_ref().map(|v| v.lot);
    for (e, t) in &tags {
        let gone = match t {
            CollectibleTag::Find(f) => !objects.contains(*f),
            CollectibleTag::Spawner(lot) => Some(*lot) == here,
        };
        if !helper || gone {
            commands.entity(e).despawn();
        }
    }
    if !helper {
        return;
    }
    let (Some(base), glyph) = (ui.icon(&mut images, "hud_icon_maptagbase_r2"), ui.icon(&mut images, "maptag_collectible")) else { return };
    let mut new: Vec<(CollectibleTag, Vec3, String)> = objects
        .iter()
        .filter(|(e, o, _)| o.kind == ObjectKind::Collectible && !tags.iter().any(|(_, t)| matches!(t, CollectibleTag::Find(f) if f == e)))
        .map(|(e, o, tf)| (CollectibleTag::Find(e), tf.translation + Vec3::Y * 3.0, o.name.clone()))
        .collect();
    // Other lots: where their rock spawners leave gems, metals and space rocks.
    for (&lot, b) in &world.data.buildings {
        if Some(lot) == here || tags.iter().any(|(_, t)| matches!(t, CollectibleTag::Spawner(l) if *l == lot)) {
            continue;
        }
        let spawners = b.objects.iter().filter(|o| {
            let class = o.script.rsplit('.').next().unwrap_or("");
            class.starts_with("RockGemMetalSpawner") && ui.data.spawners.iter().any(|s| s.class == class)
        });
        let name = world.data.lot_names.get(lot).map_or("", |s| s.as_str());
        for o in spawners {
            new.push((CollectibleTag::Spawner(lot), Vec3::from(o.position) + Vec3::Y * 3.0, format!("Gems and metals turn up here ({name})")));
        }
    }
    for (tag, at, name) in new {
        commands
            .spawn((
                MapTag { lot: usize::MAX, at },
                tag,
                Button,
                Node { position_type: PositionType::Absolute, width: Val::Px(SIZE * 0.7), height: Val::Px(SIZE * 0.7), display: Display::None, ..default() },
                ImageNode { color: Color::srgb(1.0, 0.8, 0.25), ..ImageNode::new(base.clone()) },
                crate::icons::Tooltip(name),
                crate::hud::BlocksWorld,
                GlobalZIndex(-10),
                DespawnOnExit(PlayMode::Live),
            ))
            .with_children(|t| {
                if let Some(g) = glyph.clone() {
                    t.spawn((
                        ImageNode::new(g),
                        Node { position_type: PositionType::Absolute, left: Val::Percent(22.0), top: Val::Percent(20.0), width: Val::Percent(56.0), height: Val::Percent(56.0), ..default() },
                        Pickable::IGNORE,
                    ));
                }
            });
    }
}

/// The venue glyph of a community lot, from its name (as the rabbit holes are told apart).
fn venue_glyph(internal: &str) -> &'static str {
    let n = internal.to_ascii_lowercase();
    let has = |k: &str| n.contains(k);
    [
        ("gym", "gym"),
        ("library", "library"),
        ("bookstore", "bookstore"),
        ("grocery", "grocery"),
        ("bistro", "eatery"),
        ("diner", "eatery"),
        ("restaurant", "eatery"),
        ("bar", "bar"),
        ("theat", "show"),
        ("stadium", "stadium"),
        ("spa", "spa"),
        ("salon", "spa"),
        ("pool", "pool"),
        ("museum", "museum"),
        ("science", "science"),
        ("hospital", "hospital"),
        ("school", "school"),
        ("police", "police"),
        ("fire", "firestation"),
        ("military", "military"),
        ("cityhall", "cityhall"),
        ("city hall", "cityhall"),
        ("townhall", "cityhall"),
        ("business", "business"),
        ("office", "business"),
        ("graveyard", "graveyard"),
        ("cemetery", "graveyard"),
        ("mausoleum", "graveyard"),
        ("park", "park"),
        ("beach", "park"),
        ("pond", "park"),
        ("garden", "park"),
        ("fishing", "park"),
        ("square", "park"),
    ]
    .iter()
    .find(|(k, _)| has(k))
    .map_or("park", |(_, g)| g)
}

#[allow(clippy::too_many_arguments)]
fn spawn_tags(
    mut commands: Commands,
    existing: Query<(), With<MapTag>>,
    world: Res<CurrentWorld>,
    household: Option<Res<Household>>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut ui) = ui else { return };
    if !existing.is_empty() {
        return;
    }
    let home = household.as_ref().map(|h| h.lot_index);
    let Some(base) = ui.icon(&mut images, "hud_icon_maptagbase_r2") else { return };
    for (i, lot) in world.data.lots.iter().enumerate() {
        let (glyph, tint) = if Some(i) == home {
            ("home_active", Color::srgb(0.45, 0.9, 0.35))
        } else if lot.is_residential() {
            let built = world.data.buildings.get(&i).is_some_and(|b| !b.walls.is_empty());
            (if built { "home" } else { "lot_empty" }, Color::srgb(0.95, 0.95, 0.92))
        } else {
            (venue_glyph(&lot.internal_name), Color::srgb(0.35, 0.6, 1.0))
        };
        let name = world.data.lot_names.get(i).map_or("", |s| s.as_str());
        let title = if lot.is_residential() { name.split(" — ").next().unwrap_or(name).to_string() } else { crate::rabbitholes::lot_title(lot, name) };
        let icon = ui.icon(&mut images, &format!("maptag_{glyph}"));
        let at = crate::home::lot_center(lot) + Vec3::Y * 6.0;
        commands
            .spawn((
                MapTag { lot: i, at },
                Button,
                Node { position_type: PositionType::Absolute, width: Val::Px(SIZE), height: Val::Px(SIZE), display: Display::None, ..default() },
                ImageNode { color: tint, ..ImageNode::new(base.clone()) },
                crate::icons::Tooltip(if title.is_empty() { "Lot".into() } else { title }),
                crate::hud::BlocksWorld,
                GlobalZIndex(-10),
            ))
            .with_children(|t| {
                if let Some(g) = icon {
                    t.spawn((
                        ImageNode::new(g),
                        Node { position_type: PositionType::Absolute, left: Val::Percent(22.0), top: Val::Percent(20.0), width: Val::Percent(56.0), height: Val::Percent(56.0), ..default() },
                        Pickable::IGNORE,
                    ));
                }
            });
    }
}

fn place_tags(cams: Query<(&Camera, &GlobalTransform, &SimsCamera)>, mut tags: Query<(&MapTag, &mut Node)>) {
    let Ok((cam, tf, sims_cam)) = cams.single() else { return };
    let on = sims_cam.distance > MAP_DISTANCE;
    for (tag, mut node) in &mut tags {
        let shown = on.then(|| cam.world_to_viewport(tf, tag.at).ok()).flatten().filter(|_| (tag.at - tf.translation()).dot(tf.forward().as_vec3()) > 0.0);
        match shown {
            Some(p) => {
                node.display = Display::Flex;
                node.left = Val::Px(p.x - SIZE * 0.5);
                node.top = Val::Px(p.y - SIZE * 0.5);
            }
            None => node.display = Display::None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tag_clicks(
    mut commands: Commands,
    tags: Query<(&Interaction, &MapTag, Option<&CollectibleTag>), Changed<Interaction>>,
    mut queues: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>,
    world: Res<CurrentWorld>,
    selected: Query<Entity, With<crate::sim::Selected>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut pie: ResMut<crate::hud::PieMenu>,
    mut cam: Query<&mut SimsCamera>,
    (ui, opps, clock): (Option<Res<crate::icons::GameUi>>, Query<Option<&crate::opportunities::SimOpportunities>, With<crate::sim::Selected>>, Res<crate::clock::GameClock>),
) {
    for (i, tag, collectible) in &tags {
        if *i != Interaction::Pressed {
            continue;
        }
        // A collectible: off to collect it (or to the lot where they turn up).
        if let Some(c) = collectible {
            let def = crate::interact::interactions_for(ObjectKind::Collectible).iter().position(|d| d.name == "Collect");
            let Ok(mut q) = queues.single_mut() else { continue };
            match (c, def) {
                (CollectibleTag::Find(f), Some(def)) => q.push_player(crate::interact::Action::new("Collect", ActionKind::Object { target: *f, def }, false)),
                (CollectibleTag::Spawner(lot), _) if crate::visit::visitable(&world.data, *lot) => {
                    q.push_player(crate::interact::Action::new("Visit", ActionKind::GoToLot { lot: *lot }, false))
                }
                _ => {}
            }
            continue;
        }
        let Some(lot) = world.data.lots.get(tag.lot) else { continue };
        let acts = crate::rabbitholes::activities(lot);
        let mut tasks = ui.as_ref().map(|u| crate::opportunities::lot_options(&world.data, tag.lot, opps.single().ok().flatten(), &u.data, clock.hour_f())).unwrap_or_default();
        tasks.extend(crate::gardening::lot_options(&world.data, tag.lot));
        if crate::visit::visitable(&world.data, tag.lot) {
            tasks.insert(0, ("Visit".to_string(), ActionKind::GoToLot { lot: tag.lot }));
        }
        if !lot.is_residential() && (!acts.is_empty() || !tasks.is_empty()) {
            let (Ok(actor), Some(cursor)) = (selected.single(), windows.single().ok().and_then(|w| w.cursor_position())) else { continue };
            let name = world.data.lot_names.get(tag.lot).map_or("", |s| s.as_str());
            let mut options: Vec<(String, ActionKind)> = acts.iter().enumerate().map(|(k, a)| (crate::hud::activity_label(a), ActionKind::Visit { lot: tag.lot, activity: k })).collect();
            options.extend(tasks);
            pie.at = cursor;
            crate::hud::open_pie(&mut commands, &mut pie, cursor, &crate::rabbitholes::lot_title(lot, name), actor, options);
        } else if let Ok(mut c) = cam.single_mut() {
            c.look_at(crate::home::lot_center(lot));
            c.distance = 45.0;
        }
    }
}
