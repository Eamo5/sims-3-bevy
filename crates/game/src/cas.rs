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
        app.init_resource::<CasSelected>()
            .init_resource::<CasTurn>()
            .init_resource::<CasFamilies>()
            .add_message::<CasActionRequest>()
            .add_systems(OnEnter(AppState::CreateHousehold), setup_cas)
            .add_systems(
                Update,
                (cas_actions, mirror_state, rebuild_ui, rebuild_model, turn_model, cas_button_visuals, scroll_panel, frame_camera).chain().run_if(in_state(AppState::CreateHousehold)),
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
    /// Voice `i` (of the three).
    Voice(u8),
    /// Their life state: human (none) or one of the supernaturals (`sim::Occult::ALL`).
    LifeState(Option<usize>),
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
    /// Dress the outfit `i` of `WEAR` (everyday, formal...).
    Wear(usize),
    /// Favourite food (0), music (1) or colour (2): entry `i` of its list.
    Favorite(u8, usize),
    /// The worn item of the tab's type in colourway `i` (the game's presets for it).
    Colourway(u8),
    /// Create a Style for the worn item: open or close it, and a channel's colour (of the
    /// palette).
    Styling,
    StyleColour(u8, u8),
    /// Sculpting the face: a part (head, eyes, nose, mouth), a slider moved one way or the
    /// other, or the face made afresh.
    FaceArea(u8),
    FaceSlider(u8, i8),
    RandomFace,
    Done,
}

/// The face sliders by part of the face, as Create a Sim groups them: each slider's pair (in
/// `FACE_SLIDERS`), name and ends (its second end first).
const FACE_AREAS: [(&str, &[(usize, &str, &str, &str)]); 4] = [
    (
        "Head",
        &[
            (16, "Head Width", "Narrow", "Wide"),
            (0, "Jaw Width", "Narrow", "Wide"),
            (1, "Chin Size", "Small", "Large"),
            (2, "Chin Height", "Low", "High"),
            (14, "Cheekbones", "Low", "High"),
            (15, "Cheeks", "Hollow", "Full"),
        ],
    ),
    ("Eyes", &[(6, "Eye Size", "Small", "Large"), (7, "Eye Spacing", "Close", "Apart"), (8, "Eye Height", "Low", "High"), (9, "Brows", "Low", "High")]),
    ("Nose", &[(10, "Nose Size", "Small", "Large"), (11, "Nose Width", "Narrow", "Wide"), (12, "Nose Tilt", "Down", "Up"), (13, "Nose Tip", "Small", "Large")]),
    ("Mouth", &[(3, "Mouth Width", "Narrow", "Wide"), (4, "Lips", "In", "Out"), (5, "Mouth Height", "Low", "High")]),
];

/// Styles per page (picture tiles).
const PAGE: usize = 20;

/// The outfits Create a Sim dresses, as the game's.
const WEAR: [crate::simbody::OutfitKind; 6] = [
    crate::simbody::OutfitKind::Everyday,
    crate::simbody::OutfitKind::Formal,
    crate::simbody::OutfitKind::Sleepwear,
    crate::simbody::OutfitKind::Athletic,
    crate::simbody::OutfitKind::Swimwear,
    crate::simbody::OutfitKind::Outerwear,
];

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
    /// The outfit being dressed (and shown).
    wear: crate::simbody::OutfitKind,
    /// Create a Style open.
    styling: bool,
    /// The part of the face being sculpted (`FACE_AREAS`).
    face_area: u8,
}

#[derive(Component)]
struct CasModel;

/// The Sim being made and the tab open (for the game's own frame: see `caslook`).
#[derive(Resource, Default)]
pub struct CasSelected(pub usize, pub CasTab);

impl Default for CasTab {
    fn default() -> Self {
        CasTab::Basics
    }
}

/// A turn of the Sim asked for by the puck's buttons.
#[derive(Resource, Default)]
pub struct CasTurn(pub f32);

/// Whether the town's families can be played from here.
#[derive(Resource, Default)]
pub struct CasFamilies(pub bool);

/// A CAS action asked for other than by a button press (the game's popup menus).
#[derive(Message, Clone, Copy)]
pub struct CasActionRequest(pub CasAction);

fn mirror_state(scene: Option<ResMut<CasScene>>, mut sel: ResMut<CasSelected>, mut fam: ResMut<CasFamilies>, mut framed: Local<bool>) {
    let Some(mut s) = scene else { return };
    // (The game's frame just up: the plain household panel goes.)
    let game_frame = crate::caslook::GAME_CAS.load(std::sync::atomic::Ordering::Relaxed);
    if game_frame != *framed {
        *framed = game_frame;
        s.dirty_ui = true;
    }
    if sel.0 != s.selected || sel.1 != s.tab {
        *sel = CasSelected(s.selected, s.tab);
    }
    if fam.0 != s.families.is_some() {
        fam.0 = s.families.is_some();
    }
}

/// Create a Sim's camera: on the whole Sim, or close on their face on the Face tab.
#[derive(Component)]
struct CasCamera;

/// The camera eased towards its framing for the tab: the face (at the Sim's head height for
/// their age) on the Face tab, else the whole Sim.
fn frame_camera(scene: Option<Res<CasScene>>, pending: Res<PendingHousehold>, time: Res<Time>, mut cam: Query<&mut Transform, With<CasCamera>>) {
    let (Some(scene), Ok(mut tf)) = (scene, cam.single_mut()) else { return };
    let (eye, at) = match pending.members.get(scene.selected).filter(|_| scene.tab == CasTab::Face && !scene.browsing) {
        Some(sim) => {
            let head = match sim.age {
                Age::Baby => 0.35,
                Age::Toddler => 0.68,
                Age::Child => 1.08,
                Age::Teen => 1.52,
                _ => 1.6,
            };
            (Vec3::new(0.06, head + 0.02, 1.1), Vec3::new(0.06, head - 0.02, 0.0))
        }
        None => (Vec3::new(-0.35, 1.05, 3.1), Vec3::new(-0.35, 0.92, 0.0)),
    };
    let want = Transform::from_translation(eye).looking_at(at, Vec3::Y);
    let k = (time.delta_secs() * 6.0).min(1.0);
    if tf.translation.distance(want.translation) < 1e-3 {
        return;
    }
    tf.translation = tf.translation.lerp(want.translation, k);
    tf.rotation = tf.rotation.slerp(want.rotation, k);
}

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
        wear: crate::simbody::OutfitKind::Everyday,
        styling: false,
        face_area: 0,
    });
    // The stage: camera, lights, pedestal.
    // (Ambient light belongs to the camera: alone it would bring a camera of its own, and the
    // interface would be laid out for that one.)
    commands.spawn((
        Camera3d::default(),
        CasCamera,
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

/// CAS parts of a type that fit the Sim (clothes for an outfit), sorted by name.
pub(crate) fn parts_for(cas: &CasData, sim: &Sim, t: u32, kind: crate::simbody::OutfitKind) -> Vec<(Key, String)> {
    let age = age_bits(sim.age);
    let gender = if sim.female { s3formats::sim::GENDER_FEMALE } else { s3formats::sim::GENDER_MALE };
    let mut v: Vec<(Key, String)> = cas
        .parts
        .iter()
        .filter(|p| p.baked && p.clothing_type == t && p.age_gender & age != 0 && p.age_gender & gender != 0 && !crate::simbody::is_uniform(&p.name))
        // Clothes for the outfit (every day: not the swimwear that's everyday wear too).
        .filter(|p| {
            !matches!(t, CT_TOP | CT_BOTTOM | CT_BODY | CT_SHOES)
                || match kind {
                    crate::simbody::OutfitKind::Everyday | crate::simbody::OutfitKind::Career => {
                        p.category & s3formats::sim::CAT_EVERYDAY != 0 && p.category & s3formats::sim::CAT_SWIM == 0
                    }
                    k => p.category & k.category() != 0,
                }
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
    let outfit = crate::simbody::pick_outfit_for(&scene.cas, sim, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look), scene.wear);
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
    (mut play, sounds): (MessageWriter<crate::sound::PlaySound>, Option<Res<crate::sound::Sounds>>),
    (mut renders, install, mut ready): (ResMut<crate::style::StyleRenders>, Option<Res<crate::data::InstallPath>>, MessageReader<crate::style::StyleReady>),
    mut requests: MessageReader<CasActionRequest>,
) {
    let Some(mut scene) = scene else { return };
    // A style made in Create a Style, rendered: worn.
    for r in ready.read() {
        if let Some(sim) = pending.members.iter_mut().find(|s| s.id == r.style_sim()) {
            sim.outfit.styles.retain(|s| s.part != r.style.part);
            sim.outfit.styles.push(r.style.clone());
            scene.dirty_model = true;
            scene.dirty_ui = true;
        }
    }
    let mut rng = rand::rng();
    // (Buttons pressed, and actions asked for by the game's menus.)
    let asked: Vec<CasAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| *a).chain(requests.read().map(|r| r.0)).collect();
    for action in asked {
        let action = &action;
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
                s.traits.retain(|t| t.allowed_at(s.age));
                s.traits.truncate(slots);
                while s.traits.len() < slots {
                    match crate::life::next_trait(None, &s.traits, s.age) {
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
            CasAction::LifeState(o) => {
                pending.members[k].occult = o.and_then(|i| crate::sim::Occult::ALL.get(i).copied());
            }
            CasAction::Voice(v) => {
                // The new voice, heard (a line of them checking themselves out).
                let s = &mut pending.members[k];
                s.voice = v.min(2);
                let stem = if s.female { "vo_cas_flavor_checkbA" } else { "vo_cas_flavor_checkaA" };
                if let Some(line) = sounds.as_deref().and_then(|snd| crate::sound::cas_line(snd, stem, s).or_else(|| crate::sound::cas_line(snd, "vo_cas_flavor_greetA", s))) {
                    play.write(crate::sound::PlaySound::ui(&line).with_volume(0.9));
                }
                model = false;
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
                    let list = parts_for(&scene.cas, &pending.members[k], t, scene.wear);
                    if let Some((key, _)) = list.get(scene.page * PAGE + i) {
                        let o = &mut pending.members[k].outfit;
                        match t {
                            CT_HAIR => o.hair = Some(*key),
                            _ => o.wear(scene.wear, t, *key),
                        }
                    }
                }
            }
            CasAction::Styling => {
                scene.styling = !scene.styling;
                model = false;
            }
            CasAction::StyleColour(ch, idx) => {
                model = false;
                if let Some(t) = scene.tab.clothing_type().filter(|t| *t != CT_HAIR)
                    && let Some(key) = worn(&scene, &pending.members[k], t)
                    && let Some(&colour) = crate::style::PALETTE.get(idx as usize)
                    && let Some(install) = install.as_ref()
                {
                    let sim = &mut pending.members[k];
                    sim.outfit.wear(scene.wear, t, key);
                    let preset = sim.outfit.designs.iter().find(|(p, _)| *p == key).map_or(0, |d| d.1);
                    // (On top of the style already made from this colourway, if any.)
                    let mut colours = sim.outfit.styles.iter().find(|s| s.part == key && s.preset == preset).map(|s| s.colours.clone()).unwrap_or_default();
                    colours.retain(|(c, _)| *c != ch);
                    colours.push((ch, colour));
                    colours.sort_by_key(|c| c.0);
                    renders.request(sim.id, crate::style::CustomStyle { part: key, preset, colours }, install.0.clone());
                }
            }
            CasAction::Colourway(i) => {
                if let Some(t) = scene.tab.clothing_type().filter(|t| *t != CT_HAIR)
                    && let Some(key) = worn(&scene, &pending.members[k], t)
                {
                    let o = &mut pending.members[k].outfit;
                    // (The item worn is kept, in its new colours.)
                    o.wear(scene.wear, t, key);
                    o.designs.retain(|(p, _)| *p != key);
                    // (A colourway chosen takes the place of a style made.)
                    o.styles.retain(|s| s.part != key);
                    if i > 0 {
                        o.designs.push((key, i));
                    }
                }
            }
            CasAction::Favorite(kind, i) => {
                let f = &mut pending.members[k].favorites;
                match kind {
                    0 => f.food = crate::sim::FAVORITE_FOODS[i.min(crate::sim::FAVORITE_FOODS.len() - 1)].to_string(),
                    1 => f.music = crate::sim::FAVORITE_MUSIC[i.min(crate::sim::FAVORITE_MUSIC.len() - 1)].to_string(),
                    _ => f.color = crate::sim::FAVORITE_COLORS[i.min(crate::sim::FAVORITE_COLORS.len() - 1)].to_string(),
                }
                model = false;
            }
            CasAction::Wear(i) => {
                scene.wear = WEAR[i.min(WEAR.len() - 1)];
                scene.page = 0;
            }
            CasAction::Trait(i) => {
                // Toggle trait i: remove it, or add it if there's a free slot and it fits.
                let s = &mut pending.members[k];
                let t = crate::life::Trait::ALL[i];
                if let Some(pos) = s.traits.iter().position(|x| *x == t) {
                    s.traits.remove(pos);
                } else if s.traits.len() < crate::life::trait_slots(s.age) && t.allowed_at(s.age) && t.compatible(&s.traits) {
                    s.traits.push(t);
                    // The Sim says something in character, where the game has a line for it.
                    if let Some(stem) = trait_line(t)
                        && let Some(line) = sounds.as_deref().and_then(|snd| crate::sound::cas_line(snd, stem, s))
                    {
                        play.write(crate::sound::PlaySound::ui(&line).with_volume(0.9));
                    }
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
            CasAction::FaceArea(a) => {
                scene.face_area = a;
                model = false;
            }
            CasAction::FaceSlider(i, d) => {
                // (From the face they have, sculpted.)
                let s = &mut pending.members[k];
                let mut f = crate::simbody::face_sliders(s);
                if let Some(v) = f.get_mut(i as usize) {
                    *v = (*v + d as f32 * 0.2).clamp(-1.0, 1.0);
                }
                s.face = f;
            }
            CasAction::RandomFace => {
                let s = &mut pending.members[k];
                s.face = (0..s3bake::gamedata::FACE_SLIDERS.len()).map(|_| rand::Rng::random_range(&mut rng, -1.0f32..1.0)).collect();
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
                let list = parts_for(&scene.cas, &pending.members[k], t, crate::simbody::OutfitKind::Everyday);
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
    // (In the outfit being dressed.)
    let outfit = crate::simbody::pick_outfit_for(&scene.cas, &sim, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(sim.look), scene.wear);
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
    mut turn: ResMut<CasTurn>,
) {
    let Some(mut scene) = scene else { return };
    // (The puck's turn buttons.)
    if turn.0 != 0.0 {
        scene.yaw += turn.0;
        turn.0 = 0.0;
    }
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
    renders: Res<crate::style::StyleRenders>,
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
    // Household (left; the game's own puck and character sheet instead, where they're up)
    let game_frame = crate::caslook::GAME_CAS.load(std::sync::atomic::Ordering::Relaxed);
    commands.entity(root).with_children(|r| {
        if !game_frame {
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
        }
        // Name plate (bottom centre; above the game's puck where it's up)
        r.spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(if game_frame { 170.0 } else { 24.0 }),
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
                // (With the game's character sheet, only the clothing's own kinds here.)
                let clothing = |t: CasTab| matches!(t, CasTab::Tops | CasTab::Bottoms | CasTab::Outfits | CasTab::Shoes);
                for t in CasTab::ALL {
                    if game_frame && !(clothing(t) && clothing(scene.tab)) {
                        continue;
                    }
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
                    // Their life state, as the Supernatural pack's Create a Sim has it.
                    if !sim.age.is_little() {
                        p.spawn(text("Life State", 16.0, Color::WHITE));
                        p.spawn(Node { column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|row| {
                            button(row, "Human", CasAction::LifeState(None), Val::Px(84.0), sim.occult.is_none(), 13.0);
                            for (i, o) in crate::sim::Occult::ALL.iter().enumerate() {
                                button(row, o.name(), CasAction::LifeState(Some(i)), Val::Px(84.0), sim.occult == Some(*o), 13.0);
                            }
                        });
                    }
                    // The three voices, each heard when chosen.
                    p.spawn(text("Voice", 16.0, Color::WHITE));
                    p.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                        for v in 0..3u8 {
                            button(row, format!("Voice {}", v + 1), CasAction::Voice(v), Val::Px(92.0), sim.voice == v, 14.0);
                        }
                    });
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
                    // Favourites: food, music and colour, by the game's pictures.
                    if !sim.age.is_little() {
                        let data = ui.as_ref().map(|u| u.data.clone());
                        let food_name = |key: &str| data.as_ref().and_then(|d| d.recipes.iter().find(|r| r.key == key).map(|r| r.name.clone())).unwrap_or_else(|| key.to_string());
                        let f = &sim.favorites;
                        p.spawn(text(
                            format!("Favorites: {} · {} · {}", food_name(&f.food), crate::sim::Favorites::music_name(&f.music), crate::sim::Favorites::color_name(&f.color)),
                            20.0,
                            Color::WHITE,
                        ));
                        let rows: [(u8, &str, Vec<(String, String)>, &str); 3] = [
                            (0, "food", crate::sim::FAVORITE_FOODS.iter().map(|k| (k.to_string(), food_name(k))).collect(), f.food.as_str()),
                            (1, "music", crate::sim::FAVORITE_MUSIC.iter().map(|m| (m.to_string(), crate::sim::Favorites::music_name(m).to_string())).collect(), f.music.as_str()),
                            (2, "color", crate::sim::FAVORITE_COLORS.iter().map(|c| (c.to_string(), crate::sim::Favorites::color_name(c).to_string())).collect(), f.color.as_str()),
                        ];
                        for (kind, pic, list, now) in rows {
                            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(3.0), row_gap: Val::Px(3.0), ..default() }).with_children(|grid| {
                                for (i, (key, name)) in list.iter().enumerate() {
                                    let on = key == now;
                                    let icon = ui.as_deref_mut().and_then(|u| u.icon(&mut images, &crate::sim::Favorites::icon(pic, key)));
                                    grid.spawn((
                                        Button,
                                        CasAction::Favorite(kind, i),
                                        Node {
                                            width: Val::Px(34.0),
                                            height: Val::Px(34.0),
                                            border: UiRect::all(Val::Px(if on { 3.0 } else { 0.0 })),
                                            border_radius: BorderRadius::all(Val::Px(6.0)),
                                            justify_content: JustifyContent::Center,
                                            align_items: AlignItems::Center,
                                            ..default()
                                        },
                                        BorderColor::all(PLUMBOB_GREEN),
                                        BackgroundColor(if on { Color::srgb(0.22, 0.55, 0.22) } else { BTN_NORMAL }),
                                        crate::icons::Tooltip(name.clone()),
                                    ))
                                    .with_children(|b| match icon {
                                        Some(h) => {
                                            b.spawn((ImageNode::new(h), Node { width: Val::Px(28.0), height: Val::Px(28.0), ..default() }, Pickable::IGNORE));
                                        }
                                        None => {
                                            b.spawn((text(name.chars().take(2).collect::<String>(), 11.0, Color::WHITE), Pickable::IGNORE));
                                        }
                                    });
                                }
                            });
                        }
                    }
                    let slots = crate::life::trait_slots(sim.age);
                    p.spawn(text(format!("Traits · {} of {slots}", sim.traits.len()), 20.0, Color::WHITE));
                    p.spawn(text("Click to add or remove. Traits that clash with chosen ones are dimmed.", 13.0, Color::srgb(0.75, 0.85, 1.0)));
                    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), ..default() }).with_children(|grid| {
                        for (i, t) in crate::life::Trait::ALL.iter().enumerate() {
                            if !t.allowed_at(sim.age) { continue; }
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
                    // The face's shape, part by part, on the game's sliders.
                    if sim.age != Age::Baby {
                        p.spawn(text("Face Shape", 16.0, Color::WHITE));
                        p.spawn(Node { column_gap: Val::Px(6.0), flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(6.0), ..default() }).with_children(|row| {
                            for (i, (name, _)) in FACE_AREAS.iter().enumerate() {
                                button(row, *name, CasAction::FaceArea(i as u8), Val::Px(78.0), scene.face_area as usize == i, 14.0);
                            }
                            button(row, "Random", CasAction::RandomFace, Val::Px(78.0), false, 14.0);
                        });
                        let face = crate::simbody::face_sliders(&sim);
                        for &(i, label, lo, hi) in FACE_AREAS[(scene.face_area as usize).min(FACE_AREAS.len() - 1)].1 {
                            let n = (face.get(i).copied().unwrap_or(0.0) * 5.0).round() as i32;
                            let bar: String = (0..11).map(|j| if j == n + 5 { '●' } else { '·' }).collect();
                            p.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                row.spawn((text(label, 14.0, Color::WHITE), Node { width: Val::Px(104.0), ..default() }));
                                button(row, "<", CasAction::FaceSlider(i as u8, -1), Val::Px(32.0), false, 15.0);
                                row.spawn((text(lo, 12.0, Color::srgb(0.75, 0.85, 1.0)), Node { width: Val::Px(50.0), justify_content: JustifyContent::End, ..default() }));
                                row.spawn(text(bar, 15.0, Color::srgb(1.0, 0.95, 0.7)));
                                row.spawn((text(hi, 12.0, Color::srgb(0.75, 0.85, 1.0)), Node { width: Val::Px(44.0), ..default() }));
                                button(row, ">", CasAction::FaceSlider(i as u8, 1), Val::Px(32.0), false, 15.0);
                            });
                        }
                    }
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
                        let list = parts_for(&scene.cas, &sim, t, crate::simbody::OutfitKind::Everyday);
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
                    // Clothes are chosen for each outfit: everyday, formal, sleepwear, athletic, swimwear.
                    if t != CT_HAIR {
                        p.spawn(Node { column_gap: Val::Px(6.0), ..default() }).with_children(|row| {
                            for (i, k) in WEAR.iter().enumerate() {
                                button(row, k.label().to_string(), CasAction::Wear(i), Val::Px(92.0), *k == scene.wear, 13.0);
                            }
                        });
                    }
                    let list = parts_for(&scene.cas, &sim, t, if t == CT_HAIR { crate::simbody::OutfitKind::Everyday } else { scene.wear });
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
                    // (Create a Style open on the worn item: its colour picker in place of the list.)
                    let styling_now = scene.styling
                        && current.filter(|_| t != CT_HAIR).and_then(|k| scene.cas.colourways.get(&k).map(|w| (k, w))).is_some_and(|(k, w)| {
                            let on = sim.outfit.designs.iter().find(|(p, _)| *p == k).map_or(0, |d| d.1);
                            w.channels.get(on as usize).is_some_and(|c| !c.is_empty())
                        });
                    if !styling_now {
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
                    // The worn item's colourways (the game's presets for it), as swatches.
                    if let Some(ways) = current.filter(|_| t != CT_HAIR).and_then(|k| scene.cas.colourways.get(&k).map(|w| (k, w.clone()))) {
                        let (key, ways) = ways;
                        let on = sim.outfit.designs.iter().find(|(p, _)| *p == key).map_or(0, |d| d.1);
                        let styled = sim.outfit.styles.iter().find(|s| s.part == key && s.preset == on).cloned();
                        if ways.swatches.iter().filter(|s| s.is_some()).count() > 1 {
                        p.spawn(text("Colors", 16.0, Color::WHITE));
                        p.spawn(Node { column_gap: Val::Px(8.0), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|row| {
                            for (i, sw) in ways.swatches.iter().enumerate() {
                                let Some([r, g, b]) = *sw else { continue };
                                let chosen = i as u8 == on;
                                row.spawn((
                                    Button,
                                    Swatch,
                                    CasAction::Colourway(i as u8),
                                    Node {
                                        width: Val::Px(40.0),
                                        height: Val::Px(40.0),
                                        border: UiRect::all(Val::Px(if chosen { 4.0 } else { 1.0 })),
                                        border_radius: BorderRadius::all(Val::Px(20.0)),
                                        ..default()
                                    },
                                    BorderColor::all(if chosen { PLUMBOB_GREEN } else { Color::WHITE }),
                                    BackgroundColor(Color::srgb(r, g, b)),
                                ));
                            }
                        });
                        }
                        // Create a Style: each of the colourway's colour channels, any colour of
                        // the palette.
                        let channels = ways.channels.get(on as usize).cloned().unwrap_or_default();
                        if !channels.is_empty() {
                            button(p, if scene.styling { "Close Create a Style" } else { "Create a Style" }, CasAction::Styling, Val::Px(220.0), scene.styling, 15.0);
                        }
                        if scene.styling && !channels.is_empty() {
                            if renders.busy() {
                                p.spawn(text("Restyling…", 14.0, Color::srgb(1.0, 0.9, 0.5)));
                            }
                            for (ch, own) in channels {
                                let now = styled.as_ref().and_then(|s| s.colours.iter().find(|c| c.0 == ch)).map_or(own, |c| c.1);
                                p.spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                    row.spawn((text(format!("Color {}", (b'A' + ch) as char), 14.0, Color::WHITE), Node { width: Val::Px(64.0), ..default() }));
                                    row.spawn((
                                        Node { width: Val::Px(30.0), height: Val::Px(30.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(6.0)), ..default() },
                                        BorderColor::all(Color::WHITE),
                                        BackgroundColor(Color::srgb(now[0], now[1], now[2])),
                                    ));
                                });
                                p.spawn(Node { column_gap: Val::Px(3.0), row_gap: Val::Px(3.0), flex_wrap: FlexWrap::Wrap, max_width: Val::Px(400.0), ..default() }).with_children(|row| {
                                    for (i, [r, g, b]) in crate::style::PALETTE.iter().enumerate() {
                                        row.spawn((
                                            Button,
                                            Swatch,
                                            CasAction::StyleColour(ch, i as u8),
                                            Node { width: Val::Px(22.0), height: Val::Px(22.0), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
                                            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                                            BackgroundColor(Color::srgb(*r, *g, *b)),
                                        ));
                                    }
                                });
                            }
                        }
                    }
                }
            }
        });
    });
    scene.ui = Some(root);
}

/// The Create a Sim line a Sim says on taking a trait, where the game has one.
fn trait_line(t: crate::life::Trait) -> Option<&'static str> {
    use crate::life::Trait as T;
    Some(match t {
        T::Artistic => "vo_cas_trait_artA",
        T::ComputerWhiz => "vo_cas_trait_compA",
        T::FamilyOriented => "vo_cas_trait_famA",
        T::Friendly => "vo_cas_trait_friendlyA",
        T::Good => "vo_cas_trait_goodA",
        T::Hydrophobic => "vo_cas_trait_hydroA",
        T::Inappropriate => "vo_cas_trait_impA",
        T::GreatKisser => "vo_cas_trait_kisserA",
        T::LightSleeper => "vo_cas_trait_lsleepA",
        T::Lucky => "vo_cas_trait_luckyA",
        T::Mooch => "vo_cas_trait_moochA",
        T::Schmoozer => "vo_cas_trait_schmA",
        T::Technophobe => "vo_cas_trait_tphobeA",
        T::Unflirty => "vo_cas_trait_unflirtyA",
        T::Unlucky => "vo_cas_trait_unluckyA",
        T::Vegetarian => "vo_cas_trait_vegA",
        T::Workaholic => "vo_cas_trait_workA",
        _ => return None,
    })
}
