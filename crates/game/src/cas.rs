//! Create-a-Sim: the household's Sims shown in 3D on a pedestal, idling with the game's own
//! animation, dressed from the real CAS parts — hair, tops, bottoms, outfits and shoes — with
//! skin tone, age, gender and traits.

use std::sync::Arc;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use s3bake::Key;
use s3formats::sim::{CT_BODY, CT_BOTTOM, CT_HAIR, CT_SHOES, CT_TOP, CT_BEARD, CT_EYESHADOW, CT_GLASSES, CT_LIPSTICK};

use crate::AppState;
use crate::baked::{Baked, BakedData};
use crate::home::PendingHousehold;
use crate::menu::{BTN_HOVER, BTN_NORMAL, BTN_PRESS, PLUMBOB_GREEN, text};
use crate::sim::*;
use crate::simbody::{CasData, SimRenderCtx, SimSkinMaterial, SimTextures};

pub struct CasPlugin;

impl Plugin for CasPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::CreateHousehold), setup_cas).add_systems(
            Update,
            (cas_actions, rebuild_ui, rebuild_model, turn_model, cas_button_visuals, scroll_panel).chain().run_if(in_state(AppState::CreateHousehold)),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CasTab {
    Basics,
    Hair,
    Tops,
    Bottoms,
    Outfits,
    Shoes,
    /// Facial hair, glasses and make-up.
    Face,
    Traits,
}

impl CasTab {
    const ALL: [CasTab; 8] = [CasTab::Basics, CasTab::Hair, CasTab::Tops, CasTab::Bottoms, CasTab::Outfits, CasTab::Shoes, CasTab::Face, CasTab::Traits];
    fn name(self) -> &'static str {
        match self {
            CasTab::Basics => "Basics",
            CasTab::Hair => "Hair",
            CasTab::Tops => "Tops",
            CasTab::Bottoms => "Bottoms",
            CasTab::Outfits => "Outfits",
            CasTab::Shoes => "Shoes",
            CasTab::Face => "Face",
            CasTab::Traits => "Traits",
        }
    }
    fn clothing_type(self) -> Option<u32> {
        match self {
            CasTab::Hair => Some(CT_HAIR),
            CasTab::Tops => Some(CT_TOP),
            CasTab::Bottoms => Some(CT_BOTTOM),
            CasTab::Outfits => Some(CT_BODY),
            CasTab::Shoes => Some(CT_SHOES),
            _ => None,
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub enum CasAction {
    Select(usize),
    Add,
    Remove,
    NewFamily,
    Randomize,
    RandomName,
    Gender,
    Age,
    Skin(usize),
    HairColor(usize),
    /// Eye colour `i` (of `sim::EYES`).
    EyeColor(usize),
    Tab(CasTab),
    /// Wear entry `i` of the current tab's list.
    Pick(usize),
    Page(i32),
    Trait(usize),
    /// Choose lifetime wish `i` (of `lifetime::LIFETIME_WISHES`).
    LifetimeWish(usize),
    /// Body shape: weight and fitness down (-1) or up (1).
    Weight(i8),
    Fitness(i8),
    /// Open or close the town's families.
    Families,
    /// Play town family `i` (of the playable ones).
    Family(usize),
    /// What two members are to each other: the next tie that fits (members by index).
    Tie(usize, usize),
    /// A face part of this clothing type: entry `i` of its list (`usize::MAX`: none).
    FacePart(u32, usize),
    Done,
}

/// Styles per page (picture tiles).
const PAGE: usize = 20;

#[derive(Resource)]
struct CasScene {
    baked: Arc<BakedData>,
    cas: CasData,
    selected: usize,
    tab: CasTab,
    page: usize,
    model: Option<Entity>,
    ui: Option<Entity>,
    dirty_model: bool,
    dirty_ui: bool,
    yaw: f32,
    /// The world's premade households, and whether they're being browsed.
    families: Option<Arc<s3bake::PremadesBaked>>,
    browsing: bool,
    /// How far the editing panel is scrolled down.
    scroll: f32,
    portrait: Option<Handle<Image>>,
}

#[derive(Component)]
struct CasModel;

fn setup_cas(
    mut commands: Commands,
    pending: Option<Res<PendingHousehold>>,
    selected_world: Option<Res<crate::data::SelectedWorld>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if pending.is_none() {
        commands.insert_resource(PendingHousehold::random());
    }
    let baked = match BakedData::open(s3bake::default_root(), None) {
        Ok(b) => Arc::new(b),
        Err(e) => {
            error!("CAS can't open the cache: {e}");
            return;
        }
    };
    let cas = CasData::from_baked(&baked);
    commands.insert_resource(Baked(baked.clone()));
    // The town's families (read from the world file the first time).
    let families = selected_world.and_then(|w| {
        let root = s3bake::default_root();
        let stem = w.0.path.file_stem()?.to_string_lossy().into_owned();
        if let Err(e) = s3bake::ensure_premades(&root, &w.0.path, &stem) {
            warn!("town families: {e}");
        }
        s3bake::load_premades(&root, &stem).filter(|p| p.playable().next().is_some()).map(Arc::new)
    });
    commands.insert_resource(CasScene {
        baked,
        cas,
        selected: 0,
        tab: CasTab::Basics,
        page: 0,
        model: None,
        ui: None,
        dirty_model: true,
        dirty_ui: true,
        yaw: 0.3,
        families,
        browsing: false,
        portrait: None,
        scroll: 0.0,
    });
    // The stage: camera, lights, pedestal.
    // (Ambient light belongs to the camera: alone it would bring a camera of its own, and the
    // interface would be laid out for that one.)
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-0.35, 1.05, 3.1).looking_at(Vec3::new(-0.35, 0.92, 0.0), Vec3::Y),
        AmbientLight { color: Color::srgb(0.85, 0.9, 1.0), brightness: 900.0, ..default() },
        DespawnOnExit(AppState::CreateHousehold),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(2.0, 4.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        DespawnOnExit(AppState::CreateHousehold),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.75, 0.08))),
        MeshMaterial3d(mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.88, 0.92), perceptual_roughness: 0.4, ..default() })),
        Transform::from_xyz(0.0, -0.04, 0.0),
        DespawnOnExit(AppState::CreateHousehold),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
        MeshMaterial3d(mats.add(StandardMaterial { base_color: Color::srgb(0.30, 0.45, 0.62), perceptual_roughness: 0.9, ..default() })),
        Transform::from_xyz(0.0, -0.08, 0.0),
        DespawnOnExit(AppState::CreateHousehold),
    ));
}

fn age_name(a: Age) -> &'static str {
    match a {
        Age::Baby => "Baby",
        Age::Toddler => "Toddler",
        Age::Child => "Child",
        Age::Teen => "Teen",
        Age::YoungAdult => "Young Adult",
        Age::Adult => "Adult",
        Age::Elder => "Elder",
    }
}

fn age_bits(a: Age) -> u32 {
    use s3formats::sim::*;
    match a {
        Age::Baby => AGE_BABY,
        Age::Toddler => AGE_TODDLER,
        Age::Child => AGE_CHILD,
        Age::Teen => AGE_TEEN,
        Age::YoungAdult => AGE_YOUNG_ADULT,
        Age::Adult => AGE_ADULT,
        Age::Elder => AGE_ELDER,
    }
}

/// CAS parts of a type that fit the Sim, sorted by name.
pub(crate) fn parts_for(cas: &CasData, sim: &Sim, t: u32) -> Vec<(Key, String)> {
    let age = age_bits(sim.age);
    let gender = if sim.female { s3formats::sim::GENDER_FEMALE } else { s3formats::sim::GENDER_MALE };
    let mut v: Vec<(Key, String)> = cas
        .parts
        .iter()
        .filter(|p| p.baked && p.clothing_type == t && p.age_gender & age != 0 && p.age_gender & gender != 0 && !crate::simbody::is_uniform(&p.name))
        // Clothes for every day (the swimwear, sleepwear and gym clothes are worn for those).
        .filter(|p| {
            !matches!(t, CT_TOP | CT_BOTTOM | CT_BODY | CT_SHOES)
                || (p.category & s3formats::sim::CAT_EVERYDAY != 0 && p.category & s3formats::sim::CAT_SWIM == 0)
        })
        .map(|p| (p.key, pretty_part(&p.name)))
        .collect();
    // The base game's first (its parts are in group 0), then the packs', by name.
    v.sort_by(|a, b| (a.0.1 != 0, &a.1).cmp(&(b.0.1 != 0, &b.1)));
    v.dedup_by(|a, b| a.0 == b.0);
    v
}

/// "afTopBlouseRuffle" -> "Blouse Ruffle".
fn pretty_part(name: &str) -> String {
    let rest = name.trim_start_matches(|c: char| c.is_ascii_lowercase());
    let rest = ["Hair", "Top", "Bottom", "Body", "Shoes", "Outfit"].iter().fold(rest, |r, p| r.strip_prefix(p).unwrap_or(r));
    let mut out = String::new();
    let mut prev_lower = false;
    for c in rest.chars() {
        if c == '_' {
            out.push(' ');
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        out.push(c);
    }
    let out = out.trim().to_string();
    if out.is_empty() { name.to_string() } else { out }
}

/// The part of a type the Sim is wearing now.
fn worn(scene: &CasScene, sim: &Sim, t: u32) -> Option<Key> {
    let outfit = crate::simbody::pick_outfit(&scene.cas, sim, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look));
    match t {
        CT_HAIR => outfit.hair.map(|p| p.key),
        _ => outfit.body.iter().find(|p| p.clothing_type == t).map(|p| p.key),
    }
}

#[allow(clippy::too_many_arguments)]
fn cas_actions(
    mut commands: Commands,
    q: Query<(&Interaction, &CasAction), Changed<Interaction>>,
    scene: Option<ResMut<CasScene>>,
    mut pending: ResMut<PendingHousehold>,
    mut next: ResMut<NextState<AppState>>,
    mut images: ResMut<Assets<Image>>,
    mut chosen: ResMut<crate::lifetime::ChosenLifetimeWishes>,
) {
    let Some(mut scene) = scene else { return };
    let mut rng = rand::rng();
    for (i, action) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        let k = scene.selected.min(pending.members.len().saturating_sub(1));
        let last = pending.last_name.clone();
        let mut model = true;
        match *action {
            CasAction::Select(i) => {
                scene.selected = i.min(pending.members.len() - 1);
                scene.page = 0;
            }
            CasAction::Add => {
                if pending.members.len() < 8 {
                    pending.members.push(random_sim(&mut rng, &last, None, Age::YoungAdult));
                    scene.selected = pending.members.len() - 1;
                }
            }
            CasAction::Remove => {
                if pending.members.len() > 1 {
                    let gone = pending.members[k].id;
                    pending.ties.retain(|(a, b, _)| *a != gone && *b != gone);
                    pending.members.remove(k);
                    scene.selected = k.saturating_sub(1);
                }
            }
            CasAction::NewFamily => {
                scene.portrait = None;
                *pending = PendingHousehold::random();
                scene.selected = 0;
            }
            CasAction::Randomize => {
                let s = &pending.members[k];
                let (f, age) = (s.female, s.age);
                pending.members[k] = random_sim(&mut rng, &last, Some(f), age);
            }
            CasAction::RandomName => {
                let s = &pending.members[k];
                let fresh = random_sim(&mut rng, &last, Some(s.female), s.age);
                pending.members[k].first = fresh.first;
                model = false;
            }
            CasAction::Gender => {
                let s = &mut pending.members[k];
                s.female = !s.female;
                s.outfit = OutfitChoice::default();
                let fresh = random_sim(&mut rng, &last, Some(s.female), s.age);
                s.first = fresh.first;
            }
            CasAction::Age => {
                let s = &mut pending.members[k];
                s.age = match s.age {
                    Age::Baby | Age::Toddler => Age::Child,
                    Age::Child => Age::Teen,
                    Age::Teen => Age::YoungAdult,
                    Age::YoungAdult => Age::Adult,
                    Age::Adult => Age::Elder,
                    Age::Elder => Age::Toddler,
                };
                s.outfit = OutfitChoice::default();
                let slots = crate::life::trait_slots(s.age);
                s.traits.truncate(slots);
                while s.traits.len() < slots {
                    match crate::life::next_trait(None, &s.traits) {
                        Some(t) => s.traits.push(t),
                        None => break,
                    }
                }
            }
            CasAction::Skin(i) => {
                let (r, g, b) = SKINS[i.min(SKINS.len() - 1)];
                pending.members[k].skin = Color::srgb(r, g, b);
            }
            CasAction::HairColor(i) => {
                let (r, g, b) = crate::sim::HAIRS[i.min(crate::sim::HAIRS.len() - 1)];
                pending.members[k].hair = Color::srgb(r, g, b);
            }
            CasAction::EyeColor(i) => {
                let (r, g, b) = crate::sim::EYES[i.min(crate::sim::EYES.len() - 1)];
                pending.members[k].eyes = Color::srgb(r, g, b);
            }
            CasAction::Tab(t) => {
                scene.tab = t;
                scene.page = 0;
                scene.scroll = 0.0;
                model = false;
            }
            CasAction::Page(d) => {
                scene.page = (scene.page as i32 + d).max(0) as usize;
                model = false;
            }
            CasAction::Pick(i) => {
                if let Some(t) = scene.tab.clothing_type() {
                    let list = parts_for(&scene.cas, &pending.members[k], t);
                    if let Some((key, _)) = list.get(scene.page * PAGE + i) {
                        let o = &mut pending.members[k].outfit;
                        match t {
                            CT_HAIR => o.hair = Some(*key),
                            CT_TOP => {
                                o.top = Some(*key);
                                o.full = None;
                            }
                            CT_BOTTOM => {
                                o.bottom = Some(*key);
                                o.full = None;
                            }
                            CT_BODY => {
                                o.full = Some(*key);
                                o.top = None;
                                o.bottom = None;
                            }
                            _ => o.shoes = Some(*key),
                        }
                    }
                }
            }
            CasAction::Trait(i) => {
                // Toggle trait i: remove it, or add it if there's a free slot and it fits.
                let s = &mut pending.members[k];
                let t = crate::life::Trait::ALL[i];
                if let Some(pos) = s.traits.iter().position(|x| *x == t) {
                    s.traits.remove(pos);
                } else if s.traits.len() < crate::life::trait_slots(s.age) && t.compatible(&s.traits) {
                    s.traits.push(t);
                }
                model = false;
            }
            CasAction::LifetimeWish(i) => {
                chosen.0.insert(pending.members[k].id, i);
                model = false;
            }
            CasAction::Weight(d) => {
                let s = &mut pending.members[k];
                s.weight = (s.weight + d as f32 * 0.2).clamp(-1.0, 1.0);
            }
            CasAction::Fitness(d) => {
                let s = &mut pending.members[k];
                s.fitness = (s.fitness + d as f32 * 0.1).clamp(0.0, 1.0);
            }
            CasAction::Families => {
                scene.browsing = !scene.browsing;
                model = false;
            }
            CasAction::Family(i) => {
                let Some(h) = scene.families.as_ref().and_then(|f| f.playable().nth(i)).cloned() else { continue };
                pending.members = h.members.iter().map(crate::premade::to_sim).collect();
                pending.last_name = h.name.clone();
                scene.portrait = h.portrait.and_then(|k| {
                    let bytes = std::fs::read(s3bake::default_root().tex_path(k)).ok()?;
                    crate::objects::dds_image(&bytes, true).map(|img| images.add(img))
                });
                pending.premade = Some(h);
                scene.selected = 0;
            }
            CasAction::FacePart(t, i) => {
                let list = parts_for(&scene.cas, &pending.members[k], t);
                let key = if i == usize::MAX { Some(crate::sim::OutfitChoice::NONE) } else { list.get(i).map(|p| p.0) };
                let o = &mut pending.members[k].outfit;
                match t {
                    CT_BEARD => o.beard = key,
                    CT_GLASSES => o.glasses = key,
                    CT_LIPSTICK => o.lipstick = key,
                    _ => o.eyeshadow = key,
                }
            }
            CasAction::Tie(i, j) => {
                use crate::family::Tie;
                let (Some(a), Some(b)) = (pending.members.get(i).cloned(), pending.members.get(j).cloned()) else { continue };
                let now = pending.ties.iter().find(|(x, y, _)| (*x, *y) == (a.id, b.id)).map_or(Tie::Roommates, |t| t.2);
                let at = Tie::ALL.iter().position(|t| *t == now).unwrap_or(0);
                let next_tie = (1..=Tie::ALL.len()).map(|d| Tie::ALL[(at + d) % Tie::ALL.len()]).find(|t| t.fits(a.age, b.age)).unwrap_or(Tie::Roommates);
                pending.ties.retain(|(x, y, _)| (*x, *y) != (a.id, b.id) && (*x, *y) != (b.id, a.id));
                if next_tie != Tie::Roommates {
                    pending.ties.push((a.id, b.id, next_tie));
                }
            }
            CasAction::Done => {
                // (What they are to each other, set once they've moved in.)
                if pending.premade.is_none() && pending.members.len() > 1 {
                    let fits = |a: u64, b: u64, t: crate::family::Tie| {
                        let age = |id: u64| pending.members.iter().find(|m| m.id == id).map(|m| m.age);
                        matches!((age(a), age(b)), (Some(x), Some(y)) if t.fits(x, y))
                    };
                    let ties = pending.ties.iter().copied().filter(|(a, b, t)| fits(*a, *b, *t)).collect();
                    commands.insert_resource(crate::family::HouseholdTies { members: pending.members.iter().map(|m| m.id).collect(), ties });
                }
                next.set(AppState::Loading);
                return;
            }
        }
        scene.dirty_ui = true;
        scene.dirty_model |= model;
    }
}

fn rebuild_model(
    mut commands: Commands,
    scene: Option<ResMut<CasScene>>,
    pending: Res<PendingHousehold>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut skin_mats: ResMut<Assets<SimSkinMaterial>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut textures: ResMut<SimTextures>,
) {
    let Some(mut scene) = scene else { return };
    if !scene.dirty_model {
        return;
    }
    scene.dirty_model = false;
    if let Some(m) = scene.model.take() {
        commands.entity(m).despawn();
    }
    let Some(sim) = pending.members.get(scene.selected).cloned() else { return };
    let outfit = crate::simbody::pick_outfit(&scene.cas, &sim, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look));
    let Some(model) = crate::simbody::build_sim_model(&scene.baked, &scene.cas, &sim, &outfit, crate::simbody::tone_of(&sim)) else {
        warn!("CAS: couldn't build a body for {}", sim.first);
        return;
    };
    let root = commands
        .spawn((
            CasModel,
            sim,
            SimAnim::default(),
            crate::anim::ClipPlayer::default(),
            Transform::from_rotation(Quat::from_rotation_y(scene.yaw)),
            Visibility::default(),
            DespawnOnExit(AppState::CreateHousehold),
        ))
        .id();
    let mut ctx = SimRenderCtx {
        meshes: &mut meshes,
        images: &mut images,
        mats: &mut mats,
        skin_mats: &mut skin_mats,
        bindposes: &mut bindposes,
        textures: &mut textures,
    };
    crate::simbody::spawn_sim_model(&mut commands, root, model, &mut ctx);
    scene.model = Some(root);
}

/// Drag with the mouse (or Q/E) to turn the Sim on the pedestal.
fn turn_model(
    scene: Option<ResMut<CasScene>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    windows: Query<&Window>,
    mut q: Query<&mut Transform, With<CasModel>>,
) {
    let Some(mut scene) = scene else { return };
    let over_model = windows.single().ok().and_then(|w| w.cursor_position().map(|c| c.x > w.width() * 0.32 && c.x < w.width() * 0.62)).unwrap_or(false);
    if mouse.pressed(MouseButton::Left) && over_model {
        scene.yaw += motion.delta.x * 0.01;
    }
    if keys.pressed(KeyCode::KeyQ) {
        scene.yaw += time.delta_secs() * 2.0;
    }
    if keys.pressed(KeyCode::KeyE) {
        scene.yaw -= time.delta_secs() * 2.0;
    }
    for mut tf in &mut q {
        tf.rotation = Quat::from_rotation_y(scene.yaw);
    }
}

fn cas_button_visuals(mut q: Query<(&Interaction, &mut BackgroundColor, &CasAction, Option<&Selectedness>, Has<Dimmed>), (Changed<Interaction>, Without<Swatch>)>) {
    for (i, mut bg, _, sel, dimmed) in &mut q {
        bg.0 = match i {
            Interaction::Pressed => BTN_PRESS,
            Interaction::Hovered => BTN_HOVER,
            Interaction::None if sel.is_some() => Color::srgb(0.22, 0.55, 0.22),
            Interaction::None if dimmed => DIMMED,
            Interaction::None => BTN_NORMAL,
        };
    }
}

/// A choice that isn't available right now.
#[derive(Component)]
struct Dimmed;

/// A colour swatch (keeps its own colour on hover).
#[derive(Component)]
struct Swatch;

const DIMMED: Color = Color::srgba(0.1, 0.18, 0.28, 0.6);

/// A button showing the current choice.
#[derive(Component)]
struct Selectedness;

fn button(p: &mut ChildSpawnerCommands, label: impl Into<String>, action: CasAction, w: Val, selected: bool, size: f32) {
    let mut e = p.spawn((
        Button,
        action,
        Node {
            border_radius: BorderRadius::all(Val::Px(8.0)),
            width: w,
            min_height: Val::Px(30.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(if selected { Color::srgb(0.22, 0.55, 0.22) } else { BTN_NORMAL }),
    ));
    if selected {
        e.insert(Selectedness);
    }
    e.with_children(|b| {
        b.spawn(text(label, size, Color::WHITE));
    });
}

/// The editing panel, which scrolls with the mouse wheel.
#[derive(Component)]
struct CasScroll;

fn scroll_panel(wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>, scene: Option<ResMut<CasScene>>, mut q: Query<(&mut ScrollPosition, &RelativeCursorPosition), With<CasScroll>>) {
    let Some(mut scene) = scene else { return };
    if wheel.delta.y == 0.0 {
        return;
    }
    let dy = match wheel.unit {
        bevy::input::mouse::MouseScrollUnit::Line => wheel.delta.y * 48.0,
        bevy::input::mouse::MouseScrollUnit::Pixel => wheel.delta.y,
    };
    for (mut pos, cursor) in &mut q {
        if cursor.cursor_over() {
            pos.0.y = (pos.0.y - dy).max(0.0);
            scene.scroll = pos.0.y;
        }
    }
}

fn panel_node(left: Option<f32>, right: Option<f32>, width: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: left.map_or(Val::Auto, Val::Px),
        right: right.map_or(Val::Auto, Val::Px),
        top: Val::Px(16.0),
        bottom: Val::Px(16.0),
        width: Val::Px(width),
        padding: UiRect::all(Val::Px(14.0)),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(8.0),
        border_radius: BorderRadius::all(Val::Px(12.0)),
        overflow: Overflow::scroll_y(),
        ..default()
    }
}

fn rebuild_ui(
    mut commands: Commands,
    scene: Option<ResMut<CasScene>>,
    pending: Res<PendingHousehold>,
    mut ui: Option<ResMut<crate::icons::GameUi>>,
    mut images: ResMut<Assets<Image>>,
    mut had_icons: Local<bool>,
    chosen: Res<crate::lifetime::ChosenLifetimeWishes>,
) {
    let Some(mut scene) = scene else { return };
    // (Redrawn once more when the game's icons become available.)
    if ui.is_some() && !*had_icons {
        *had_icons = true;
        scene.dirty_ui = true;
    }
    if !scene.dirty_ui {
        return;
    }
    scene.dirty_ui = false;
    if let Some(u) = scene.ui.take() {
        commands.entity(u).despawn();
    }
    let sel = scene.selected.min(pending.members.len().saturating_sub(1));
    let sim = pending.members[sel].clone();
    let panel_bg = BackgroundColor(crate::menu::PANEL_BG);
    let root = commands
        .spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, DespawnOnExit(AppState::CreateHousehold)))
        .id();
    // Household (left)
    commands.entity(root).with_children(|r| {
        r.spawn((panel_node(Some(16.0), None, 300.0), panel_bg)).with_children(|p| {
            p.spawn(text("Create a Sim", 30.0, Color::WHITE));
            p.spawn(text(format!("The {} Household", pending.last_name), 20.0, PLUMBOB_GREEN));
            for (i, m) in pending.members.iter().enumerate() {
                button(p, format!("{} · {}", m.first, age_name(m.age)), CasAction::Select(i), Val::Percent(100.0), i == sel, 16.0);
            }
            p.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|row| {
                button(row, "Add Sim", CasAction::Add, Val::Px(128.0), false, 15.0);
                button(row, "Remove", CasAction::Remove, Val::Px(128.0), false, 15.0);
            });
            button(p, "New Family", CasAction::NewFamily, Val::Percent(100.0), false, 15.0);
            // What they are to each other (click to change).
            if pending.members.len() > 1 && pending.premade.is_none() {
                p.spawn(text("Relationships", 16.0, PLUMBOB_GREEN));
                for i in 0..pending.members.len() {
                    for j in i + 1..pending.members.len() {
                        let (a, b) = (&pending.members[i], &pending.members[j]);
                        let tie = pending.ties.iter().find(|(x, y, _)| (*x, *y) == (a.id, b.id)).map_or(crate::family::Tie::Roommates, |t| t.2);
                        button(p, tie.describe(&a.first, &b.first), CasAction::Tie(i, j), Val::Percent(100.0), false, 13.0);
                    }
                }
            }
            if scene.families.is_some() {
                button(p, if scene.browsing { "Back to Create a Sim" } else { "Play a Town Family" }, CasAction::Families, Val::Percent(100.0), scene.browsing, 15.0);
            }
            p.spawn(Node { flex_grow: 1.0, ..default() });
            let done = if pending.premade.is_some() { format!("Play the {}s", pending.last_name) } else { "Done".to_string() };
            button(p, done, CasAction::Done, Val::Percent(100.0), false, 22.0);
        });
        // Name plate (bottom centre)
        r.spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(24.0),
                left: Val::Px(330.0),
                right: Val::Px(470.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|n| {
            n.spawn(text(sim.full_name(), 30.0, Color::WHITE));
            n.spawn(text(
                format!("{} {} · {}", if sim.female { "Female" } else { "Male" }, age_name(sim.age), sim.traits.iter().map(|t| t.name()).collect::<Vec<_>>().join(", ")),
                15.0,
                Color::srgb(0.95, 0.85, 0.55),
            ));
            n.spawn(text("Drag or use Q / E to turn", 12.0, Color::srgba(1.0, 1.0, 1.0, 0.6)));
        });
        // The town's families (right, while browsing)
        if scene.browsing && let Some(fam) = scene.families.clone() {
            r.spawn((panel_node(None, Some(16.0), 440.0), panel_bg)).with_children(|p| {
                p.spawn(text("Town Families", 26.0, Color::WHITE));
                p.spawn(text("The families who already live here. Pick one to play.", 13.0, Color::srgb(0.75, 0.85, 1.0)));
                if let Some(h) = &pending.premade {
                    if let Some(img) = &scene.portrait {
                        p.spawn((ImageNode::new(img.clone()), Node { width: Val::Px(200.0), height: Val::Px(200.0), align_self: AlignSelf::Center, ..default() }));
                    }
                    p.spawn(text(format!("The {} Household", h.name), 20.0, PLUMBOB_GREEN));
                    if !h.bio.is_empty() {
                        p.spawn(text(h.bio.clone(), 14.0, Color::WHITE));
                    }
                    for m in &h.members {
                        let s = crate::premade::to_sim(m);
                        let traits: Vec<&str> = s.traits.iter().map(|t| t.name()).collect();
                        p.spawn(text(format!("{} {} — {}", s.first, s.last, age_name(s.age)), 15.0, Color::srgb(0.95, 0.85, 0.55)));
                        if !traits.is_empty() {
                            p.spawn(text(traits.join(", "), 12.0, Color::srgb(0.8, 0.85, 0.95)));
                        }
                    }
                }
                p.spawn((text("All families", 18.0, Color::WHITE), Node { margin: UiRect::top(Val::Px(10.0)), ..default() }));
                let current = pending.premade.as_ref().map(|h| h.id);
                for (i, h) in fam.playable().enumerate() {
                    let label = format!("{} · {} Sim{} · §{}", h.name, h.members.len(), if h.members.len() == 1 { "" } else { "s" }, h.funds);
                    button(p, label, CasAction::Family(i), Val::Percent(100.0), current == Some(h.id), 15.0);
                }
            });
            return;
        }
        // Editing panel (right), scrolled where it was.
        r.spawn((panel_node(None, Some(16.0), 440.0), panel_bg, CasScroll, RelativeCursorPosition::default(), ScrollPosition(Vec2::new(0.0, scene.scroll)))).with_children(|p| {
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|tabs| {
                for t in CasTab::ALL {
                    button(tabs, t.name(), CasAction::Tab(t), Val::Auto, t == scene.tab, 15.0);
                }
            });
            match scene.tab {
                CasTab::Basics => {
                    p.spawn(text("Basics", 20.0, Color::WHITE));
                    button(p, format!("Gender: {}", if sim.female { "Female" } else { "Male" }), CasAction::Gender, Val::Percent(100.0), false, 16.0);
                    button(p, format!("Age: {}", age_name(sim.age)), CasAction::Age, Val::Percent(100.0), false, 16.0);
                    button(p, "New Name", CasAction::RandomName, Val::Percent(100.0), false, 16.0);
                    button(p, "Randomize Everything", CasAction::Randomize, Val::Percent(100.0), false, 16.0);
                    if !sim.age.is_little() {
                        let bar = |v: f32| {
                            let n = (v * 5.0).round() as i32;
                            (0..11).map(|i| if i == n + 5 { '●' } else { '·' }).collect::<String>()
                        };
                        for (label, value, down, up, lo, hi) in [
                            ("Weight", bar(sim.weight), CasAction::Weight(-1), CasAction::Weight(1), "Thin", "Heavy"),
                            ("Fitness", bar(sim.fitness * 2.0 - 1.0), CasAction::Fitness(-1), CasAction::Fitness(1), "Soft", "Fit"),
                        ] {
                            p.spawn(text(label, 16.0, Color::WHITE));
                            p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                button(row, "<", down, Val::Px(36.0), false, 16.0);
                                row.spawn(text(lo, 13.0, Color::srgb(0.75, 0.85, 1.0)));
                                row.spawn(text(value, 16.0, Color::srgb(1.0, 0.95, 0.7)));
                                row.spawn(text(hi, 13.0, Color::srgb(0.75, 0.85, 1.0)));
                                button(row, ">", up, Val::Px(36.0), false, 16.0);
                            });
                        }
                    }
                    p.spawn(text("Skin Tone", 16.0, Color::WHITE));
                    p.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                        let cur = sim.skin.to_srgba();
                        for (i, (r, g, b)) in SKINS.iter().enumerate() {
                            let on = (cur.red - r).abs() < 0.01 && (cur.green - g).abs() < 0.01;
                            row.spawn((
                                Button,
                                Swatch,
                                CasAction::Skin(i),
                                Node {
                                    width: Val::Px(52.0),
                                    height: Val::Px(52.0),
                                    border: UiRect::all(Val::Px(if on { 4.0 } else { 1.0 })),
                                    border_radius: BorderRadius::all(Val::Px(26.0)),
                                    ..default()
                                },
                                BorderColor::all(if on { PLUMBOB_GREEN } else { Color::WHITE }),
                                BackgroundColor(Color::srgb(*r, *g, *b)),
                            ));
                        }
                    });
                }
                CasTab::Traits => {
                    // The lifetime wish: five that suit the Sim's traits (teens and up), as
                    // icons, the chosen one named and described.
                    if !sim.age.is_little() && sim.age != Age::Child {
                        use crate::lifetime::{LIFETIME_WISHES, group, suggestions};
                        let pick = chosen.0.get(&sim.id).copied();
                        let mut options = suggestions(&sim, 5);
                        if let Some(c) = pick.filter(|c| !options.contains(c)) {
                            options.insert(0, c);
                            options.truncate(5);
                        }
                        let data = ui.as_ref().map(|u| u.data.clone());
                        p.spawn(text("Lifetime Wish", 20.0, Color::WHITE));
                        p.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                            for &i in &options {
                                let d = &LIFETIME_WISHES[i];
                                let on = pick == Some(i);
                                let icon = ui.as_deref_mut().and_then(|u| u.icon(&mut images, &d.icon(data.as_deref())));
                                let mut e = row.spawn((
                                    Button,
                                    CasAction::LifetimeWish(i),
                                    Node {
                                        width: Val::Px(62.0),
                                        height: Val::Px(62.0),
                                        border: UiRect::all(Val::Px(if on { 4.0 } else { 2.0 })),
                                        border_radius: BorderRadius::all(Val::Px(31.0)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderColor::all(if on { PLUMBOB_GREEN } else { Color::srgb(0.55, 0.75, 1.0) }),
                                    BackgroundColor(if on { Color::srgb(0.22, 0.55, 0.22) } else { BTN_NORMAL }),
                                    crate::icons::Tooltip(format!("{}\n{}\n{} lifetime happiness", d.name, d.describe(data.as_deref()), group(d.points(data.as_deref()) as i64))),
                                ));
                                if on {
                                    e.insert(Selectedness);
                                }
                                e.with_children(|b| match icon {
                                    Some(h) => {
                                        b.spawn((ImageNode::new(h), Node { width: Val::Px(46.0), height: Val::Px(46.0), ..default() }, Pickable::IGNORE));
                                    }
                                    None => {
                                        b.spawn((text(d.name.chars().next().unwrap_or('?').to_string(), 20.0, Color::WHITE), Pickable::IGNORE));
                                    }
                                });
                            }
                        });
                        match pick {
                            Some(i) => {
                                let d = &LIFETIME_WISHES[i];
                                p.spawn(text(d.name, 16.0, Color::srgb(1.0, 0.9, 0.5)));
                                p.spawn(text(
                                    format!("{} · {} lifetime happiness", d.describe(data.as_deref()), group(d.points(data.as_deref()) as i64)),
                                    13.0,
                                    Color::srgb(0.8, 0.88, 1.0),
                                ));
                            }
                            None => {
                                p.spawn(text("Pick the dream of this Sim's life: these suit their traits.", 13.0, Color::srgb(0.75, 0.85, 1.0)));
                            }
                        }
                    }
                    let slots = crate::life::trait_slots(sim.age);
                    p.spawn(text(format!("Traits · {} of {slots}", sim.traits.len()), 20.0, Color::WHITE));
                    p.spawn(text("Click to add or remove. Traits that clash with chosen ones are dimmed.", 13.0, Color::srgb(0.75, 0.85, 1.0)));
                    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), ..default() }).with_children(|grid| {
                        for (i, t) in crate::life::Trait::ALL.iter().enumerate() {
                            let chosen = sim.traits.contains(t);
                            let ok = chosen || (sim.traits.len() < slots && t.compatible(&sim.traits));
                            // The game's icon and description, when converted.
                            let info = ui.as_deref().and_then(|u| u.trait_info(*t));
                            let icon = match (ui.as_deref_mut(), &info) {
                                (Some(u), Some(i)) => u.icon(&mut images, &i.icon),
                                _ => None,
                            };
                            let mut e = grid.spawn((
                                Button,
                                CasAction::Trait(i),
                                Node {
                                    width: Val::Px(132.0),
                                    min_height: Val::Px(30.0),
                                    padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                                    column_gap: Val::Px(4.0),
                                    justify_content: if icon.is_some() { JustifyContent::FlexStart } else { JustifyContent::Center },
                                    align_items: AlignItems::Center,
                                    border_radius: BorderRadius::all(Val::Px(6.0)),
                                    ..default()
                                },
                                BackgroundColor(if chosen { Color::srgb(0.22, 0.55, 0.22) } else if ok { BTN_NORMAL } else { DIMMED }),
                            ));
                            if chosen {
                                e.insert(Selectedness);
                            } else if !ok {
                                e.insert(Dimmed);
                            }
                            if let Some(i) = info.as_ref().filter(|i| !i.desc.is_empty()) {
                                e.insert(crate::icons::Tooltip(format!("{}\n{}", i.name, i.desc)));
                            }
                            e.with_children(|b| {
                                if let Some(h) = icon {
                                    let color = if ok { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.35) };
                                    b.spawn((
                                        ImageNode { color, ..ImageNode::new(h) },
                                        Node { width: Val::Px(26.0), height: Val::Px(26.0), ..default() },
                                        Pickable::IGNORE,
                                    ));
                                }
                                b.spawn((text(t.name(), 12.0, if ok { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.4) }), Pickable::IGNORE));
                            });
                        }
                    });
                }
                CasTab::Face => {
                    // Facial hair (grown men), glasses, and make-up (women and teen girls).
                    let grown = matches!(sim.age, Age::YoungAdult | Age::Adult | Age::Elder);
                    let girl = sim.female && (grown || sim.age == Age::Teen);
                    let sections: Vec<(u32, &str, Option<s3bake::Key>)> = [
                        (CT_BEARD, "Facial Hair", !sim.female && grown, sim.outfit.beard),
                        (CT_GLASSES, "Glasses", !sim.age.is_little(), sim.outfit.glasses),
                        (CT_LIPSTICK, "Lipstick", girl, sim.outfit.lipstick),
                        (CT_EYESHADOW, "Eye Shadow", girl, sim.outfit.eyeshadow),
                    ]
                    .into_iter()
                    .filter(|s| s.2)
                    .map(|(t, n, _, c)| (t, n, c))
                    .collect();
                    // The eye colour, as swatches (shown as the iris looks: the colour's doubled
                    // over its shading).
                    p.spawn(text("Eye Color", 16.0, Color::WHITE));
                    p.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                        let cur = sim.eyes.to_srgba();
                        for (i, (r, g, b)) in crate::sim::EYES.iter().enumerate() {
                            let on = (cur.red - r).abs() < 0.01 && (cur.green - g).abs() < 0.01 && (cur.blue - b).abs() < 0.01;
                            row.spawn((
                                Button,
                                Swatch,
                                CasAction::EyeColor(i),
                                Node {
                                    width: Val::Px(34.0),
                                    height: Val::Px(34.0),
                                    border: UiRect::all(Val::Px(if on { 4.0 } else { 1.0 })),
                                    border_radius: BorderRadius::all(Val::Px(17.0)),
                                    ..default()
                                },
                                BorderColor::all(if on { PLUMBOB_GREEN } else { Color::WHITE }),
                                BackgroundColor(Color::srgb((r * 1.5).min(1.0), (g * 1.5).min(1.0), (b * 1.5).min(1.0))),
                            ));
                        }
                    });
                    for (t, title, current) in sections {
                        let list = parts_for(&scene.cas, &sim, t);
                        p.spawn(text(title, 16.0, Color::WHITE));
                        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(5.0), row_gap: Val::Px(5.0), ..default() }).with_children(|grid| {
                            let none = current == Some(crate::sim::OutfitChoice::NONE);
                            button(grid, "None", CasAction::FacePart(t, usize::MAX), Val::Px(56.0), none, 12.0);
                            for (i, (key, name)) in list.iter().enumerate().take(24) {
                                let chosen = current == Some(*key);
                                let thumb = ui.as_deref_mut().and_then(|u| u.icon(&mut images, &s3bake::gamedata::cas_thumb_name(key.2)));
                                let Some(thumb) = thumb else {
                                    button(grid, name.clone(), CasAction::FacePart(t, i), Val::Px(56.0), chosen, 10.0);
                                    continue;
                                };
                                grid.spawn((
                                    Button,
                                    CasAction::FacePart(t, i),
                                    Node {
                                        width: Val::Px(56.0),
                                        height: Val::Px(56.0),
                                        border: UiRect::all(Val::Px(if chosen { 3.0 } else { 0.0 })),
                                        border_radius: BorderRadius::all(Val::Px(6.0)),
                                        ..default()
                                    },
                                    BorderColor::all(PLUMBOB_GREEN),
                                    BackgroundColor(if chosen { Color::srgb(0.22, 0.55, 0.22) } else { BTN_NORMAL }),
                                    crate::icons::Tooltip(name.clone()),
                                ))
                                .with_children(|b| {
                                    b.spawn((ImageNode::new(thumb), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                                });
                            }
                        });
                    }
                }
                tab => {
                    let t = tab.clothing_type().unwrap();
                    let list = parts_for(&scene.cas, &sim, t);
                    let current = worn(&scene, &sim, t);
                    let pages = list.len().div_ceil(PAGE).max(1);
                    let page = scene.page.min(pages - 1);
                    scene.page = page;
                    if tab == CasTab::Hair {
                        // The hair colour, as swatches.
                        p.spawn(text("Hair Color", 16.0, Color::WHITE));
                        p.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                            let cur = sim.hair.to_srgba();
                            for (i, (r, g, b)) in crate::sim::HAIRS.iter().enumerate() {
                                let on = (cur.red - r).abs() < 0.01 && (cur.green - g).abs() < 0.01 && (cur.blue - b).abs() < 0.01;
                                row.spawn((
                                    Button,
                                    Swatch,
                                    CasAction::HairColor(i),
                                    Node {
                                        width: Val::Px(40.0),
                                        height: Val::Px(40.0),
                                        border: UiRect::all(Val::Px(if on { 4.0 } else { 1.0 })),
                                        border_radius: BorderRadius::all(Val::Px(20.0)),
                                        ..default()
                                    },
                                    BorderColor::all(if on { PLUMBOB_GREEN } else { Color::WHITE }),
                                    BackgroundColor(Color::srgb(*r, *g, *b)),
                                ));
                            }
                        });
                    }
                    p.spawn(text(format!("{} · {} styles", tab.name(), list.len()), 20.0, Color::WHITE));
                    // The game's pictures of each style (names on hover).
                    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() }).with_children(|grid| {
                        for (i, (key, name)) in list.iter().enumerate().skip(page * PAGE).take(PAGE) {
                            let action = CasAction::Pick(i - page * PAGE);
                            let chosen = Some(*key) == current;
                            let thumb = ui.as_deref_mut().and_then(|u| u.icon(&mut images, &s3bake::gamedata::cas_thumb_name(key.2)));
                            let Some(thumb) = thumb else {
                                button(grid, name.clone(), action, Val::Px(96.0), chosen, 12.0);
                                continue;
                            };
                            let mut e = grid.spawn((
                                Button,
                                action,
                                Node {
                                    width: Val::Px(96.0),
                                    height: Val::Px(96.0),
                                    border: UiRect::all(Val::Px(if chosen { 3.0 } else { 0.0 })),
                                    border_radius: BorderRadius::all(Val::Px(8.0)),
                                    ..default()
                                },
                                BorderColor::all(PLUMBOB_GREEN),
                                BackgroundColor(if chosen { Color::srgb(0.22, 0.55, 0.22) } else { BTN_NORMAL }),
                                crate::icons::Tooltip(name.clone()),
                            ));
                            if chosen {
                                e.insert(Selectedness);
                            }
                            e.with_children(|b| {
                                b.spawn((ImageNode::new(thumb), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                            });
                        }
                    });
                    p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        button(row, "< Prev", CasAction::Page(-1), Val::Px(110.0), false, 15.0);
                        row.spawn(text(format!("{} / {}", page + 1, pages), 15.0, Color::WHITE));
                        button(row, "Next >", CasAction::Page(1), Val::Px(110.0), false, 15.0);
                    });
                }
            }
        });
    });
    scene.ui = Some(root);
}
