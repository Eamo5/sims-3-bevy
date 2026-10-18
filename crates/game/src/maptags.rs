//! Map View tags: zoomed out over town, every lot gets the game's map tag (its tag base, coloured
//! by kind, with the venue's glyph from the game's map-tag atlas): the household's home with a
//! star, other homes, empty lots, and each community venue. Hovering shows the lot's name;
//! clicking a venue offers its activities, clicking a home flies the camera there.
//!
//! The household's Sims have theirs too, their faces where they are (at home, out on a lot, in a
//! venue or at work: clicked, that's the Sim selected), and the selected Sim's opportunities
//! theirs (the opportunity's icon), at the venue where it's done or by the Sim it's done with.
//!
//! Each shows under its kind's filters (`mapfilter`), as the game's map-tag tuning has them: the
//! home always; other homes under Friends' Homes, empty lots only under All; venues under
//! Community Locations, and Careers too where a household Sim works; the household's Sims under
//! Home and Work (the selected one always); opportunities under Home and Work and Opportunities;
//! collectibles under Community Locations; a rabbit hole a stake can be bought in under Real
//! Estate. A lot a household Sim is out at is under Home and Work as well.

use bevy::prelude::*;

use crate::PlayMode;
use crate::camera::SimsCamera;
use crate::interact::{ActionKind, Household, ObjectKind};
use crate::loading::CurrentWorld;
use crate::mapfilter::{ALL, ALL_ONLY, FRIENDS_HOMES, HOUSEHOLD_AND_WORK, OPPORTUNITIES, PUBLIC_SPACES, REAL_ESTATE, WORK};

pub struct MapTagsPlugin;

impl Plugin for MapTagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_tags, collectible_tags, sim_tags, opportunity_tags, place_tags, tag_clicks, sim_tag_clicks).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), |mut c: Commands, q: Query<Entity, With<MapTag>>| {
                for e in &q {
                    c.entity(e).try_despawn();
                }
            });
    }
}

/// Above this camera distance the town shows its map tags.
pub(crate) const MAP_DISTANCE: f32 = 140.0;
const SIZE: f32 = 40.0;

#[derive(Component)]
struct MapTag {
    lot: usize,
    at: Vec3,
    /// The filters it shows under (`mapfilter`'s bits), as its kind has them.
    bits: u32,
}

/// A collectible's tag (shown by the Collection Helper): the gem, metal or space rock lying on
/// the lot being visited, or where they turn up on another lot (its rock spawner).
#[derive(Component)]
enum CollectibleTag {
    Find(Entity),
    Spawner(usize),
}

/// A household Sim's tag: their face, where they are.
#[derive(Component)]
struct SimTag(Entity);

/// An opportunity's tag: the opportunity (its index in the game data's), and the Sim it's done
/// with, if it's by them.
#[derive(Component)]
struct OppTag(usize, Option<Entity>);

/// The lot type a career's Sims work at (its rabbit hole: `opportunities::lot_types`).
fn workplace(career: &str) -> Option<&'static str> {
    Some(match career {
        "Business" | "Journalism" => "BusinessAndJournalism",
        "Culinary" => "Restaurant",
        "Political" => "CityHall",
        "Professional Sports" | "ProfessionalSports" | "Athletic" => "Stadium",
        "Science" => "ScienceLab",
        "Law Enforcement" | "LawEnforcement" => "PoliceStation",
        "Military" => "MilitaryBase",
        "Music" => "Theatre",
        "Medical" => "Hospital",
        "Criminal" => "Hideout",
        _ => return None,
    })
}

/// Where a household Sim is, for their tag: their own place when they're about, else the lot
/// they're out at (a venue, their work), else home.
#[allow(clippy::type_complexity)]
fn sim_tags(
    mut commands: Commands,
    members: Query<
        (Entity, &crate::sim::Sim, &GlobalTransform, &InheritedVisibility, Option<&crate::rabbitholes::AtRabbitHole>, Has<crate::careers::AtWork>, Option<&crate::careers::Job>),
        With<crate::sim::HouseholdMember>,
    >,
    mut tags: Query<(Entity, &SimTag, &mut MapTag, &mut crate::icons::Tooltip)>,
    world: Res<CurrentWorld>,
    household: Option<Res<Household>>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut portraits: ResMut<crate::portraits::Portraits>,
) {
    let Some(mut ui) = ui else { return };
    let home = household.as_ref().and_then(|h| world.data.lots.get(h.lot_index)).map(crate::home::lot_center);
    for (e, tag, ..) in &tags {
        if !members.contains(tag.0) {
            commands.entity(e).try_despawn();
        }
    }
    for (e, sim, tf, shown, out, at_work, job) in &members {
        let lot_of = |t: &str| (0..world.data.lots.len()).find(|&l| crate::opportunities::lot_types(&world.data, l).contains(&t));
        let (at, where_) = if shown.get() {
            (tf.translation(), "here".to_string())
        } else if let Some(r) = out.filter(|r| r.lot < world.data.lots.len()) {
            (crate::home::lot_center(&world.data.lots[r.lot]), format!("at {}", r.place))
        } else if let Some(l) = at_work.then(|| job.and_then(|j| workplace(j.career().name)).and_then(lot_of)).flatten() {
            (crate::home::lot_center(&world.data.lots[l]), "at work".to_string())
        } else if let Some(h) = home {
            (h, if at_work { "at work".to_string() } else { "out".to_string() })
        } else {
            continue;
        };
        let at = at + Vec3::Y * 9.0;
        let words = format!("{} ({where_})", sim.full_name());
        if let Some((_, _, mut t, mut tip)) = tags.iter_mut().find(|t| t.1.0 == e) {
            if t.at.distance(at) > 0.05 {
                t.at = at;
            }
            if tip.0 != words {
                tip.0 = words;
            }
            continue;
        }
        let Some(base) = ui.icon(&mut images, "hud_icon_maptagbase_r2") else { return };
        let face = portraits.portrait(&mut images, e);
        commands
            .spawn((
                MapTag { lot: usize::MAX, at, bits: HOUSEHOLD_AND_WORK },
                SimTag(e),
                Button,
                Node { position_type: PositionType::Absolute, width: Val::Px(SIZE), height: Val::Px(SIZE), display: Display::None, ..default() },
                ImageNode { color: Color::srgb(0.45, 0.9, 0.35), ..ImageNode::new(base) },
                crate::icons::Tooltip(words),
                crate::hud::BlocksWorld,
                GlobalZIndex(-9),
                DespawnOnExit(PlayMode::Live),
            ))
            .with_children(|t| {
                t.spawn((
                    ImageNode::new(face),
                    crate::portraits::PortraitOf(e),
                    Node { position_type: PositionType::Absolute, left: Val::Percent(16.0), top: Val::Percent(12.0), width: Val::Percent(68.0), height: Val::Percent(68.0), ..default() },
                    Pickable::IGNORE,
                ));
            });
    }
}

/// The selected Sim's opportunities in Map View: at each lot where one's venue task is done, and
/// by the Sim one's done with (when they're about), with the opportunity's icon; what's to be done
/// on hover.
#[allow(clippy::type_complexity)]
fn opportunity_tags(
    mut commands: Commands,
    selected: Query<Option<&crate::opportunities::SimOpportunities>, With<crate::sim::Selected>>,
    people: Query<(Entity, &crate::sim::Sim, &GlobalTransform, &InheritedVisibility)>,
    mut tags: Query<(Entity, &OppTag, &mut MapTag)>,
    world: Res<CurrentWorld>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Local<Vec<(usize, usize, Option<Entity>)>>,
) {
    let Some(mut ui) = ui else { return };
    let data = ui.data.clone();
    let opps = selected.single().ok().flatten();
    // (What's wanted: each opportunity's places, and the Sims it's with.)
    let mut want: Vec<(usize, usize, Option<Entity>, Vec3, String)> = Vec::new();
    for a in opps.map(|o| o.active.as_slice()).unwrap_or_default() {
        let Some(o) = data.opportunities.get(a.index) else { continue };
        let with = a.with.as_ref().map(|w| w.1.as_str());
        let words = format!("{}: {}", o.name, crate::opportunities::objective_with(o, a.activity.name, &data, with));
        if let Some((id, _)) = &a.with {
            if let Some((e, _, tf, _)) = people.iter().find(|p| p.1.id == *id && p.3.get()) {
                want.push((a.index, usize::MAX, Some(e), tf.translation() + Vec3::Y * 9.0, words));
            }
        } else if !o.rabbit_hole.is_empty() && !a.visited {
            for l in (0..world.data.lots.len()).filter(|&l| crate::opportunities::lot_types(&world.data, l).contains(&o.rabbit_hole.as_str())) {
                want.push((a.index, l, None, crate::home::lot_center(&world.data.lots[l]) + Vec3::Y * 14.0, words.clone()));
            }
        }
    }
    let keys: Vec<(usize, usize, Option<Entity>)> = want.iter().map(|w| (w.0, w.1, w.2)).collect();
    if *shown != keys {
        for (e, ..) in &tags {
            commands.entity(e).try_despawn();
        }
        *shown = keys;
        let Some(base) = ui.icon(&mut images, "hud_icon_maptagbase_r2") else { return };
        for (index, lot, with, at, words) in want {
            let icon = data.opportunities.get(index).and_then(|o| ui.icon(&mut images, &o.icon));
            commands
                .spawn((
                    MapTag { lot: if with.is_some() { usize::MAX } else { lot }, at, bits: HOUSEHOLD_AND_WORK | OPPORTUNITIES },
                    OppTag(index, with),
                    Button,
                    Node { position_type: PositionType::Absolute, width: Val::Px(SIZE * 0.85), height: Val::Px(SIZE * 0.85), display: Display::None, ..default() },
                    ImageNode { color: Color::srgb(1.0, 0.82, 0.3), ..ImageNode::new(base.clone()) },
                    crate::icons::Tooltip(words),
                    crate::hud::BlocksWorld,
                    GlobalZIndex(-8),
                    DespawnOnExit(PlayMode::Live),
                ))
                .with_children(|t| {
                    if let Some(g) = icon {
                        t.spawn((
                            ImageNode::new(g),
                            Node { position_type: PositionType::Absolute, left: Val::Percent(18.0), top: Val::Percent(14.0), width: Val::Percent(64.0), height: Val::Percent(64.0), ..default() },
                            Pickable::IGNORE,
                        ));
                    }
                });
        }
        return;
    }
    // (The Sims it's with move about.)
    for (_, tag, mut t) in &mut tags {
        if let Some(e) = tag.1
            && let Ok((_, _, tf, _)) = people.get(e)
        {
            let at = tf.translation() + Vec3::Y * 9.0;
            if t.at.distance(at) > 0.05 {
                t.at = at;
            }
        }
    }
}

/// A household Sim's tag clicked: they're selected, and the camera's on them; an opportunity's
/// Sim's: the camera's on them.
fn sim_tag_clicks(
    mut commands: Commands,
    tags: Query<(&Interaction, &MapTag, Option<&SimTag>, Option<&OppTag>), Changed<Interaction>>,
    selected: Query<Entity, With<crate::sim::Selected>>,
    mut cam: Query<&mut SimsCamera>,
) {
    for (i, tag, sim, opp) in &tags {
        if *i != Interaction::Pressed {
            continue;
        }
        let who = sim.map(|s| s.0).or(opp.and_then(|o| o.1));
        let Some(who) = who else { continue };
        if sim.is_some() {
            for s in &selected {
                commands.entity(s).remove::<crate::sim::Selected>();
            }
            commands.entity(who).insert(crate::sim::Selected);
        }
        if let Ok(mut c) = cam.single_mut() {
            c.look_at(tag.at - Vec3::Y * 9.0);
            c.distance = 30.0;
        }
    }
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
            commands.entity(e).try_despawn();
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
                MapTag { lot: usize::MAX, at, bits: PUBLIC_SPACES },
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
    existing: Query<Entity, With<MapTag>>,
    world: Res<CurrentWorld>,
    household: Option<Res<Household>>,
    ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut ui) = ui else { return };
    // (The town changed (a lot's type, say): its tags made afresh.)
    if world.is_changed() && !existing.is_empty() {
        for e in &existing {
            commands.entity(e).try_despawn();
        }
        return;
    }
    if !existing.is_empty() {
        return;
    }
    let home = household.as_ref().map(|h| h.lot_index);
    let Some(base) = ui.icon(&mut images, "hud_icon_maptagbase_r2") else { return };
    for (i, lot) in world.data.lots.iter().enumerate() {
        let (glyph, tint, bits) = if Some(i) == home {
            ("home_active", Color::srgb(0.45, 0.9, 0.35), ALL)
        } else if lot.is_residential() {
            let built = world.data.buildings.get(&i).is_some_and(|b| !b.walls.is_empty());
            if built { ("home", Color::srgb(0.95, 0.95, 0.92), FRIENDS_HOMES) } else { ("lot_empty", Color::srgb(0.95, 0.95, 0.92), ALL_ONLY) }
        } else {
            // (Real Estate too where a stake can be bought: `realestate`.)
            let estate = if crate::realestate::purchasable(&world.data, i) { REAL_ESTATE } else { 0 };
            (venue_glyph(&lot.internal_name), Color::srgb(0.35, 0.6, 1.0), PUBLIC_SPACES | estate)
        };
        let name = world.data.lot_names.get(i).map_or("", |s| s.as_str());
        let title = if lot.is_residential() { name.split(" — ").next().unwrap_or(name).to_string() } else { crate::rabbitholes::lot_title(lot, name) };
        let icon = ui.icon(&mut images, &format!("maptag_{glyph}"));
        let at = crate::home::lot_center(lot) + Vec3::Y * 6.0;
        commands
            .spawn((
                MapTag { lot: i, at, bits },
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

#[allow(clippy::type_complexity)]
fn place_tags(
    cams: Query<(&Camera, &GlobalTransform, &SimsCamera)>,
    mut tags: Query<(Entity, &MapTag, &mut Node, Option<&SimTag>, Has<OppTag>)>,
    filter: Res<crate::mapfilter::MapFilter>,
    members: Query<(Entity, Has<crate::sim::Selected>, Option<&crate::careers::Job>, Option<&crate::rabbitholes::AtRabbitHole>, Option<&crate::visit::OnLot>), With<crate::sim::HouseholdMember>>,
    world: Res<CurrentWorld>,
) {
    let Ok((cam, tf, sims_cam)) = cams.single() else { return };
    let on = sims_cam.distance > MAP_DISTANCE;
    // (Where the household's Sims work, and the lots they're out at.)
    let (mut work, mut out): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    if on {
        let types: Vec<&str> = members.iter().filter_map(|(_, _, job, ..)| job.and_then(|j| workplace(j.career().name))).collect();
        if !types.is_empty() {
            work = (0..world.data.lots.len()).filter(|&l| crate::opportunities::lot_types(&world.data, l).iter().any(|t| types.contains(t))).collect();
        }
        out = members.iter().filter_map(|(_, _, _, r, v)| r.map(|r| r.lot).or(v.map(|v| v.0))).collect();
    }
    let selected_sims: Vec<Entity> = members.iter().filter(|m| m.1).map(|m| m.0).collect();
    // (The lots' tags where they are; the Sims' and opportunities' moved along beside any they'd
    // cover, as the game's map keeps its tags apart.)
    let mut placed: Vec<Vec2> = Vec::new();
    let mut movable: Vec<(Entity, Vec2)> = Vec::new();
    for (e, tag, mut node, sim, opp) in &mut tags {
        let mut bits = tag.bits;
        if work.contains(&tag.lot) {
            bits |= WORK | HOUSEHOLD_AND_WORK;
        }
        if out.contains(&tag.lot) {
            bits |= HOUSEHOLD_AND_WORK;
        }
        if sim.is_some_and(|s| selected_sims.contains(&s.0)) {
            bits = ALL;
        }
        let shown = on.then(|| cam.world_to_viewport(tf, tag.at).ok()).flatten().filter(|_| filter.shows(bits) && (tag.at - tf.translation()).dot(tf.forward().as_vec3()) > 0.0);
        match shown {
            Some(p) if sim.is_some() || opp => movable.push((e, p)),
            Some(p) => {
                node.display = Display::Flex;
                node.left = Val::Px(p.x - SIZE * 0.5);
                node.top = Val::Px(p.y - SIZE * 0.5);
                placed.push(p);
            }
            None => node.display = Display::None,
        }
    }
    movable.sort_by(|a, b| a.0.cmp(&b.0));
    for (e, mut p) in movable {
        for _ in 0..8 {
            if !placed.iter().any(|q| q.distance(p) < SIZE * 0.8) {
                break;
            }
            p.x += SIZE * 0.85;
        }
        placed.push(p);
        if let Ok((_, _, mut node, ..)) = tags.get_mut(e) {
            node.display = Display::Flex;
            node.left = Val::Px(p.x - SIZE * 0.5);
            node.top = Val::Px(p.y - SIZE * 0.5);
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
    (ui, opps, clock): (Option<Res<crate::icons::GameUi>>, Query<(Option<&crate::opportunities::SimOpportunities>, Option<&crate::careers::Job>), With<crate::sim::Selected>>, Res<crate::clock::GameClock>),
    (town, story, dormant, household): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>, Option<Res<crate::interact::Household>>),
    learners: Query<(&crate::sim::Sim, Option<&crate::classes::ClassesTaken>)>,
    company_q: Query<(Entity, &crate::sim::Sim, Has<crate::interact::Visitor>, Option<&crate::visit::OnLot>), Or<(With<crate::sim::HouseholdMember>, With<crate::interact::Visitor>)>>,
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
        let (mine, job) = opps.single().ok().unwrap_or((None, None));
        let mut tasks = ui.as_ref().map(|u| crate::opportunities::lot_options(&world.data, tag.lot, mine, &u.data, crate::opportunities::Today::of(&clock, job))).unwrap_or_default();
        tasks.extend(crate::gardening::lot_options(&world.data, tag.lot));
        if let Ok((s, taken)) = learners.get(selected.single().unwrap_or(Entity::PLACEHOLDER)) {
            tasks.extend(crate::classes::lot_classes(lot, clock.hour_f(), clock.day(), s.age, taken, None).into_iter().map(|(label, task)| (label, ActionKind::Visit { lot: tag.lot, activity: task })));
            tasks.splice(0..0, crate::venues::world_lot_options(&world.data, tag.lot, clock.day(), clock.hour_f(), s.age).into_iter().map(|(label, task)| (label, ActionKind::Visit { lot: tag.lot, activity: task })));
        }
        if crate::visit::visitable(&world.data, tag.lot) {
            tasks.insert(0, ("Visit".to_string(), ActionKind::GoToLot { lot: tag.lot }));
        }
        // (A home the town's families live in: a visit to them.)
        let family = lot
            .is_residential()
            .then(|| crate::homes::family_at(&world, town.as_deref(), &story, &dormant, household.as_ref().map(|h| h.lot_index), tag.lot))
            .flatten();
        if let (Some(f), Ok(actor), Some(cursor)) = (family, selected.single(), windows.single().ok().and_then(|w| w.cursor_position())) {
            crate::homes::name_home(tag.lot, &f.name);
            pie.at = cursor;
            let options = vec![("Visit Lot".to_string(), ActionKind::GoToLot { lot: tag.lot })];
            crate::hud::open_pie(&mut commands, &mut pie, cursor, &format!("The {} Household", f.name), actor, options);
        } else if !lot.is_residential() && (!acts.is_empty() || !tasks.is_empty()) {
            let (Ok(actor), Some(cursor)) = (selected.single(), windows.single().ok().and_then(|w| w.cursor_position())) else { continue };
            let name = world.data.lot_names.get(tag.lot).map_or("", |s| s.as_str());
            let mut options: Vec<(String, ActionKind)> = acts.iter().enumerate().map(|(k, a)| (crate::hud::activity_label(a), ActionKind::Visit { lot: tag.lot, activity: k })).collect();
            // (Going out with someone about with the Sim, as the game's "...With".)
            let here = company_q.get(actor).ok().and_then(|c| c.3.map(|l| l.0));
            let company: Vec<String> = company_q
                .iter()
                .filter(|c| c.0 != actor && !c.1.age.is_little() && c.3.map(|l| l.0) == here && (!c.2 || here.is_none()))
                .map(|c| c.1.full_name())
                .collect();
            let outings: Vec<(String, usize)> = tasks
                .iter()
                .filter_map(|(_, k)| match k {
                    ActionKind::Visit { activity, .. } if crate::venues::groupable(*activity) => Some((crate::venues::outing_name(*activity)?.to_string(), *activity)),
                    _ => None,
                })
                .collect();
            options.extend(tasks);
            pie.submenus.clear();
            if !company.is_empty() {
                for (name, task) in outings {
                    let list: Vec<(String, ActionKind)> = company.iter().map(|c| (c.clone(), ActionKind::Visit { lot: tag.lot, activity: task })).collect();
                    options.push((format!("{name}{} ›", crate::venues::WITH), crate::hud::submenu_kind(pie.submenus.len())));
                    pie.submenus.push((format!("{name}{}", crate::venues::WITH), list));
                }
            }
            pie.at = cursor;
            crate::hud::open_pie(&mut commands, &mut pie, cursor, &crate::rabbitholes::lot_title(lot, name), actor, options);
        } else if let Ok(mut c) = cam.single_mut() {
            c.look_at(crate::home::lot_center(lot));
            c.distance = 45.0;
        }
    }
}
