//! Create a Pet: the Pets pack's half of Create a Sim, in the game's own panels (UI.dll's
//! `CASPuck` pet menu, `CAPPetSheet`, `CAPBasics`, `CAPBreeds` and `CAPCharacter`). The puck's
//! add button has the pack's menu (a Sim, a horse, a dog or a cat, or genetics); a new pet joins
//! the household's skewer after its Sims (up to six, ten in all: `kMaxPetsPerHousehold`) and
//! stands in the dressing room in the Sim's place, with the pet sheet beside it: Basics (its
//! name, the household's, male or female; grown, as Create a Pet makes them), Breed (the game's
//! breeds list, `CAPPetBreeds`: the mixed breed first, then by name, each with the game's
//! picture of it; dogs big or little), Character (its traits in their five sockets, the game's
//! traits picker for the pack's traits of its species, or a random set), and Accessories (a cat's
//! or dog's collars, a horse's bridles, saddles and the rest, each with the game's picture of it,
//! worn on the pet as chosen). Randomize draws another pet of its species. The pets come home with
//! the household, wearing what they were given.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::AppState;
use crate::cas::CasAction;
use crate::home::PendingHousehold;
use crate::layout::{SetIcon, Spawned, UiAssets, UiButton, UiScrollBar, UiSlider};
use crate::pets::SavedPet;

pub struct CapPlugin;

impl Plugin for CapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cap>()
            .add_message::<CapRequest>()
            .add_systems(Update, (requests, pet_stage, pet_sheet, basics_panel, breeds_panel, character_panel, accessories_panel, shape_panel, advanced_panel, coat_panel, tmh_panel).chain().run_if(in_state(AppState::CreateHousehold)))
            .add_systems(Update, cap_test)
            .add_systems(PreUpdate, minimap_hits.after(bevy::ui::UiSystems::Focus).run_if(in_state(AppState::CreateHousehold)))
            .add_systems(OnExit(AppState::CreateHousehold), |mut cap: ResMut<Cap>| {
                *cap = Cap::default();
                UP.store(false, Ordering::Relaxed);
            });
    }
}

/// Whether a pet's being made (Create a Sim's own panels and sheet step aside).
pub static UP: AtomicBool = AtomicBool::new(false);

/// As many pets as a household can have, and Sims and pets together (`kMaxPetsPerHousehold`,
/// `kMaxPerHousehold`).
pub const MAX_PETS: usize = 6;
pub const MAX_IN_HOUSEHOLD: usize = 10;

/// What the puck asks of Create a Pet.
#[derive(Message, Clone, Copy)]
pub enum CapRequest {
    /// A new pet of a kind (`ac`, `ad`, `ah`).
    New(&'static str),
    /// The pet being made taken out of the household.
    Remove,
}

/// A pet of the household's on the skewer.
#[derive(Component)]
pub struct PetButton(pub usize);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Panel {
    Basics,
    Breed,
    Character,
    Accessories,
    Shape,
    Coat,
}

/// Create a Pet's state: the pet being made, the panel open, the game's windows for them, the
/// pet standing in the room (and what it shows), the breeds' pictures.
#[derive(Resource, Default)]
pub struct Cap {
    pub pet: Option<usize>,
    panel: Option<Panel>,
    sheet: Option<Spawned>,
    basics: Option<Spawned>,
    breeds: Option<Spawned>,
    character: Option<Spawned>,
    accessories: Option<Spawned>,
    accessories_shown: Option<(usize, Vec<u64>, usize)>,
    acc_cells: Option<Entity>,
    acc_scroll: usize,
    /// What the pet in the room has on.
    worn: Vec<u64>,
    model: Option<(Entity, u64)>,
    thumbs: HashMap<u64, Handle<Image>>,
    sheet_shown: Option<(String, Option<Panel>)>,
    basics_shown: Option<(usize, String, String, bool, String)>,
    breeds_shown: Option<(usize, u64, bool, usize)>,
    character_shown: Option<(usize, Vec<String>)>,
    typing: Option<(u32, String)>,
    /// The breeds list's first row in view, the little dogs' list (not the big ones'), and its
    /// cells and scroll bar.
    scroll: usize,
    little: bool,
    cells: Option<Entity>,
    bar: Option<Entity>,
    picker: Option<crate::traitpicker::Picker>,
    /// Shape's panels, the region shown, its cells, bar and first row, and what they show.
    fbd: Option<Spawned>,
    face_body: Option<Spawned>,
    shape_region: u32,
    shape_cells: Option<Entity>,
    shape_bar: Option<Entity>,
    shape_scroll: usize,
    shape_shown: Option<(usize, String, u32, Vec<u64>, usize, bool)>,
    /// Shape's Advanced: its panel, whether it's up, the region (and whether on the body's
    /// picture), the region picked on the minimap, the hit masks, the sliders' rows, bar and first
    /// row, each slider (its entity, its index, its value last seen), and what they show.
    adv: Option<Spawned>,
    adv_open: bool,
    adv_region: u32,
    adv_body: bool,
    adv_pick: Option<u32>,
    adv_masks: HashMap<u64, Option<(u32, u32, Vec<u8>)>>,
    adv_rows: Option<Entity>,
    adv_bar: Option<Entity>,
    adv_scroll: usize,
    adv_sliders: Vec<(Entity, usize, f32)>,
    adv_shown: Option<(usize, String, bool, u32, usize)>,
    adv_slid: bool,
    /// Coat's panel and its frame, its cells, bar, first row and swatches, and what they show.
    coat: Option<Spawned>,
    ccm: Option<Spawned>,
    coat_cells: Option<Entity>,
    coat_bar: Option<Entity>,
    coat_scroll: usize,
    coat_swatches: Option<Entity>,
    coat_shown: Option<(usize, String, u64, Option<[u8; 3]>, usize)>,
    /// The coat frame's tab, and Tail and Mane's panel, cells, bar, first row and what they show.
    ccm_tab: CcmTab,
    tmh: Option<Spawned>,
    tmh_cells: Option<Entity>,
    tmh_bar: Option<Entity>,
    tmh_scroll: usize,
    tmh_shown: Option<(usize, CcmTab, String, usize, bool)>,
    /// The picker asked for by a test.
    test_picker: bool,
}

impl Cap {
    /// A breed's picture (the game's, as baked).
    pub fn thumb(&mut self, images: &mut Assets<Image>, breed: &s3bake::pets::PetBreed) -> Option<Handle<Image>> {
        if let Some(h) = self.thumbs.get(&breed.outfit) {
            return Some(h.clone());
        }
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let (w, h, px) = s3bake::gamedata::decode_icon(&breed.thumb)?;
        let img = images.add(Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD));
        self.thumbs.insert(breed.outfit, img.clone());
        Some(img)
    }
}

/// Where the camera stands for a pet of a kind, and what it looks at (the whole pet, beside the
/// panels as a Sim is).
pub fn framing(kind: &str) -> (Vec3, Vec3) {
    match kind {
        "ah" | "ch" => (Vec3::new(-0.75, 1.6, 6.2), Vec3::new(-0.75, 0.95, 0.0)),
        "ad" | "cd" => (Vec3::new(-0.32, 0.8, 2.7), Vec3::new(-0.32, 0.4, 0.0)),
        "al" | "cl" => (Vec3::new(-0.21, 0.55, 1.85), Vec3::new(-0.21, 0.26, 0.0)),
        _ => (Vec3::new(-0.17, 0.45, 1.45), Vec3::new(-0.17, 0.2, 0.0)),
    }
}

/// The breeds of a kind as Create a Pet lists them.
fn listed<'a>(data: &'a s3bake::PetsBaked, kind: &str) -> Vec<&'a s3bake::pets::PetBreed> {
    let mut l: Vec<&s3bake::pets::PetBreed> = data.breeds.iter().filter(|b| b.kind == kind && b.listed.is_some()).collect();
    l.sort_by_key(|b| b.listed);
    l
}

/// A new pet of a kind: the mixed breed, a name, either sex, three traits.
fn new_pet(data: &s3bake::PetsBaked, kind: &'static str, rng: &mut impl Rng) -> Option<SavedPet> {
    let breed = listed(data, kind).first().copied().or_else(|| data.breeds.iter().find(|b| b.kind == kind))?;
    Some(SavedPet {
        id: rng.random(),
        name: crate::pets::random_pet_name(rng),
        kind: kind.to_string(),
        outfit: breed.outfit,
        position: None,
        motives: None,
        bonds: Vec::new(),
        traits: crate::pettraits::random(kind, rng),
        female: rng.random(),
        saddled: false,
        tricks: None,
        accessories: Vec::new(),
        shapes: Vec::new(),
        coat: None,
        tint: None,
        tail: None,
        mane: None,
        hooves: None,
        sliders: Vec::new(),
    })
}

/// The puck's asks, and the skewer's clicks: a pet made, taken out or chosen; a Sim chosen (or
/// one made) leaves the pet.
#[allow(clippy::too_many_arguments)]
fn requests(
    mut cap: ResMut<Cap>,
    mut asks: MessageReader<CapRequest>,
    mut cas_asks: MessageReader<crate::cas::CasActionRequest>,
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    pet_clicks: Query<(&Interaction, &PetButton), Changed<Interaction>>,
    sim_clicks: Query<(&Interaction, &CasAction), Changed<Interaction>>,
) {
    for a in asks.read() {
        match *a {
            CapRequest::New(kind) => {
                let Some(d) = data.0.as_deref() else { continue };
                if pending.pets.len() >= MAX_PETS || pending.members.len() + pending.pets.len() >= MAX_IN_HOUSEHOLD {
                    continue;
                }
                if let Some(p) = new_pet(d, kind, &mut rand::rng()) {
                    pending.pets.push(p);
                    cap.pet = Some(pending.pets.len() - 1);
                    cap.panel = None;
                }
            }
            CapRequest::Remove => {
                if let Some(i) = cap.pet.take().filter(|i| *i < pending.pets.len()) {
                    pending.pets.remove(i);
                }
                cap.panel = None;
            }
        }
    }
    for (i, b) in &pet_clicks {
        if *i == Interaction::Pressed {
            cap.pet = Some(b.0);
        }
    }
    let to_sims = sim_clicks.iter().any(|(i, a)| *i == Interaction::Pressed && matches!(a, CasAction::Select(_) | CasAction::Add)) || cas_asks.read().any(|a| matches!(a.0, CasAction::Add | CasAction::NewFamily | CasAction::Family(_)));
    if to_sims {
        cap.pet = None;
    }
    if cap.pet.is_some_and(|i| i >= pending.pets.len()) {
        cap.pet = None;
    }
    if cap.pet.is_none() {
        cap.panel = None;
        cap.typing = None;
    }
    let up = cap.pet.is_some();
    if UP.load(Ordering::Relaxed) != up {
        UP.store(up, Ordering::Relaxed);
    }
}

/// The pet being made in the room, in the Sim's place (turned with them), idling.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pet_stage(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    pending: Res<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    (scene, baked): (Option<Res<crate::cas::CasScene>>, Option<Res<crate::baked::Baked>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials, mut bindposes): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<SkinnedMeshInverseBindposes>>),
    mut sims: Query<&mut Visibility, With<crate::cas::CasModel>>,
    mut models: Query<&mut Transform, (With<crate::pets::Pet>, Without<crate::cas::CasModel>)>,
) {
    let pet = cap.pet.and_then(|i| pending.pets.get(i));
    for mut v in &mut sims {
        let want = if pet.is_some() { Visibility::Hidden } else { Visibility::Inherited };
        if *v != want {
            *v = want;
        }
    }
    // (Made again for another breed, or another shape.)
    let want = pet.map(|p| {
        let tint = p.tint.map_or(0, |c| c.iter().fold(1u64, |a, v| a << 8 | (v * 255.0) as u64));
        let parts = [&p.tail, &p.mane, &p.hooves].map(|n| n.as_deref().map_or(0, |n| s3pkg::fnv64(n) | 1));
        p.shapes.iter().chain(p.coat.iter()).chain(std::iter::once(&tint)).chain(parts.iter()).fold(p.outfit, |h, s| h.rotate_left(7) ^ s)
    });
    if cap.model.map(|m| m.1) != want {
        if let Some((e, _)) = cap.model.take() {
            commands.entity(e).try_despawn();
        }
        let (Some(p), Some(d), Some(b)) = (pet, data.0.as_deref(), baked) else { return };
        let Some(breed) = d.breeds.iter().position(|x| x.outfit == p.outfit) else { return };
        let mut ctx = crate::objects::AssetCtx { baked: &b.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        if let Some(e) = crate::pets::spawn_pet(&mut commands, d, breed, (p.name.clone(), p.female), Vec3::ZERO, 0.0, &p.look(), &mut ctx, &mut assets, &mut bindposes) {
            let mut anim = crate::pets::PetAnim::default();
            anim.play(format!("{}_idle_stand_breathe_x", d.breeds[breed].kind));
            commands.entity(e).insert((anim, DespawnOnExit(AppState::CreateHousehold)));
            cap.model = want.map(|w| (e, w));
            cap.worn.clear();
        }
    }
    // (Dressed in what it's chosen to wear.)
    if let (Some((e, _)), Some(p)) = (cap.model, pet)
        && p.accessories != cap.worn
    {
        cap.worn = p.accessories.clone();
        commands.entity(e).insert(crate::pets::WantsAccessories(p.accessories.clone()));
    }
    // (Turned as the Sim is, a little more side on: a pet's long.)
    if let (Some((e, _)), Some(s)) = (cap.model, scene)
        && let Ok(mut tf) = models.get_mut(e)
    {
        let r = Quat::from_rotation_y(s.yaw + 0.9);
        if tf.rotation != r {
            tf.rotation = r;
        }
    }
}

// `CAPPetSheet`'s windows.
const SHEET_BASICS: u32 = 0x0077_7000;
const SHEET_BASICS_FEMALE: u32 = 0x0077_7007;
const SHEET_BREED: u32 = 0x0077_7001;
const SHEET_FACE: u32 = 0x0077_7002;
const SHEET_CHARACTER: u32 = 0x0077_7003;
const SHEET_COAT: u32 = 0x0077_7004;
const SHEET_ACCESSORIES: u32 = 0x0077_7005;
const SHEET_TEXT: u32 = 0x0077_7020;
const SHEET_DOGS: u32 = 0x0077_7022;
const SHEET_HORSES: u32 = 0x0077_7023;
const SHEET_CATS: u32 = 0x0077_7024;
const SHEET_RANDOMIZE: u32 = 0x0077_70b2;
const SHEET_NOT_HERE: [u32; 3] = [0x0077_70b3, 0x0077_70b4, 0x0077_70b5];

/// The species' panel of the sheet (each with its own Breed, Coat, Face and Accessories).
fn species_panel(kind: &str) -> u32 {
    match kind.get(1..) {
        Some("h") => SHEET_HORSES,
        Some("c") => SHEET_CATS,
        _ => SHEET_DOGS,
    }
}

/// A layout spawned for Create a Pet (hidden till it's wanted).
fn spawn_layout(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, name: &str, z: i32) -> Option<Spawned> {
    let s = ui.spawn(commands, images, fonts, name)?;
    commands.entity(s.root?).insert((DespawnOnExit(AppState::CreateHousehold), GlobalZIndex(z), Visibility::Hidden));
    Some(s)
}

/// The pet sheet beside the pet: its species' buttons, the panel open lit (its names put away
/// while one's open), Coat, Face and Accessories not here; Randomize.
#[allow(clippy::too_many_arguments)]
fn pet_sheet(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    frame: Option<Res<crate::caslook::CasFrame>>,
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    (mut vis, mut buttons): (Query<&mut Visibility>, Query<&mut UiButton>),
) {
    let (Some(mut ui), Some(_)) = (ui, frame) else { return };
    if cap.sheet.is_none() && ui.layout("CAPPetSheet").is_some() {
        cap.sheet = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPPetSheet", 5);
        if let Some(s) = &cap.sheet {
            for id in SHEET_NOT_HERE {
                for e in s.all_with(id) {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
            for (id, tip) in [(SHEET_BASICS, "Basics"), (SHEET_BASICS_FEMALE, "Basics"), (SHEET_CHARACTER, "Character"), (SHEET_RANDOMIZE, "Randomize")] {
                for e in s.all_with(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
            for (id, tip) in [(SHEET_BREED, "Breed"), (SHEET_COAT, "Coat"), (SHEET_FACE, "Face"), (SHEET_ACCESSORIES, "Accessories")] {
                for e in s.all_with(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
        }
    }
    let Some(s) = cap.sheet.clone() else { return };
    let modal = crate::casfamily::UP.load(Ordering::Relaxed) || crate::casgenetics::UP.load(Ordering::Relaxed);
    let pet = cap.pet.and_then(|i| pending.pets.get(i)).cloned();
    crate::livehud::set_visible(&mut vis, s.root, pet.is_some() && !modal);
    let Some(pet) = pet else {
        cap.sheet_shown = None;
        return;
    };
    let panel = species_panel(&pet.kind);
    let within = |id: u32| s.id(panel).and_then(|p| s.within(p, id));
    let pressed = |e: Option<Entity>| crate::livehud::pressed(&clicks, e);
    let mut open = cap.panel;
    if s.all_with(SHEET_BASICS).into_iter().chain(s.all_with(SHEET_BASICS_FEMALE)).any(|e| pressed(Some(e))) {
        open = if open == Some(Panel::Basics) { None } else { Some(Panel::Basics) };
    }
    if pressed(within(SHEET_BREED)) {
        open = if open == Some(Panel::Breed) { None } else { Some(Panel::Breed) };
    }
    if pressed(s.id(SHEET_CHARACTER)) {
        open = if open == Some(Panel::Character) { None } else { Some(Panel::Character) };
    }
    let dressable = data.0.as_deref().is_some_and(|d| d.kinds.iter().any(|k| k.code == pet.kind && !k.accessories.is_empty()));
    if pressed(within(SHEET_COAT)) {
        open = if open == Some(Panel::Coat) { None } else { Some(Panel::Coat) };
    }
    if pressed(within(SHEET_FACE)) {
        // (From the sliders, back to the shapes.)
        open = match open {
            Some(Panel::Shape) if cap.adv_open => {
                cap.adv_open = false;
                Some(Panel::Shape)
            }
            Some(Panel::Shape) => None,
            _ => Some(Panel::Shape),
        };
    }
    if dressable && pressed(within(SHEET_ACCESSORIES)) {
        open = if open == Some(Panel::Accessories) { None } else { Some(Panel::Accessories) };
    }
    if open != cap.panel {
        cap.panel = open;
        cap.typing = None;
        cap.adv_open = false;
    }
    // Randomize: another pet of its species (breed, name, sex and traits).
    if pressed(s.id(SHEET_RANDOMIZE))
        && let (Some(i), Some(d)) = (cap.pet, data.0.as_deref())
    {
        let mut rng = rand::rng();
        let kind: &'static str = crate::pettraits::KINDS.iter().copied().find(|k| *k == pet.kind).unwrap_or("ac");
        let p = &mut pending.pets[i];
        if let Some(b) = listed(d, kind).choose(&mut rng) {
            p.outfit = b.outfit;
        }
        p.name = crate::pets::random_pet_name(&mut rng);
        p.female = rng.random();
        p.traits = crate::pettraits::random_up_to(kind, 3, &mut rng).into_iter().map(str::to_string).collect();
    }
    let want = (pet.kind.clone(), cap.panel);
    let _ = dressable;
    if cap.sheet_shown.as_ref() == Some(&want) {
        return;
    }
    cap.sheet_shown = Some(want);
    for p in [SHEET_DOGS, SHEET_HORSES, SHEET_CATS] {
        crate::livehud::set_visible(&mut vis, s.id(p), p == panel);
    }
    crate::livehud::set_visible(&mut vis, s.id(SHEET_TEXT), cap.panel.is_none());
    // (The female's Basics button for a female pet, as the game's sheet has it.)
    crate::livehud::set_visible(&mut vis, s.id(SHEET_BASICS_FEMALE), pet.female);
    crate::livehud::set_visible(&mut vis, s.id(SHEET_BASICS), !pet.female);
    let mut set = |e: Option<Entity>, selected: bool, disabled: bool| {
        if let Some(mut b) = e.and_then(|e| buttons.get_mut(e).ok())
            && (b.selected, b.disabled) != (selected, disabled)
        {
            b.selected = selected;
            b.disabled = disabled;
        }
    };
    set(s.id(SHEET_BASICS), cap.panel == Some(Panel::Basics), false);
    set(s.id(SHEET_BASICS_FEMALE), cap.panel == Some(Panel::Basics), false);
    set(within(SHEET_BREED), cap.panel == Some(Panel::Breed), false);
    set(s.id(SHEET_CHARACTER), cap.panel == Some(Panel::Character), false);
    set(within(SHEET_FACE), cap.panel == Some(Panel::Shape), false);
    set(within(SHEET_COAT), cap.panel == Some(Panel::Coat), false);
    let dressable = data.0.as_deref().is_some_and(|d| d.kinds.iter().any(|k| k.code == pet.kind && !k.accessories.is_empty()));
    set(within(SHEET_ACCESSORIES), cap.panel == Some(Panel::Accessories), !dressable);
}

// `CAPBasics`'s windows.
const BASICS_CLOSE: u32 = 0x05db_9700;
const FIRST_NAME: u32 = 0x05db_9701;
const LAST_NAME: u32 = 0x05db_9702;
const MALE: u32 = 0x05db_9703;
const FEMALE: u32 = 0x05db_9704;
const ADULT: u32 = 0x05db_9709;
const ELDER: u32 = 0x05db_970a;
const DOG_AGES: u32 = 0x05db_970b;
const CAT_AGES: u32 = 0x05db_970c;
const HORSE_AGES: u32 = 0x05db_970d;
const RANDOM_NAME: u32 = 0x05db_970e;
const INVALID_NAME: u32 = 0x10;

/// Basics: the pet's name (typed into its box: a click, the keys, Enter), the household's name,
/// male or female; grown (an elder pet isn't made here).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn basics_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pending: ResMut<PendingHousehold>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    (mut vis, mut buttons, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut Text>),
    (mut keyboard, mouse): (MessageReader<bevy::input::keyboard::KeyboardInput>, Res<ButtonInput<MouseButton>>),
) {
    let Some(mut ui) = ui else { return };
    let on = cap.panel == Some(Panel::Basics);
    if on && cap.basics.is_none() && ui.layout("CAPBasics").is_some() {
        cap.basics = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPBasics", 6);
        if let Some(s) = &cap.basics {
            commands.entity(s.root.unwrap()).insert(Visibility::Hidden);
            for e in s.all_with(INVALID_NAME) {
                commands.entity(e).insert(Visibility::Hidden);
            }
            for (id, tip) in [(FEMALE, "Female"), (MALE, "Male"), (RANDOM_NAME, "Random Name"), (BASICS_CLOSE, "Close")] {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
            for (id, tip) in [(ADULT, "Adult"), (ELDER, "Elder")] {
                for e in s.all_with(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
            for id in [FIRST_NAME, LAST_NAME] {
                if let Some(e) = s.id(id) {
                    commands.entity(e).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
                }
            }
        }
        // (Its windows are there to set from the next frame.)
        return;
    }
    let Some(s) = cap.basics.clone() else { return };
    crate::livehud::set_visible(&mut vis, s.root, on);
    let Some(i) = cap.pet.filter(|_| on) else {
        cap.basics_shown = None;
        keyboard.clear();
        return;
    };
    let pressed = |id: u32| crate::livehud::pressed(&clicks, s.id(id));
    if pressed(BASICS_CLOSE) {
        cap.panel = None;
        return;
    }
    let mut rng = rand::rng();
    if pressed(FEMALE) {
        pending.pets[i].female = true;
    }
    if pressed(MALE) {
        pending.pets[i].female = false;
    }
    if pressed(RANDOM_NAME) {
        pending.pets[i].name = crate::pets::random_pet_name(&mut rng);
    }
    // Typing a name: a click on its box, then the keys; Enter (or a click elsewhere) keeps it.
    for id in [FIRST_NAME, LAST_NAME] {
        if pressed(id) {
            let now = if id == FIRST_NAME { pending.pets[i].name.clone() } else { pending.last_name.clone() };
            cap.typing = Some((id, now));
        }
    }
    let mut finished = false;
    if let Some((id, words)) = cap.typing.as_mut() {
        use bevy::input::keyboard::Key;
        for ev in keyboard.read() {
            if !ev.state.is_pressed() {
                continue;
            }
            match &ev.logical_key {
                Key::Character(c) if words.chars().count() < 20 => words.push_str(c.as_str()),
                Key::Space if words.chars().count() < 20 => words.push(' '),
                Key::Backspace => {
                    words.pop();
                }
                Key::Enter | Key::Escape | Key::Tab => finished = true,
                _ => {}
            }
        }
        let typed = words.trim().to_string();
        if !typed.is_empty() {
            if *id == FIRST_NAME {
                pending.pets[i].name = typed;
            } else {
                for m in pending.members.iter_mut() {
                    m.last = typed.clone();
                }
                pending.last_name = typed;
            }
        }
        let box_pressed = s.id(*id).is_some_and(|e| clicks.get(e).is_ok_and(|(_, x)| *x == Interaction::Pressed));
        if mouse.just_pressed(MouseButton::Left) && !box_pressed {
            finished = true;
        }
    } else {
        keyboard.clear();
    }
    if finished {
        cap.typing = None;
    }
    let pet = &pending.pets[i];
    let caret = |id: u32, v: &str| match &cap.typing {
        Some((t, w)) if *t == id => format!("{w}|"),
        _ => v.to_string(),
    };
    let (first, last) = (caret(FIRST_NAME, &pet.name), caret(LAST_NAME, &pending.last_name));
    let want = (i, first.clone(), last.clone(), pet.female, pet.kind.clone());
    if cap.basics_shown.as_ref() == Some(&want) {
        return;
    }
    cap.basics_shown = Some(want);
    let ages = match pet.kind.get(1..) {
        Some("h") => HORSE_AGES,
        Some("c") => CAT_AGES,
        _ => DOG_AGES,
    };
    for a in [DOG_AGES, CAT_AGES, HORSE_AGES] {
        crate::livehud::set_visible(&mut vis, s.id(a), a == ages);
    }
    let mut set = |e: Option<Entity>, selected: bool, disabled: bool| {
        if let Some(mut b) = e.and_then(|e| buttons.get_mut(e).ok())
            && (b.selected, b.disabled) != (selected, disabled)
        {
            b.selected = selected;
            b.disabled = disabled;
        }
    };
    set(s.id(FEMALE), pet.female, false);
    set(s.id(MALE), !pet.female, false);
    let panel = s.id(ages);
    set(panel.and_then(|p| s.within(p, ADULT)), true, false);
    set(panel.and_then(|p| s.within(p, ELDER)), false, true);
    crate::livehud::set_text(&mut texts, s.text(FIRST_NAME), &first);
    crate::livehud::set_text(&mut texts, s.text(LAST_NAME), &last);
}

// `CAPBreeds`'s windows.
const BREEDS_CLOSE: u32 = 0xb5b9_5101;
const BREEDS_GRID: u32 = 0xb5b9_5102;
const DOG_TOGGLE: u32 = 0xb5b9_5110;
const LITTLE_DOGS: u32 = 0xb5b9_5111;
const BIG_DOGS: u32 = 0xb5b9_5112;
/// A cell's picture window (`GenericCasItemGrow_landscape`'s).
const CELL_THUMBNAIL: u32 = 0x20;
/// The item grid's own scroll bar (layout `ItemGrid`).
const GRID_SCROLLBAR: u32 = 0x0582_0672;

/// A breed's cell in the grid (its outfit).
#[derive(Component)]
struct BreedCell(u64);

/// Breed: the game's breeds of the pet's species in its grid (two across), each its picture and
/// name, the pet's own lit; a click makes it that breed. Dogs have the big and little dogs' lists
/// (a little dog's breed makes it a little dog). Scrolled by the wheel or the grid's bar.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn breeds_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    (clicks, cells, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &BreedCell), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    (mut vis, mut buttons, mut bars): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
) {
    let Some(mut ui) = ui else { return };
    let on = cap.panel == Some(Panel::Breed);
    if on && cap.breeds.is_none() && ui.layout("CAPBreeds").is_some() {
        cap.breeds = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPBreeds", 6);
        if let Some(s) = &cap.breeds {
            for (id, tip) in [(LITTLE_DOGS, "Little Dog"), (BIG_DOGS, "Big Dog"), (BREEDS_CLOSE, "Close")] {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
            if let (Some(grid), Some(g), Some(mut b)) = (s.id(BREEDS_GRID), ui.find("CAPBreeds", BREEDS_GRID).cloned(), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
                let (w, h) = (g.area[2] - g.area[0], g.area[3] - g.area[1]);
                b.flags |= s3bake::ui::WIN_VISIBLE;
                b.place = s3bake::ui::UiPlace::Fixed;
                b.area = [w - 20.0, 0.0, w - 2.0, h - 4.0];
                cap.bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, grid).root;
                commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default()).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::clip());
            }
        }
        return;
    }
    let Some(s) = cap.breeds.clone() else { return };
    crate::livehud::set_visible(&mut vis, s.root, on);
    let (Some(i), Some(d)) = (cap.pet.filter(|_| on), data.0.clone()) else {
        cap.breeds_shown = None;
        return;
    };
    let pressed = |id: u32| crate::livehud::pressed(&clicks, s.id(id));
    if pressed(BREEDS_CLOSE) {
        cap.panel = None;
        return;
    }
    let pet = pending.pets[i].clone();
    let dog = matches!(pet.kind.as_str(), "ad" | "al");
    // (The dogs' lists: the one its kind is in, to begin with.)
    if cap.breeds_shown.as_ref().is_none_or(|b| b.0 != i) {
        cap.little = pet.kind == "al";
        cap.scroll = 0;
    }
    if dog && pressed(LITTLE_DOGS) && !cap.little {
        cap.little = true;
        cap.scroll = 0;
    }
    if dog && pressed(BIG_DOGS) && cap.little {
        cap.little = false;
        cap.scroll = 0;
    }
    let kind = if dog { if cap.little { "al" } else { "ad" } } else { pet.kind.as_str() };
    for (x, c) in &cells {
        if *x == Interaction::Pressed {
            pending.pets[i].outfit = c.0;
            pending.pets[i].kind = kind.to_string();
        }
    }
    let list = listed(&d, kind);
    let Some(g) = ui.find("CAPBreeds", BREEDS_GRID).and_then(|w| w.grid) else { return };
    let cols = g.columns.max(1) as usize;
    let rows = (list.len() + cols - 1) / cols;
    let most = rows.saturating_sub(g.rows.max(1) as usize);
    let over_grid = s.id(BREEDS_GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_grid && wheel.delta.y != 0.0 {
        cap.scroll = (cap.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        if let Some(mut b) = cap.bar.and_then(|e| bars.get_mut(e).ok()) {
            b.value = cap.scroll;
        }
    } else if let Some(b) = cap.bar.and_then(|e| bars.get(e).ok())
        && b.value != cap.scroll
    {
        cap.scroll = b.value.min(most);
    }
    if let Some(mut b) = cap.bar.and_then(|e| bars.get_mut(e).ok())
        && (b.total, b.visible) != (rows, g.rows.max(1) as usize)
    {
        (b.total, b.visible) = (rows, g.rows.max(1) as usize);
    }
    let outfit = pending.pets[i].outfit;
    let want = (i, outfit, cap.little, cap.scroll);
    if cap.breeds_shown.as_ref() == Some(&want) {
        return;
    }
    cap.breeds_shown = Some(want);
    for id in [DOG_TOGGLE, LITTLE_DOGS, BIG_DOGS] {
        crate::livehud::set_visible(&mut vis, s.id(id), dog);
    }
    for (id, lit) in [(LITTLE_DOGS, cap.little), (BIG_DOGS, !cap.little)] {
        if let Some(mut b) = s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != lit
        {
            b.selected = lit;
        }
    }
    if let Some(old) = cap.cells.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(grid), Some(template)) = (s.id(BREEDS_GRID), ui.layout("GenericCasItemGrow_landscape").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(grid).insert_children(0, &[holder]);
    cap.cells = Some(holder);
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    for (n, b) in list.iter().enumerate().skip(cap.scroll * cols).take(cols * g.rows.max(1) as usize) {
        let k = n - cap.scroll * cols;
        let (x, y) = (g.padding[0] + (k % cols) as f32 * step.x, g.padding[1] + (k / cols) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        commands.entity(root).insert((BreedCell(b.outfit), crate::icons::Tooltip(b.name.clone())));
        if b.outfit == outfit {
            commands.entity(root).insert(crate::layout::Selected);
        }
        if let (Some(win), Some(h)) = (c.id(CELL_THUMBNAIL), cap.thumb(&mut images, b)) {
            crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
        }
    }
}

// `CAPFBDBasic`'s windows: Done, the species' tabs and each tab's button (`ControlIDs`), the
// long and short backgrounds; and `CAPFaceBody`'s title, presets' grid and Advanced buttons.
const FBD_DONE: u32 = 0x40f2_2201;
const FBD_TABS: [(&str, u32); 3] = [("d", 0x40f2_2202), ("c", 0x40f2_2203), ("h", 0x40f2_2204)];
const FBD_LONG: u32 = 0x40f2_2220;
const FBD_SHORT: u32 = 0x40f2_2221;
const SHAPE_TITLE: u32 = 0x2184_91f1;
const SHAPE_GRID: u32 = 0x2184_91f2;
const SHAPE_ADVANCED: [u32; 3] = [0x2184_91f3, 0x2184_91f4, 0x2184_91f5];
/// The tabs: their buttons, the region each shows (`SetFaceBodyCategory`), and its title (the
/// game's key, and the words where it has none).
const SHAPE_TABS: [(u32, u32, &str, &str); 6] = [
    (0x40f2_2210, s3bake::pets::PET_FULL_BODY, "Ui/Caption/CAS/CAP/FaceBody:Title", "Full Body"),
    (0x40f2_2215, s3bake::pets::PET_BODY, "Ui/Caption/CAS/CAP/Body:Title", "Body"),
    (0x40f2_2211, s3bake::pets::PET_HEAD, "Ui/Caption/CAS/CAP/Face:Title", "Head"),
    (0x40f2_2212, s3bake::pets::PET_EYES, "Ui/Caption/CAS/CAP/Eyes:Title", "Eyes"),
    (0x40f2_2213, s3bake::pets::PET_SNOUT, "Ui/Caption/CAS/CAP/Snout:Title", "Snout"),
    (0x40f2_2214, s3bake::pets::PET_EARS, "Ui/Caption/CAS/CAP/Ears:Title", "Ears"),
];

// `CAPFBDAdvanced`'s windows (`ControlIDs`): the species' minimaps, each with its head and body
// pictures, the toggles between them and its sides; the sliders' label and grid; what isn't here.
const ADV_MINIMAPS: [(&str, u32); 3] = [("d", 0x03cf_2621), ("c", 0x03cf_2622), ("h", 0x03cf_2623)];
const ADV_HEAD: u32 = 0x03cf_262a;
const ADV_BODY: u32 = 0x03cf_262b;
const ADV_TO_HEAD: u32 = 0x03cf_2613;
const ADV_TO_BODY: u32 = 0x03cf_2614;
const ADV_SIDES: [u32; 2] = [0x03cf_2611, 0x03cf_2612];
const ADV_LABEL: u32 = 0x03cf_2616;
const ADV_GRID: u32 = 0x03cf_2661;
const ADV_NOT_HERE: [u32; 5] = [0x03cf_2615, 0x03cf_2653, 0x03cf_2680, 0x03cf_2690, 0x03cf_2601];
const ADV_LONG: u32 = 0x03cf_2655;
/// A slider row's (`CASBodySlider`) name and slider.
const ROW_TITLE: u32 = 2;
const ROW_SLIDER: u32 = 4;
/// The minimap's regions (`FacialBlendRegions`): the body's head, full body and legs.
const REGION_HEAD: u32 = 2;
const REGION_BODY: u32 = 0x200;
const REGION_EARS: u32 = 0x20;
/// A dog's sliders for ears that stand up (`EarsAlt`).
const REGION_EARS_UP: u32 = 0x1000;

/// One of the minimap's regions, its picture (its hit mask: where on the button it is).
#[derive(Component, Clone, Copy)]
struct RegionButton {
    region: u32,
    mask: u64,
}

/// Where Shape's tab opens the sliders (`SetDefaultPart`): its region, and whether that's the
/// body's picture (or the head's).
fn advanced_region(tab: u32, horse: bool) -> (u32, bool) {
    match tab {
        s3bake::pets::PET_EYES => (0x80, false),
        s3bake::pets::PET_SNOUT => (if horse { 8 } else { 0x10 }, false),
        s3bake::pets::PET_EARS => (REGION_EARS, false),
        s3bake::pets::PET_HEAD => (REGION_HEAD, true),
        _ => (REGION_BODY, true),
    }
}

/// The first picture a window draws (a minimap button's: its hit mask).
fn first_image(d: &s3bake::ui::UiDrawable) -> Option<u64> {
    match d {
        s3bake::ui::UiDrawable::Std { images, .. } => images.iter().copied().find(|i| *i != 0),
        s3bake::ui::UiDrawable::Image { image, .. } => (*image != 0).then_some(*image),
        s3bake::ui::UiDrawable::Multi(list) => list.iter().find_map(first_image),
    }
}

/// A pet's blends' amounts as its sliders show them: its breed's and shapes', those set by the
/// sliders over them.
fn slider_amounts(d: &s3bake::PetsBaked, pet: &SavedPet) -> Vec<(u64, f32)> {
    let Some(breed) = d.breeds.iter().find(|b| b.outfit == pet.outfit) else { return Vec::new() };
    let mut a = d.amounts(breed, &pet.shapes);
    for (b, v) in &pet.sliders {
        match a.iter_mut().find(|x| x.0 == *b) {
            Some(x) => x.1 = *v,
            None => a.push((*b, *v)),
        }
    }
    a
}

/// A slider's value (-1 to 1 both ways, else 0 to 1), as the game's `FacialBlendData.Value`:
/// its second blend's, the other way, where that's set.
pub fn slider_value(s: &s3bake::pets::PetSlider, amounts: &[(u64, f32)]) -> f32 {
    let of = |b: u64| amounts.iter().find(|x| x.0 == b && b != 0).map_or(0.0, |x| x.1);
    let (a, b) = (of(s.blends[0]), of(s.blends[1]));
    if s.blends[1] != 0 && b > 0.0 { -b } else { a }
}

/// A slider moved to a value: its blends' amounts (the one way's, and the other's nothing).
pub fn slider_set(s: &s3bake::pets::PetSlider, v: f32) -> Vec<(u64, f32)> {
    match (s.blends[1], v >= 0.0) {
        (0, _) => vec![(s.blends[0], v.max(0.0))],
        (b, true) => vec![(s.blends[0], v), (b, 0.0)],
        (b, false) => vec![(s.blends[0], 0.0), (b, -v)],
    }
}

/// The minimap's regions under the pointer, by their pictures' hit masks: the one hit lit as the
/// pointer's over it (and pressed), the others not (their buttons lie one over another).
#[allow(clippy::type_complexity)]
fn minimap_hits(
    mut cap: ResMut<Cap>,
    ui: Option<Res<UiAssets>>,
    mut buttons: Query<(&RegionButton, &bevy::ui::RelativeCursorPosition, &InheritedVisibility, &mut Interaction)>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    let Some(ui) = ui else { return };
    if !cap.adv_open {
        return;
    }
    let mut hit: Option<u32> = None;
    for (b, at, vis, _) in &buttons {
        if hit.is_some() || !vis.get() || !at.cursor_over() {
            continue;
        }
        let Some(p) = at.normalized else { continue };
        let mask = cap.adv_masks.entry(b.mask).or_insert_with(|| ui.image_pixels(b.mask)).as_ref();
        if let Some((w, h, px)) = mask {
            let (x, y) = (((p.x + 0.5) * *w as f32) as u32, ((p.y + 0.5) * *h as f32) as u32);
            if x < *w && y < *h && px[((y * w + x) * 4 + 3) as usize] > 127 {
                hit = Some(b.region);
            }
        }
    }
    for (b, _, _, mut i) in &mut buttons {
        let want = match hit {
            Some(r) if r == b.region && mouse.pressed(MouseButton::Left) => Interaction::Pressed,
            Some(r) if r == b.region => Interaction::Hovered,
            _ => Interaction::None,
        };
        i.set_if_neq(want);
    }
    if let Some(r) = hit.filter(|_| mouse.just_pressed(MouseButton::Left)) {
        cap.adv_pick = Some(r);
    }
}

/// Shape's Advanced (`CAPFBDAdvanced`): the species' minimap, its head or its body (the toggle
/// turning between them), a region clicked on it (where its picture is: its hit mask); and the
/// region's sliders in the game's rows (`CASBodySlider`), the species' own and those cats and
/// dogs share, both ways or one, their values the pet's blends' amounts. Moved, the pet's shaped
/// so as it moves; kept with the pet. The wheel or the bar turns the rows.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn advanced_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    (mut vis, mut buttons, mut bars, sliders): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>, Query<&UiSlider>),
    (regions, over, mut skels): (Query<(Entity, &RegionButton)>, Query<&bevy::ui::RelativeCursorPosition>, Query<&mut crate::pets::PetSkeleton>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
) {
    let Some(mut ui) = ui else { return };
    let on = cap.panel == Some(Panel::Shape) && cap.adv_open;
    if on && cap.adv.is_none() && ui.layout("CAPFBDAdvanced").is_some() {
        let Some(s) = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPFBDAdvanced", 7) else { return };
        if let Some(r) = s.root {
            commands.entity(r).entry::<Node>().and_modify(|mut n| n.left = Val::Px(ADV_LEFT));
        }
        for (_, id) in ADV_MINIMAPS {
            if let Some(e) = s.id(id) {
                commands.entity(e).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::clip());
            }
        }
        for id in ADV_NOT_HERE.into_iter().chain(ADV_SIDES).chain([ADV_LONG]) {
            for e in s.all_with(id) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
        // (The regions' buttons, on the head's and the body's pictures, as the game finds where
        // the pointer is on them: by their pictures.)
        let layout = ui.layout("CAPFBDAdvanced").cloned();
        for (_, map) in ADV_MINIMAPS {
            for part in [ADV_HEAD, ADV_BODY] {
                let (Some(win), Some(pe)) = (layout.as_ref().and_then(|l| l.find(map)).and_then(|m| m.find(part)), s.id(map).and_then(|m| s.within(m, part))) else { continue };
                for c in win.children.iter().filter(|c| c.cls.contains("Button") && c.id != 0) {
                    let (Some(e), Some(mask)) = (s.within(pe, c.id), c.drawable.as_ref().and_then(first_image)) else { continue };
                    let tip = ui.localize(&c.tooltip).filter(|t| !t.is_empty()).unwrap_or_else(|| c.comment.clone());
                    commands.entity(e).insert((RegionButton { region: c.id, mask }, bevy::ui::RelativeCursorPosition::default(), bevy::ui::FocusPolicy::Pass, crate::icons::Tooltip(tip)));
                }
            }
        }
        if let (Some(grid), Some(g), Some(mut b)) = (s.id(ADV_GRID), ui.find("CAPFBDAdvanced", ADV_GRID).cloned(), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
            let (w, h) = (g.area[2] - g.area[0], g.area[3] - g.area[1]);
            b.flags |= s3bake::ui::WIN_VISIBLE;
            b.place = s3bake::ui::UiPlace::Fixed;
            b.area = [w - 20.0, 0.0, w - 2.0, h - 4.0];
            cap.adv_bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, grid).root;
            commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default()).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::clip());
        }
        cap.adv = Some(s);
        return;
    }
    let Some(s) = cap.adv.clone() else { return };
    crate::livehud::set_visible(&mut vis, s.root, on);
    let (Some(i), Some(d)) = (cap.pet.filter(|_| on), data.0.clone()) else {
        cap.adv_shown = None;
        return;
    };
    let pet = pending.pets[i].clone();
    let species = pet.kind.get(1..).map_or("d", |s| if s == "l" { "d" } else { s });
    let map = ADV_MINIMAPS.iter().find(|m| m.0 == species).and_then(|m| s.id(m.1));
    // The head or the body.
    if crate::livehud::pressed(&clicks, map.and_then(|m| s.within(m, ADV_TO_HEAD))) {
        (cap.adv_body, cap.adv_region) = (false, REGION_EARS);
        cap.adv_scroll = 0;
    }
    if crate::livehud::pressed(&clicks, map.and_then(|m| s.within(m, ADV_TO_BODY))) {
        (cap.adv_body, cap.adv_region) = (true, REGION_BODY);
        cap.adv_scroll = 0;
    }
    if let Some(r) = cap.adv_pick.take()
        && r != cap.adv_region
    {
        cap.adv_region = r;
        cap.adv_scroll = 0;
    }
    // (A dog whose ears stand has their sliders: `GetCurrentEarSliderRegion`.)
    let breed = d.breeds.iter().find(|b| b.outfit == pet.outfit);
    let region = if cap.adv_region == REGION_EARS && species == "d" && breed.and_then(|b| b.ears.as_deref()).is_some_and(|e| e.contains("Up")) { REGION_EARS_UP } else { cap.adv_region };
    let list: Vec<usize> = d.sliders_for(&pet.kind, region).iter().filter_map(|x| d.sliders.iter().position(|y| std::ptr::eq(*x, y))).collect();
    // A slider moved: the pet's blends so, and the pet shaped so there and then.
    let mut moved = false;
    // (CAP_SLIDE=<slider>:<value, -1 to 1> (tests): that slider of the region's moved so, once.)
    if let Some((name, v)) = std::env::var("CAP_SLIDE").ok().and_then(|v| v.split_once(':').and_then(|(n, x)| Some((n.to_string(), x.parse::<f32>().ok()?))))
        && cap.adv_shown.is_some()
        && !cap.adv_slid
        && let Some(&n) = list.iter().find(|n| d.sliders[**n].name.eq_ignore_ascii_case(&name))
    {
        cap.adv_slid = true;
        let p = &mut pending.pets[i];
        for (b, a) in slider_set(&d.sliders[n], v) {
            match p.sliders.iter_mut().find(|x| x.0 == b) {
                Some(x) => x.1 = a,
                None => p.sliders.push((b, a)),
            }
        }
        info!("create a pet test: {name} at {v}");
        moved = true;
        cap.adv_shown = None;
    }
    for (e, n, last) in cap.adv_sliders.clone() {
        let Ok(sl) = sliders.get(e) else { continue };
        if (sl.value - last).abs() < 0.5 {
            continue;
        }
        let v = sl.value / 256.0;
        let p = &mut pending.pets[i];
        for (b, a) in slider_set(&d.sliders[n], v) {
            match p.sliders.iter_mut().find(|x| x.0 == b) {
                Some(x) => x.1 = a,
                None => p.sliders.push((b, a)),
            }
        }
        if let Some(x) = cap.adv_sliders.iter_mut().find(|x| x.0 == e) {
            x.2 = sl.value;
        }
        moved = true;
    }
    if moved
        && let (Some((model, _)), Some(breed)) = (cap.model, breed)
        && let Ok(mut skel) = skels.get_mut(model)
    {
        let p = &pending.pets[i];
        skel.reshape(&d.shape(&p.kind, breed, &p.shapes, &p.sliders));
    }
    let Some(g) = ui.find("CAPFBDAdvanced", ADV_GRID).and_then(|w| w.grid) else { return };
    let rows = g.rows.max(1) as usize;
    let most = list.len().saturating_sub(rows);
    let over_grid = s.id(ADV_GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_grid && wheel.delta.y != 0.0 {
        cap.adv_scroll = (cap.adv_scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        if let Some(mut b) = cap.adv_bar.and_then(|e| bars.get_mut(e).ok()) {
            b.value = cap.adv_scroll;
        }
    } else if let Some(b) = cap.adv_bar.and_then(|e| bars.get(e).ok())
        && b.value != cap.adv_scroll
    {
        cap.adv_scroll = b.value.min(most);
    }
    if let Some(mut b) = cap.adv_bar.and_then(|e| bars.get_mut(e).ok())
        && (b.total, b.visible) != (list.len(), rows)
    {
        (b.total, b.visible) = (list.len(), rows);
    }
    crate::livehud::set_visible(&mut vis, cap.adv_bar, list.len() > rows);
    let want = (i, pet.kind.clone(), cap.adv_body, region, cap.adv_scroll);
    if cap.adv_shown.as_ref() == Some(&want) {
        return;
    }
    cap.adv_shown = Some(want);
    // The species' minimap, at the head or the body; the region lit.
    for (sp, id) in ADV_MINIMAPS {
        crate::livehud::set_visible(&mut vis, s.id(id), sp == species);
    }
    if let Some(m) = map {
        crate::livehud::set_visible(&mut vis, s.within(m, ADV_HEAD), !cap.adv_body);
        crate::livehud::set_visible(&mut vis, s.within(m, ADV_BODY), cap.adv_body);
        crate::livehud::set_visible(&mut vis, s.within(m, ADV_TO_BODY), !cap.adv_body);
        crate::livehud::set_visible(&mut vis, s.within(m, ADV_TO_HEAD), cap.adv_body);
    }
    let mut label = String::new();
    for (e, r) in &regions {
        let lit = r.region == cap.adv_region;
        if let Ok(mut b) = buttons.get_mut(e)
            && b.selected != lit
        {
            b.selected = lit;
        }
        if lit && label.is_empty() {
            label = ui.layout("CAPFBDAdvanced").and_then(|l| l.find(r.region)).map(|w| ui.localize(&w.tooltip).filter(|t| !t.is_empty()).unwrap_or_else(|| w.comment.clone())).unwrap_or_default();
        }
    }
    if let Some(t) = s.text(ADV_LABEL) {
        commands.entity(t).insert(Text::new(label));
    }
    // The region's sliders.
    if let Some(old) = cap.adv_rows.take() {
        commands.entity(old).try_despawn();
    }
    cap.adv_sliders.clear();
    let (Some(grid), Some(template)) = (s.id(ADV_GRID), ui.layout("CASBodySlider").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(grid).insert_children(0, &[holder]);
    cap.adv_rows = Some(holder);
    let amounts = slider_amounts(&d, &pending.pets[i]);
    let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
    for (k, &n) in list.iter().enumerate().skip(cap.adv_scroll).take(rows) {
        let sl = &d.sliders[n];
        let (x, y) = (g.padding[0], g.padding[1] + (k - cap.adv_scroll) as f32 * step);
        let mut row = template.clone();
        row.area = [x, y, x + g.cell[0], y + g.cell[1]];
        crate::layout::edit_windows(&mut row, ROW_TITLE, &mut |t| t.caption = sl.name.clone());
        let v = (slider_value(sl, &amounts) * 256.0).round();
        crate::layout::edit_windows(&mut row, ROW_SLIDER, &mut |w| {
            if let Some(range) = w.slider.as_mut() {
                (range[0], range[1], range[2]) = (if sl.bidirectional { -256.0 } else { 0.0 }, 256.0, v);
            }
        });
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &row, holder);
        if let Some(e) = c.id(ROW_SLIDER) {
            cap.adv_sliders.push((e, n, v));
        }
    }
}

/// Where the Advanced panel sits (its layout's at the screen's edge).
const ADV_LEFT: f32 = 91.0;

/// A shape's cell in the grid (its compound blend's instance).
#[derive(Component)]
struct ShapeCell(u64);

/// A pet's age, species and sex bits, as Create a Pet's pictures of its shapes are filed.
fn shape_thumb_flags(kind: &str, female: bool) -> [u32; 4] {
    let (age, species) = s3bake::gamedata::pet_flags(kind);
    let sex = if female { 0x2000 } else { 0x1000 };
    let other = 0x3000 ^ sex;
    // (Its own; the other sex's; a grown one's, of either.)
    [species | age | sex, species | age | other, species | 0x20 | sex, species | 0x20 | other]
}

/// Shape (`CAPFBDBasic` and `CAPFaceBody`): the tabs for the full body, body, head, eyes, snout
/// and ears (those the species has shapes for), and the chosen one's shapes in the grid, two by
/// two, each the game's picture of it; a click shapes the pet so (`PetsBaked::shape`: the shape's
/// blends over its breed's), the one it has lit. The wheel or the bar turns the grid's rows.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shape_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts, mut game_ui): (ResMut<Assets<Image>>, ResMut<Assets<Font>>, Option<ResMut<crate::icons::GameUi>>),
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    (clicks, cells, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &ShapeCell), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    (mut vis, mut buttons, mut bars, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>, Query<&mut Text>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
) {
    let Some(mut ui) = ui else { return };
    let on = cap.panel == Some(Panel::Shape) && !cap.adv_open;
    if on && cap.fbd.is_none() && ui.layout("CAPFBDBasic").is_some() && ui.layout("CAPFaceBody").is_some() {
        cap.fbd = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPFBDBasic", 6);
        cap.face_body = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPFaceBody", 7);
        if let Some(s) = &cap.fbd {
            // (The short background, as `SetLongPanel(use: false)` has it.)
            for (id, show) in [(FBD_LONG, false), (FBD_SHORT, true)] {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(if show { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
            if let Some(e) = s.id(FBD_DONE) {
                commands.entity(e).insert(crate::icons::Tooltip(ui.localize("Ui/Caption/Global:Done").unwrap_or_else(|| "Done".into())));
            }
            for (button, _, key, or) in SHAPE_TABS {
                let tip = ui.localize(key).filter(|t| !t.is_empty()).unwrap_or_else(|| or.to_string());
                for e in s.all_with(button) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.clone()));
                }
            }
        }
        if let Some(s) = &cap.face_body {
            // (Advanced: the sliders, `CAPFBDAdvanced`.)
            let tip = ui.localize("Ui/Tooltip/CAS/CAP/FaceBody:Advanced").filter(|t| !t.is_empty()).unwrap_or_else(|| "Advanced".into());
            for id in SHAPE_ADVANCED {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.clone()));
                }
            }
            if let (Some(grid), Some(g), Some(mut b)) = (s.id(SHAPE_GRID), ui.find("CAPFaceBody", SHAPE_GRID).cloned(), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
                let (w, h) = (g.area[2] - g.area[0], g.area[3] - g.area[1]);
                b.flags |= s3bake::ui::WIN_VISIBLE;
                b.place = s3bake::ui::UiPlace::Fixed;
                b.area = [w - 20.0, 0.0, w - 2.0, h - 4.0];
                cap.shape_bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, grid).root;
                commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default()).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::clip());
            }
        }
        return;
    }
    let (Some(fbd), Some(s)) = (cap.fbd.clone(), cap.face_body.clone()) else { return };
    crate::livehud::set_visible(&mut vis, fbd.root, on);
    crate::livehud::set_visible(&mut vis, s.root, on);
    let (Some(i), Some(d)) = (cap.pet.filter(|_| on), data.0.clone()) else {
        cap.shape_shown = None;
        return;
    };
    if crate::livehud::pressed(&clicks, fbd.id(FBD_DONE)) {
        cap.panel = None;
        return;
    }
    let pet = pending.pets[i].clone();
    let species = pet.kind.get(1..).map_or("d", |s| if s == "l" { "d" } else { s });
    let tabs = FBD_TABS.iter().find(|t| t.0 == species).and_then(|t| fbd.id(t.1));
    // (The species' Advanced button: to its sliders, at the region shown, as `SetDefaultPart`.)
    for (sp, id) in ["d", "c", "h"].into_iter().zip(SHAPE_ADVANCED) {
        crate::livehud::set_visible(&mut vis, s.id(id), sp == species);
        if sp == species && crate::livehud::pressed(&clicks, s.id(id)) {
            let (region, body) = advanced_region(cap.shape_region, species == "h");
            (cap.adv_open, cap.adv_region, cap.adv_body, cap.adv_scroll, cap.adv_shown) = (true, region, body, 0, None);
            return;
        }
    }
    let offered: Vec<(u32, u32, &str, &str)> = SHAPE_TABS.iter().copied().filter(|t| !d.presets_for(&pet.kind, t.1).is_empty()).collect();
    if !offered.iter().any(|t| t.1 == cap.shape_region) {
        cap.shape_region = offered.first().map_or(s3bake::pets::PET_FULL_BODY, |t| t.1);
        cap.shape_scroll = 0;
    }
    for (button, region, ..) in &offered {
        if crate::livehud::pressed(&clicks, tabs.and_then(|p| fbd.within(p, *button))) && cap.shape_region != *region {
            cap.shape_region = *region;
            cap.shape_scroll = 0;
        }
    }
    let list = d.presets_for(&pet.kind, cap.shape_region);
    for (x, c) in &cells {
        if *x == Interaction::Pressed {
            // (One shape a region: the latest laid over the rest.)
            let region = cap.shape_region;
            let shapes = &mut pending.pets[i].shapes;
            shapes.retain(|s| d.presets.iter().find(|p| p.instance == *s).is_none_or(|p| p.region != region));
            shapes.push(c.0);
            // (Its blends as it has them, not as the sliders last left them.)
            if let Some(p) = d.presets.iter().find(|p| p.instance == c.0) {
                pending.pets[i].sliders.retain(|(b, _)| !p.blends.iter().any(|x| x.0 == *b));
            }
            info!("create a pet: shaped {}", d.presets.iter().find(|p| p.instance == c.0).map_or("", |p| p.name.as_str()));
        }
    }
    let Some(g) = ui.find("CAPFaceBody", SHAPE_GRID).and_then(|w| w.grid) else { return };
    let cols = g.columns.max(1) as usize;
    let rows = list.len().div_ceil(cols);
    let most = rows.saturating_sub(g.rows.max(1) as usize);
    let over_grid = s.id(SHAPE_GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_grid && wheel.delta.y != 0.0 {
        cap.shape_scroll = (cap.shape_scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        if let Some(mut b) = cap.shape_bar.and_then(|e| bars.get_mut(e).ok()) {
            b.value = cap.shape_scroll;
        }
    } else if let Some(b) = cap.shape_bar.and_then(|e| bars.get(e).ok())
        && b.value != cap.shape_scroll
    {
        cap.shape_scroll = b.value.min(most);
    }
    if let Some(mut b) = cap.shape_bar.and_then(|e| bars.get_mut(e).ok())
        && (b.total, b.visible) != (rows, g.rows.max(1) as usize)
    {
        (b.total, b.visible) = (rows, g.rows.max(1) as usize);
    }
    let shapes = pending.pets[i].shapes.clone();
    let want = (i, pet.kind.clone(), cap.shape_region, shapes.clone(), cap.shape_scroll, game_ui.is_some());
    if cap.shape_shown.as_ref() == Some(&want) {
        return;
    }
    cap.shape_shown = Some(want);
    // The species' tabs, those with shapes, the region's lit.
    for (sp, panel) in FBD_TABS {
        crate::livehud::set_visible(&mut vis, fbd.id(panel), sp == species);
    }
    for (button, region, ..) in SHAPE_TABS {
        let e = tabs.and_then(|p| fbd.within(p, button));
        crate::livehud::set_visible(&mut vis, e, offered.iter().any(|t| t.1 == region));
        if let Some(mut b) = e.and_then(|e| buttons.get_mut(e).ok())
            && b.selected != (region == cap.shape_region)
        {
            b.selected = region == cap.shape_region;
        }
    }
    if let Some((_, _, key, or)) = SHAPE_TABS.iter().find(|t| t.1 == cap.shape_region) {
        let title = ui.localize(key).filter(|t| !t.is_empty()).unwrap_or_else(|| or.to_string());
        crate::livehud::set_text(&mut texts, s.text(SHAPE_TITLE), &title);
    }
    if let Some(old) = cap.shape_cells.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(grid), Some(template)) = (s.id(SHAPE_GRID), ui.layout("GenericCasItemGrow_landscape").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(grid).insert_children(0, &[holder]);
    cap.shape_cells = Some(holder);
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    let flags = shape_thumb_flags(&pet.kind, pet.female);
    for (n, p) in list.iter().enumerate().skip(cap.shape_scroll * cols).take(cols * g.rows.max(1) as usize) {
        let k = n - cap.shape_scroll * cols;
        let (x, y) = (g.padding[0] + (k % cols) as f32 * step.x, g.padding[1] + (k / cols) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        commands.entity(root).insert(ShapeCell(p.instance));
        if shapes.contains(&p.instance) {
            commands.entity(root).insert(crate::layout::Selected);
        }
        let thumb = game_ui.as_deref_mut().and_then(|gu| flags.iter().find_map(|f| gu.icon(&mut images, &s3bake::gamedata::face_thumb_name(p.instance, *f))));
        if let (Some(win), Some(h)) = (c.id(CELL_THUMBNAIL), thumb) {
            crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
        }
    }
}

// `CAPCoat`'s windows (`ControlIDs`): its title, the coats' grid, the colours' grid and its
// buttons, and what isn't here (the filter, saving and sharing coats, the Advanced painting).
const COAT_TITLE: u32 = 0x05db_cf01;
const COATS_GRID: u32 = 0x05db_cf21;
const COAT_COLORS: u32 = 0x05db_cf22;
const COAT_TRASH: u32 = 0x05db_b903;
const COAT_PICKER: u32 = 0x05db_b950;
const COAT_NOT_HERE: [u32; 3] = [0x05db_b90f, 0x05db_cf30, 0x05db_b90d];
// `CAPCCMBasic`'s, the frame Coat sits in: its species' tabs, Close, the long backing.
const CCM_CLOSE: u32 = 0x05db_9c00;
const CCM_COAT: u32 = 0x05db_9c03;
const CCM_TABS: [(&str, u32); 3] = [("h", 0x05db_9e00), ("d", 0x05db_9e01), ("c", 0x05db_9e02)];
const CCM_LONG: u32 = 0x05db_9d00;
const CCM_SHORT: u32 = 0x05db_9d01;
/// The long backing's height under Coat (`SetLongPanelHeight`: down past its colours).
const CCM_LONG_HEIGHT: f32 = 530.0;
/// The short one's under the tail, mane and feathering (down past their grid).
const CCM_SHORT_HEIGHT: f32 = 370.0;
/// The frame's buttons, and the tab each opens (none: not here): Coat, Fur Shape, Mane, Tail,
/// Hooves, Unicorn.
const CCM_BUTTONS: [(u32, Option<CcmTab>); 6] = [
    (CCM_COAT, Some(CcmTab::Coat)),
    (0x05db_9c06, None),
    (0x05db_9c09, Some(CcmTab::Mane)),
    (0x05db_9c0a, Some(CcmTab::Tail)),
    (0x05db_9c0c, Some(CcmTab::Hooves)),
    (0x05db_9c0f, None),
];

/// Which of the coat frame's panels is up.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum CcmTab {
    #[default]
    Coat,
    Tail,
    Mane,
    /// A horse's feathering (its hoof hair).
    Hooves,
}

/// A tail's or mane's cell (its part's name).
#[derive(Component)]
struct PartCell(String);

/// Tail and Mane (`CAPColoringTMH` in the coat's frame): the kind's tails (or a horse's manes),
/// two by two with the game's pictures of them, the pet's lit; a click puts it on the pet.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tmh_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts, mut gu): (ResMut<Assets<Image>>, ResMut<Assets<Font>>, Option<ResMut<crate::icons::GameUi>>),
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    (cells, over): (Query<(&Interaction, &PartCell), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    (mut vis, mut bars): (Query<&mut Visibility>, Query<&mut UiScrollBar>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
) {
    let Some(mut ui) = ui else { return };
    let on = cap.panel == Some(Panel::Coat) && cap.ccm_tab != CcmTab::Coat;
    if on && cap.tmh.is_none() && ui.layout("CAPColoringTMH").is_some() {
        cap.tmh = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPColoringTMH", 7);
        if let Some(s) = cap.tmh.clone() {
            // (Its colours, filter and saving aren't here: the coat's colour is the tail's.)
            for id in [0x05db_b90f, 0x05db_cf30, 0x05db_cf20] {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
            if let (Some(grid), Some(g), Some(mut b)) = (s.id(COATS_GRID), ui.find("CAPColoringTMH", COATS_GRID).cloned(), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
                let (w, h) = (g.area[2] - g.area[0], g.area[3] - g.area[1]);
                b.flags |= s3bake::ui::WIN_VISIBLE;
                b.place = s3bake::ui::UiPlace::Fixed;
                b.area = [w - 20.0, 0.0, w - 2.0, h - 4.0];
                cap.tmh_bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, grid).root;
                commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default()).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::clip());
            }
        }
        return;
    }
    let Some(s) = cap.tmh.clone() else { return };
    crate::livehud::set_visible(&mut vis, s.root, on);
    let (Some(i), Some(d)) = (cap.pet.filter(|_| on), data.0.clone()) else {
        cap.tmh_shown = None;
        return;
    };
    let pet = pending.pets[i].clone();
    let (Some(kind), Some(breed)) = (d.kinds.iter().find(|k| k.code == pet.kind), d.breeds.iter().find(|b| b.outfit == pet.outfit)) else { return };
    let tab = cap.ccm_tab;
    // (Its parts; a horse's feathering has bare hooves first: an empty name.)
    let (parts, own): (Vec<(String, u64)>, String) = match tab {
        CcmTab::Mane => (kind.manes.iter().map(|p| (p.name.clone(), p.instance)).collect(), breed.mane.clone().unwrap_or_default()),
        CcmTab::Hooves => (std::iter::once((String::new(), 0)).chain(kind.hooves.iter().map(|p| (p.name.clone(), p.instance))).collect(), breed.hooves.clone().unwrap_or_default()),
        _ => (kind.tails.iter().map(|p| (p.name.clone(), p.instance)).collect(), breed.tail.clone().unwrap_or_default()),
    };
    fn slot(p: &mut SavedPet, tab: CcmTab) -> &mut Option<String> {
        match tab {
            CcmTab::Mane => &mut p.mane,
            CcmTab::Hooves => &mut p.hooves,
            _ => &mut p.tail,
        }
    }
    for (x, c) in &cells {
        if *x == Interaction::Pressed {
            *slot(&mut pending.pets[i], tab) = (c.0 != own).then(|| c.0.clone());
        }
    }
    // (CAP_PICK=<n> (tests): the nth chosen, once.)
    if let Some(n) = std::env::var("CAP_PICK").ok().and_then(|v| v.parse::<usize>().ok()).filter(|_| cap.tmh_shown.is_none())
        && let Some((p, _)) = parts.get(n)
    {
        *slot(&mut pending.pets[i], tab) = Some(p.clone());
        info!("create a pet test: the {tab:?} {p:?}");
    }
    let worn = slot(&mut pending.pets[i], tab).clone().unwrap_or(own);
    let list = parts;
    let mane = tab;
    let Some(g) = ui.find("CAPColoringTMH", COATS_GRID).and_then(|w| w.grid) else { return };
    let cols = g.columns.max(1) as usize;
    let rows = list.len().div_ceil(cols);
    let most = rows.saturating_sub(g.rows.max(1) as usize);
    let over_grid = s.id(COATS_GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_grid && wheel.delta.y != 0.0 {
        cap.tmh_scroll = (cap.tmh_scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        if let Some(mut b) = cap.tmh_bar.and_then(|e| bars.get_mut(e).ok()) {
            b.value = cap.tmh_scroll;
        }
    } else if let Some(b) = cap.tmh_bar.and_then(|e| bars.get(e).ok())
        && b.value != cap.tmh_scroll
    {
        cap.tmh_scroll = b.value.min(most);
    }
    if let Some(mut b) = cap.tmh_bar.and_then(|e| bars.get_mut(e).ok())
        && (b.total, b.visible) != (rows, g.rows.max(1) as usize)
    {
        (b.total, b.visible) = (rows, g.rows.max(1) as usize);
    }
    let want = (i, mane, worn.clone(), cap.tmh_scroll, gu.is_some());
    if cap.tmh_shown.as_ref() == Some(&want) {
        return;
    }
    cap.tmh_shown = Some(want);
    if let Some(t) = s.text(COAT_TITLE) {
        let (key, or) = match tab {
            CcmTab::Mane => ("Ui/Caption/CAS/CAP/Mane:Title", "Mane"),
            CcmTab::Hooves => ("Ui/Caption/CAS/CAP/Hooves:Title", "Feathering"),
            _ => ("Ui/Caption/CAS/CAP/Tail:Title", "Tail"),
        };
        commands.entity(t).insert(Text::new(ui.localize(key).filter(|t| !t.is_empty()).unwrap_or_else(|| or.into())));
    }
    if let Some(old) = cap.tmh_cells.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(grid), Some(template)) = (s.id(COATS_GRID), ui.layout("GenericCasItemGrow_landscape").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(grid).insert_children(0, &[holder]);
    cap.tmh_cells = Some(holder);
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    for (n, (name, instance)) in list.iter().enumerate().skip(cap.tmh_scroll * cols).take(cols * g.rows.max(1) as usize) {
        let k = n - cap.tmh_scroll * cols;
        let (x, y) = (g.padding[0] + (k % cols) as f32 * step.x, g.padding[1] + (k / cols) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        let tip = if name.is_empty() { "None".to_string() } else { part_words(name) };
        commands.entity(root).insert((PartCell(name.clone()), crate::icons::Tooltip(tip)));
        if worn == *name {
            commands.entity(root).insert(crate::layout::Selected);
        }
        // (Bare hooves: the game's empty cell, `AddHairTypeGridItem(null)`.)
        let pic = (*instance != 0).then(|| gu.as_deref_mut().and_then(|g| g.icon(&mut images, &s3bake::gamedata::cas_thumb_name(*instance)))).flatten();
        if let (Some(win), Some(h)) = (c.id(CELL_THUMBNAIL), pic) {
            crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
        }
    }
}

/// A coat's cell (its breed's outfit), and a colour's.
#[derive(Component)]
struct CoatCell(u64);
#[derive(Component)]
struct CoatColour([f32; 3]);

/// Coat (`CAPCoat`): the coats of the pet's kind, each breed's, two by two with the game's
/// pictures (`AddBreedCoats`); a click gives the pet that coat, keeping its breed's build (the
/// game's `colorationOnly`). Under them, colours to shift the coat to (the natural fur colours;
/// Color, the picker, for any other; the bin, back to its own colours).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn coat_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    (clicks, cells, colours, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &CoatCell), Changed<Interaction>>, Query<(&Interaction, &CoatColour), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    (mut vis, mut bars, mut texts, mut buttons): (Query<&mut Visibility>, Query<&mut UiScrollBar>, Query<&mut Text>, Query<&mut UiButton>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut picked: MessageReader<crate::cas::CasActionRequest>,
) {
    let Some(mut ui) = ui else { return };
    let on = cap.panel == Some(Panel::Coat);
    // (A colour of its own from the picker.)
    for a in picked.read() {
        if let CasAction::CoatRgb(c) = a.0
            && let Some(p) = cap.pet.and_then(|i| pending.pets.get_mut(i))
        {
            p.tint = Some(c);
        }
    }
    if on && cap.coat.is_none() && ui.layout("CAPCoat").is_some() && ui.layout("CAPCCMBasic").is_some() {
        cap.ccm = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPCCMBasic", 6);
        cap.coat = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPCoat", 7);
        if let Some(f) = cap.ccm.clone() {
            for (id, show) in [(CCM_LONG, true), (CCM_SHORT, false)] {
                if let Some(e) = f.id(id) {
                    commands.entity(e).insert(if show { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
            if let Some(e) = f.id(CCM_LONG) {
                commands.entity(e).entry::<Node>().and_modify(|mut n| n.height = Val::Px(CCM_LONG_HEIGHT));
            }
            if let Some(e) = f.id(CCM_SHORT) {
                commands.entity(e).entry::<Node>().and_modify(|mut n| n.height = Val::Px(CCM_SHORT_HEIGHT));
            }
            if let Some(e) = f.id(CCM_CLOSE) {
                commands.entity(e).insert(crate::icons::Tooltip("Close".into()));
            }
            for e in f.all_with(CCM_COAT) {
                commands.entity(e).insert(crate::icons::Tooltip("Coat".into()));
            }
        }
        if let Some(s) = cap.coat.clone() {
            for id in COAT_NOT_HERE {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
            for (id, tip) in [(COAT_TRASH, "Delete"), (COAT_PICKER, "Color")] {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
            if let (Some(grid), Some(g), Some(mut b)) = (s.id(COATS_GRID), ui.find("CAPCoat", COATS_GRID).cloned(), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
                let (w, h) = (g.area[2] - g.area[0], g.area[3] - g.area[1]);
                b.flags |= s3bake::ui::WIN_VISIBLE;
                b.place = s3bake::ui::UiPlace::Fixed;
                b.area = [w - 20.0, 0.0, w - 2.0, h - 4.0];
                cap.coat_bar = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, grid).root;
                commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default()).entry::<Node>().and_modify(|mut n| n.overflow = Overflow::clip());
            }
            if let Some(t) = s.text(COAT_TITLE) {
                let title = ui.localize("Ui/Caption/CAS/CAP/Coat:Title").filter(|t| !t.is_empty()).unwrap_or_else(|| "Coat".into());
                commands.entity(t).insert(Text::new(title));
            }
        }
        return;
    }
    let (Some(s), Some(frame)) = (cap.coat.clone(), cap.ccm.clone()) else { return };
    crate::livehud::set_visible(&mut vis, frame.root, on);
    let pet = cap.pet.filter(|_| on).and_then(|i| pending.pets.get(i)).cloned();
    if let Some(pet) = &pet {
        if crate::livehud::pressed(&clicks, frame.id(CCM_CLOSE)) {
            cap.panel = None;
            return;
        }
        // (The frame's tabs for its species: Coat, Tail, a horse's Mane; the fur's shape, hooves
        // and horn aren't here.)
        let species = pet.kind.get(1..).map_or("d", |s| if s == "l" { "d" } else { s });
        let offered = |tab: CcmTab| !matches!(tab, CcmTab::Mane | CcmTab::Hooves) || species == "h";
        for (sp, panel) in CCM_TABS {
            crate::livehud::set_visible(&mut vis, frame.id(panel), sp == species);
            let Some(p) = frame.id(panel) else { continue };
            for (id, tab) in CCM_BUTTONS {
                let e = frame.within(p, id);
                if sp == species
                    && let Some(t) = tab.filter(|t| offered(*t))
                    && crate::livehud::pressed(&clicks, e)
                    && cap.ccm_tab != t
                {
                    cap.ccm_tab = t;
                    cap.coat_shown = None;
                    cap.tmh_shown = None;
                }
                if let Some(mut b) = e.and_then(|e| buttons.get_mut(e).ok()) {
                    let (sel, off) = (tab == Some(cap.ccm_tab), !tab.is_some_and(offered));
                    if (b.selected, b.disabled) != (sel, off) {
                        (b.selected, b.disabled) = (sel, off);
                    }
                }
            }
        }
        if !offered(cap.ccm_tab) {
            cap.ccm_tab = CcmTab::Coat;
        }
        // (The long backing under the coat and its colours; the short one, cut to the grid, under
        // the parts: `SetLongPanel`, `SetShortPanelHeight`.)
        let long = cap.ccm_tab == CcmTab::Coat;
        crate::livehud::set_visible(&mut vis, frame.id(CCM_LONG), long);
        crate::livehud::set_visible(&mut vis, frame.id(CCM_SHORT), !long);
    }
    let on = on && cap.ccm_tab == CcmTab::Coat;
    crate::livehud::set_visible(&mut vis, s.root, on);
    let (Some(i), Some(d)) = (cap.pet.filter(|_| on), data.0.clone()) else {
        cap.coat_shown = None;
        return;
    };
    let pet = pending.pets[i].clone();
    for (x, c) in &cells {
        if *x == Interaction::Pressed {
            pending.pets[i].coat = (c.0 != pet.outfit).then_some(c.0);
            info!("create a pet: the coat of {:016x}", c.0);
        }
    }
    for (x, c) in &colours {
        if *x == Interaction::Pressed {
            pending.pets[i].tint = Some(c.0);
        }
    }
    if crate::livehud::pressed(&clicks, s.id(COAT_TRASH)) {
        pending.pets[i].tint = None;
    }
    if crate::livehud::pressed(&clicks, s.id(COAT_PICKER)) {
        commands.insert_resource(crate::cascolour::PickColour(crate::cascolour::PickFor::Coat, pet.tint.unwrap_or([0.45, 0.32, 0.2])));
    }
    if let Some(mut b) = s.id(COAT_TRASH).and_then(|e| buttons.get_mut(e).ok())
        && b.disabled != pending.pets[i].tint.is_none()
    {
        b.disabled = pending.pets[i].tint.is_none();
    }
    let list = listed(&d, &pet.kind);
    let Some(g) = ui.find("CAPCoat", COATS_GRID).and_then(|w| w.grid) else { return };
    let cols = g.columns.max(1) as usize;
    let rows = list.len().div_ceil(cols);
    let most = rows.saturating_sub(g.rows.max(1) as usize);
    let over_grid = s.id(COATS_GRID).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_grid && wheel.delta.y != 0.0 {
        cap.coat_scroll = (cap.coat_scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        if let Some(mut b) = cap.coat_bar.and_then(|e| bars.get_mut(e).ok()) {
            b.value = cap.coat_scroll;
        }
    } else if let Some(b) = cap.coat_bar.and_then(|e| bars.get(e).ok())
        && b.value != cap.coat_scroll
    {
        cap.coat_scroll = b.value.min(most);
    }
    if let Some(mut b) = cap.coat_bar.and_then(|e| bars.get_mut(e).ok())
        && (b.total, b.visible) != (rows, g.rows.max(1) as usize)
    {
        (b.total, b.visible) = (rows, g.rows.max(1) as usize);
    }
    let worn = pending.pets[i].coat.unwrap_or(pet.outfit);
    let tint = pending.pets[i].tint.map(|c| c.map(|v| (v * 255.0) as u8));
    let want = (i, pet.kind.clone(), worn, tint, cap.coat_scroll);
    if cap.coat_shown.as_ref() == Some(&want) {
        return;
    }
    let fresh = cap.coat_shown.as_ref().is_none_or(|w| w.0 != i || w.1 != pet.kind);
    cap.coat_shown = Some(want);
    let _ = &mut texts;
    // The coats.
    if let Some(old) = cap.coat_cells.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(grid), Some(template)) = (s.id(COATS_GRID), ui.layout("GenericCasItemGrow_landscape").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(grid).insert_children(0, &[holder]);
    cap.coat_cells = Some(holder);
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    for (n, b) in list.iter().enumerate().skip(cap.coat_scroll * cols).take(cols * g.rows.max(1) as usize) {
        let k = n - cap.coat_scroll * cols;
        let (x, y) = (g.padding[0] + (k % cols) as f32 * step.x, g.padding[1] + (k / cols) as f32 * step.y);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        cell.cls = "Button".into();
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
        let Some(root) = c.root else { continue };
        commands.entity(root).insert((CoatCell(b.outfit), crate::icons::Tooltip(b.name.clone())));
        if b.outfit == worn {
            commands.entity(root).insert(crate::layout::Selected);
        }
        if let (Some(win), Some(h)) = (c.id(CELL_THUMBNAIL), cap.thumb(&mut images, b)) {
            crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
        }
    }
    // The colours, once (they don't change).
    if fresh && let (Some(cgrid), Some(cg), Some(swatch)) = (s.id(COAT_COLORS), ui.find("CAPCoat", COAT_COLORS).and_then(|w| w.grid), ui.layout("HairColorShiftPresetGridItem").cloned()) {
        if let Some(old) = cap.coat_swatches.take() {
            commands.entity(old).try_despawn();
        }
        let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
        commands.entity(cgrid).insert_children(0, &[holder]);
        cap.coat_swatches = Some(holder);
        for (n, c) in crate::sim::HAIRS.iter().take(cg.columns.max(1) as usize * cg.rows.max(1) as usize).enumerate() {
            let (x, y) = (cg.padding[0] + n as f32 * cg.cell[0], cg.padding[1]);
            let mut cell = swatch.clone();
            cell.area = [x, y, x + cg.cell[0], y + cg.cell[1]];
            cell.cls = "Button".into();
            let w = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, holder);
            let Some(root) = w.root else { continue };
            commands.entity(root).insert(CoatColour([c.0, c.1, c.2]));
            // (The swatch: the colour over the cell, inside its frame.)
            commands.entity(root).with_children(|p| {
                p.spawn((
                    Node { position_type: PositionType::Absolute, left: Val::Px(6.0), top: Val::Px(6.0), right: Val::Px(6.0), bottom: Val::Px(6.0), ..default() },
                    BackgroundColor(Color::srgb(c.0, c.1, c.2)),
                    ZIndex(3),
                    Pickable::IGNORE,
                ));
            });
        }
    }
}

// `CAPAccessories`' windows: Done, and the horse's outfits' tabs (one outfit here).
const ACC_DONE: u32 = 0x7ced_0001;
const ACC_HORSE_TABS: u32 = 0x7ced_0010;

/// An accessory's cell: its kind of part, and the part (0: none of that kind).
#[derive(Component)]
struct AccessoryCell(u32, u64);

/// What a kind of accessory's called, for a pet of a kind.
fn accessory_kind(part_type: u32, kind: &str) -> &'static str {
    match (part_type, kind) {
        (51, "ah") => "Bridle",
        (51, _) => "Collar",
        (52, _) => "Saddle",
        (57, _) => "Blanket",
        (58, _) => "Breast Collar",
        _ => "Accessory",
    }
}

/// A part's name as words: `ahSaddleWesternV2`, "Saddle Western V2".
fn part_words(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.get(2..).unwrap_or(name).chars().enumerate() {
        if ch == '_' {
            out.push(' ');
            continue;
        }
        let digit_after_letter = ch.is_ascii_digit() && out.ends_with(|c: char| c.is_ascii_alphabetic() && c != 'V');
        if i > 0 && (ch.is_uppercase() || digit_after_letter) && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(if i == 0 { ch.to_ascii_uppercase() } else { ch });
    }
    out
}

/// Accessories (`CAPAccessories`): the pet's kinds of accessories, a row (or rows) each in the
/// panel under its name, its parts along it with the game's pictures of them (and none), what it
/// wears lit; a click puts it on in place of the other of its kind. Scrolled by the wheel.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn accessories_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    (clicks, cells, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &AccessoryCell), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    mut vis: Query<&mut Visibility>,
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
) {
    let Some(mut ui) = ui else { return };
    let on = cap.panel == Some(Panel::Accessories);
    if on && cap.accessories.is_none() && ui.layout("CAPAccessories").is_some() {
        cap.accessories = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPAccessories", 6);
        if let Some(s) = &cap.accessories {
            for e in s.all_with(ACC_HORSE_TABS) {
                commands.entity(e).insert(Visibility::Hidden);
            }
            if let Some(e) = s.id(ACC_DONE) {
                commands.entity(e).insert(crate::icons::Tooltip("Done".into()));
            }
            if let Some(r) = s.root {
                commands.entity(r).insert(bevy::ui::RelativeCursorPosition::default());
            }
        }
        return;
    }
    let Some(s) = cap.accessories.clone() else { return };
    crate::livehud::set_visible(&mut vis, s.root, on);
    let (Some(i), Some(d)) = (cap.pet.filter(|_| on), data.0.clone()) else {
        cap.accessories_shown = None;
        return;
    };
    if crate::livehud::pressed(&clicks, s.id(ACC_DONE)) {
        cap.panel = None;
        return;
    }
    let kind = pending.pets[i].kind.clone();
    let Some(k) = d.kinds.iter().find(|k| k.code == kind) else { return };
    for (x, c) in &cells {
        if *x == Interaction::Pressed {
            let worn = &mut pending.pets[i].accessories;
            worn.retain(|w| k.accessories.iter().find(|a| a.instance == *w).is_none_or(|a| a.part_type != c.0));
            if c.1 != 0 {
                worn.push(c.1);
            }
        }
    }
    // (Its kinds of part, in the game's order, and their parts by name.)
    let mut types: Vec<u32> = k.accessories.iter().map(|a| a.part_type).collect();
    types.sort();
    types.dedup();
    let rows: Vec<(u32, Vec<&s3bake::pets::PetAccessory>)> = types
        .iter()
        .map(|t| {
            let mut v: Vec<&s3bake::pets::PetAccessory> = k.accessories.iter().filter(|a| a.part_type == *t).collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            (*t, v)
        })
        .collect();
    // (The game's cell, as big as it comes: as many across as the panel has room for.)
    let Some(area) = ui.layout("CAPAccessories").map(|l| l.area) else { return };
    let (w, h) = (area[2] - area[0], area[3] - area[1]);
    let Some(cell_area) = ui.layout("GenericCasItemGrow_landscape").map(|l| l.area) else { return };
    let cell = Vec2::new(cell_area[2] - cell_area[0], cell_area[3] - cell_area[1]);
    let cols = (((w - 64.0) / (cell.x + 4.0)).floor() as usize).max(1);
    let lines: Vec<(u32, bool, Vec<Option<&s3bake::pets::PetAccessory>>)> = rows
        .iter()
        .flat_map(|(t, v)| {
            let all: Vec<Option<&s3bake::pets::PetAccessory>> = std::iter::once(None).chain(v.iter().copied().map(Some)).collect();
            all.chunks(cols).enumerate().map(|(n, c)| (*t, n == 0, c.to_vec())).collect::<Vec<_>>()
        })
        .collect();
    let line = cell.y + 22.0;
    let fit = ((h - 84.0) / line).floor().max(1.0) as usize;
    let most = lines.len().saturating_sub(fit);
    let over_panel = s.root.and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    if over_panel && wheel.delta.y != 0.0 {
        cap.acc_scroll = (cap.acc_scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
    }
    cap.acc_scroll = cap.acc_scroll.min(most);
    let worn = pending.pets[i].accessories.clone();
    let want = (i, worn.clone(), cap.acc_scroll);
    if cap.accessories_shown.as_ref() == Some(&want) {
        return;
    }
    cap.accessories_shown = Some(want);
    if let Some(old) = cap.acc_cells.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(root), Some(template)) = (s.root, ui.layout("GenericCasItemGrow_landscape").cloned()) else { return };
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
    commands.entity(root).add_child(holder);
    cap.acc_cells = Some(holder);
    let (font, _) = ui.text_font(&mut fonts, 0);
    for (row, (t, first, parts)) in lines.iter().enumerate().skip(cap.acc_scroll).take(fit) {
        let y = 70.0 + (row - cap.acc_scroll) as f32 * line;
        // (Each kind's name over its first row.)
        if *first {
            let label = commands
                .spawn((
                    Node { position_type: PositionType::Absolute, left: Val::Px(32.0), top: Val::Px(y - 18.0), ..default() },
                    Text::new(accessory_kind(*t, &kind)),
                    TextFont { font_size: bevy::text::FontSize::Px(13.0), ..font.clone() },
                    TextColor(Color::srgb(0.12, 0.22, 0.42)),
                    Pickable::IGNORE,
                ))
                .id();
            commands.entity(holder).add_child(label);
        }
        for (col, part) in parts.iter().enumerate() {
            let x = 32.0 + col as f32 * (cell.x + 4.0);
            let mut c = template.clone();
            c.area = [x, y, x + cell.x, y + cell.y];
            c.cls = "Button".into();
            let spawned = ui.spawn_under(&mut commands, &mut images, &mut fonts, &c, holder);
            let Some(r) = spawned.root else { continue };
            let instance = part.map_or(0, |p| p.instance);
            let tip = part.map_or_else(|| format!("No {}", accessory_kind(*t, &kind)), |p| part_words(&p.name));
            commands.entity(r).insert((AccessoryCell(*t, instance), crate::icons::Tooltip(tip)));
            let chosen = match part {
                Some(p) => worn.contains(&p.instance),
                None => !rows.iter().find(|r| r.0 == *t).is_some_and(|r| r.1.iter().any(|p| worn.contains(&p.instance))),
            };
            if chosen {
                commands.entity(r).insert(crate::layout::Selected);
            }
            if let (Some(win), Some(p)) = (spawned.id(CELL_THUMBNAIL), part)
                && let Some(h) = gu.as_deref_mut().and_then(|g| g.icon(&mut images, &s3bake::gamedata::cas_thumb_name(p.instance)))
            {
                crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
            }
        }
    }
}

// `CAPCharacter`'s windows.
const CHAR_CLOSE: u32 = 0x05db_9900;
const CHAR_VOICES: u32 = 0x05db_8001;
const ADD_TRAITS: u32 = 0x05e5_3401;
const RANDOM_TRAITS: u32 = 0xaaaa_aaaa;
const TRAIT_SLOT: u32 = 0x0500_1001;

/// Character: the pet's traits in their sockets (five for a grown pet), the game's traits picker
/// for the pack's traits of its species (a socket or Add or Change Traits), or a random set.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn character_panel(
    mut commands: Commands,
    mut cap: ResMut<Cap>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pending: ResMut<PendingHousehold>,
    data: Res<crate::pets::PetData>,
    (clicks, rows, over): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &crate::traitpicker::TraitRow), Changed<Interaction>>, Query<&bevy::ui::RelativeCursorPosition>),
    (mut vis, mut buttons, mut bars): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut UiScrollBar>),
    (wheel, time, mut play): (Res<bevy::input::mouse::AccumulatedMouseScroll>, Res<Time>, MessageWriter<crate::sound::PlaySound>),
    mut gu: Option<ResMut<crate::icons::GameUi>>,
    (frame, mut darkened): (Option<Res<crate::caslook::CasFrame>>, Local<bool>),
) {
    let Some(mut ui) = ui else { return };
    // The panels behind the picker darkened while it's up, as the game's (and the Sims' own).
    if cap.picker.is_some() != *darkened {
        *darkened = cap.picker.is_some();
        let roots = frame.iter().flat_map(|f| f.roots()).chain([cap.sheet.as_ref().and_then(|s| s.root), cap.character.as_ref().and_then(|s| s.root)]).flatten();
        for root in roots {
            if *darkened {
                commands.entity(root).insert(crate::layout::Shade(120.0 / 255.0));
            } else {
                commands.entity(root).remove::<crate::layout::Shade>();
            }
        }
    }
    let on = cap.panel == Some(Panel::Character);
    if on && cap.character.is_none() && ui.layout("CAPCharacter").is_some() {
        cap.character = spawn_layout(&mut commands, &mut ui, &mut images, &mut fonts, "CAPCharacter", 6);
        if let Some(s) = &cap.character {
            for e in s.all_with(CHAR_VOICES) {
                commands.entity(e).insert(Visibility::Hidden);
            }
            for (id, tip) in [(RANDOM_TRAITS, "Randomize Traits"), (ADD_TRAITS, "Add or Change Traits"), (CHAR_CLOSE, "Close")] {
                if let Some(e) = s.id(id) {
                    commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
                }
            }
        }
        return;
    }
    let Some(s) = cap.character.clone() else { return };
    crate::livehud::set_visible(&mut vis, s.root, on);
    let Some(i) = cap.pet.filter(|_| on) else {
        cap.character_shown = None;
        if let Some(p) = cap.picker.take() {
            p.close(&mut commands);
        }
        return;
    };
    let kind: &'static str = crate::pettraits::KINDS.iter().copied().find(|k| *k == pending.pets[i].kind).unwrap_or("ac");
    // The picker, while it's up.
    if let Some(p) = cap.picker.as_mut() {
        match crate::traitpicker::update(p, &mut commands, &mut ui, (&mut images, &mut fonts), (&clicks, &rows, &over), (&wheel, time.elapsed_secs()), (&mut buttons, &mut bars), &mut play, gu.as_deref_mut()) {
            crate::traitpicker::Outcome::Open => {}
            crate::traitpicker::Outcome::Accepted(_) => {
                pending.pets[i].traits = p.pet_traits();
                p.close(&mut commands);
                cap.picker = None;
            }
            crate::traitpicker::Outcome::Cancelled => {
                p.close(&mut commands);
                cap.picker = None;
            }
        }
        return;
    }
    let pressed = |id: u32| crate::livehud::pressed(&clicks, s.id(id));
    if pressed(CHAR_CLOSE) {
        cap.panel = None;
        return;
    }
    if pressed(RANDOM_TRAITS) {
        pending.pets[i].traits = crate::pettraits::random_up_to(kind, crate::pettraits::max_traits(kind), &mut rand::rng()).into_iter().map(str::to_string).collect();
    }
    let asked = std::mem::take(&mut cap.test_picker);
    if (asked || pressed(ADD_TRAITS) || (0..5).any(|n| pressed(TRAIT_SLOT + n))) && crate::traitpicker::available(&ui) {
        let pet = &pending.pets[i];
        let pic = data.0.as_deref().and_then(|d| d.breeds.iter().find(|b| b.outfit == pet.outfit).cloned()).and_then(|b| cap.thumb(&mut images, &b));
        let x = ui.layout("TraitsPicker").map_or(500.0, |l| l.area[0]);
        cap.picker = crate::traitpicker::open_pet(&mut commands, &mut ui, &mut images, &mut fonts, &pet.name, (kind, pic), &pet.traits, Some(Vec2::new(x, 12.0)), (GlobalZIndex(8), DespawnOnExit(AppState::CreateHousehold)));
        return;
    }
    let traits = pending.pets[i].traits.clone();
    let want = (i, traits.clone());
    if cap.character_shown.as_ref() == Some(&want) {
        return;
    }
    cap.character_shown = Some(want);
    let slots = crate::pettraits::max_traits(kind);
    for n in 0..5 {
        let Some(e) = s.id(TRAIT_SLOT + n as u32) else { continue };
        commands.entity(e).insert(if n < slots { Visibility::Inherited } else { Visibility::Hidden });
        let info = traits.get(n).and_then(|h| gu.as_deref().and_then(|g| g.data.traits.iter().find(|t| &t.hex == h).cloned()));
        let icon = info.as_ref().and_then(|t| gu.as_deref_mut().and_then(|g| g.icon(&mut images, &t.icon)));
        match (info, icon) {
            (Some(t), Some(h)) => {
                commands.entity(e).insert((SetIcon(h), crate::icons::Tooltip(format!("{}\n{}", t.name, t.desc))));
            }
            _ => {
                if let Some(icon) = buttons.get(e).ok().and_then(|b| b.icon()) {
                    commands.entity(icon).insert(Visibility::Hidden);
                }
                commands.entity(e).insert(crate::icons::Tooltip("Add or Change Traits".into()));
            }
        }
    }
}

/// `CAP_TEST=<kind>[:basics|breed|character]` (tests): from the main menu into Create a Sim for
/// the first town, a pet of that kind made there, and that panel opened.
#[allow(clippy::too_many_arguments)]
fn cap_test(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    worlds: Option<Res<crate::data::WorldList>>,
    mut cap: ResMut<Cap>,
    mut asks: MessageWriter<CapRequest>,
    mut stage: Local<(u8, f32)>,
    (mut pending, data): (Option<ResMut<PendingHousehold>>, Res<crate::pets::PetData>),
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(v) = std::env::var("CAP_TEST") else { return };
    let (kind, panel) = v.split_once(':').unwrap_or((v.as_str(), ""));
    let kind: &'static str = crate::pettraits::KINDS.iter().copied().find(|k| *k == kind).unwrap_or("ac");
    let now = time.elapsed_secs();
    match (stage.0, state.get()) {
        (0, AppState::MainMenu) if now > 2.0 => {
            if let Some(w) = worlds.as_ref().and_then(|w| w.0.first()) {
                commands.insert_resource(crate::data::SelectedWorld(w.clone()));
                next.set(AppState::CreateHousehold);
                *stage = (1, now);
            }
        }
        (1, AppState::CreateHousehold) if now - stage.1 > 4.0 => {
            asks.write(CapRequest::New(kind));
            *stage = (2, now);
        }
        (2, AppState::CreateHousehold) if now - stage.1 > 1.0 => {
            cap.panel = match panel {
                "basics" => Some(Panel::Basics),
                "breed" => Some(Panel::Breed),
                "character" | "picker" => Some(Panel::Character),
                "accessories" => Some(Panel::Accessories),
                "shape" => Some(Panel::Shape),
                "coat" => Some(Panel::Coat),
                _ => None,
            };
            cap.test_picker = panel == "picker";
            // (CAP_SHAPE=<shape name>,...: shaped so; CAP_REGION=<region>: Shape turned to it.)
            if let (Ok(v), Some(pending), Some(d)) = (std::env::var("CAP_SHAPE"), pending.as_deref_mut(), data.0.as_deref())
                && let Some(p) = pending.pets.last_mut()
            {
                for name in v.split(',') {
                    if let Some(s) = d.presets.iter().find(|s| s.name.eq_ignore_ascii_case(name.trim())) {
                        p.shapes.retain(|x| d.presets.iter().find(|q| q.instance == *x).is_none_or(|q| q.region != s.region));
                        p.shapes.push(s.instance);
                        info!("create a pet test: shaped {}", s.name);
                    }
                }
            }
            // (CAP_COAT=<outfit name>: that breed's coat; CAP_TINT=r,g,b: shifted to that colour.)
            if let (Some(pending), Some(d)) = (pending.as_deref_mut(), data.0.as_deref())
                && let Some(p) = pending.pets.last_mut()
            {
                if let Ok(o) = std::env::var("CAP_COAT") {
                    let o = s3pkg::fnv64(&o);
                    p.coat = d.breeds.iter().any(|b| b.outfit == o).then_some(o);
                }
                if let Some(t) = std::env::var("CAP_TINT").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 3) {
                    p.tint = Some([t[0], t[1], t[2]]);
                }
            }
            // (CAP_CCM=tail|mane: the coat frame turned to it.)
            match std::env::var("CAP_CCM").as_deref() {
                Ok("tail") => cap.ccm_tab = CcmTab::Tail,
                Ok("mane") => cap.ccm_tab = CcmTab::Mane,
                Ok("hooves") => cap.ccm_tab = CcmTab::Hooves,
                _ => {}
            }
            // (CAP_ADV=<region, hex> (tests): Shape's sliders up, at that region.)
            if let Some(r) = std::env::var("CAP_ADV").ok().and_then(|r| u32::from_str_radix(r.trim_start_matches("0x"), 16).ok()) {
                (cap.adv_open, cap.adv_region, cap.adv_body) = (true, r, matches!(r, 2 | 0x200 | 0x400 | 0x800));
            }
            if let Some(r) = std::env::var("CAP_REGION").ok().and_then(|r| r.parse::<u32>().ok()) {
                cap.shape_region = r;
            }
            // (CAP_BREED=<outfit name>: of that breed.)
            if let (Ok(b), Some(pending)) = (std::env::var("CAP_BREED"), pending.as_deref_mut())
                && let Some(p) = pending.pets.last_mut()
            {
                p.outfit = s3pkg::fnv64(&b);
                info!("create a pet test: a {b}");
            }
            info!("create a pet test: a {kind}, {panel}");
            // (CAP_WEAR=<part name>,...: wearing those accessories.)
            if let (Ok(w), Some(i), Some(d), Some(pending)) = (std::env::var("CAP_WEAR"), cap.pet, data.0.as_deref(), pending.as_deref_mut())
                && let Some(k) = d.kinds.iter().find(|k| k.code == kind)
            {
                let names: Vec<String> = w.split(',').map(|n| n.trim().to_ascii_lowercase()).collect();
                pending.pets[i].accessories = k.accessories.iter().filter(|a| names.contains(&a.name.to_ascii_lowercase())).map(|a| a.instance).collect();
                info!("create a pet test: wearing {:?}", pending.pets[i].accessories);
            }
            *stage = (3, now);
        }
        // (CAP_SHOT=<png>: a picture of it, then away.)
        (3, AppState::CreateHousehold) if now - stage.1 > 3.0 => {
            if let Ok(path) = std::env::var("CAP_SHOT") {
                use bevy::render::view::screenshot::{Screenshot, save_to_disk};
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            }
            *stage = (4, now);
        }
        (4, _) if now - stage.1 > 2.0 && std::env::var("CAP_SHOT").is_ok() => {
            exit.write(AppExit::Success);
            *stage = (5, now);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessories_are_named_as_the_game_shows_them() {
        assert_eq!(part_words("ahSaddleWesternV2"), "Saddle Western V2");
        assert_eq!(part_words("acCollarJeweled_bell"), "Collar Jeweled bell");
        assert_eq!(accessory_kind(51, "ah"), "Bridle");
        assert_eq!(accessory_kind(51, "ad"), "Collar");
        assert_eq!(accessory_kind(52, "ah"), "Saddle");
    }

    #[test]
    fn a_slider_sets_one_blend_or_the_other() {
        let both = s3bake::pets::PetSlider { bidirectional: true, blends: [1, 2], ..Default::default() };
        let one = s3bake::pets::PetSlider { blends: [3, 0], ..Default::default() };
        // (Right: the first blend; left: the second, the first let go.)
        assert_eq!(slider_set(&both, 0.5), vec![(1, 0.5), (2, 0.0)]);
        assert_eq!(slider_set(&both, -0.25), vec![(1, 0.0), (2, 0.25)]);
        assert_eq!(slider_set(&one, -1.0), vec![(3, 0.0)]);
        assert_eq!(slider_value(&both, &[(1, 0.3), (2, 0.0)]), 0.3);
        assert_eq!(slider_value(&both, &[(1, 0.3), (2, 0.6)]), -0.6);
        assert_eq!(slider_value(&one, &[]), 0.0);
        // (Where Shape's tabs open the sliders.)
        assert_eq!(advanced_region(s3bake::pets::PET_SNOUT, true), (8, false));
        assert_eq!(advanced_region(s3bake::pets::PET_FULL_BODY, false), (0x200, true));
    }

    #[test]
    fn a_new_pet_is_the_mixed_breed_of_its_kind() {
        let breed = |kind: &str, outfit: u64, listed: Option<u32>| s3bake::pets::PetBreed { outfit, kind: kind.into(), listed, ..Default::default() };
        let data = s3bake::PetsBaked { version: 0, kinds: Vec::new(), breeds: vec![breed("ac", 1, Some(3)), breed("ac", 2, Some(0)), breed("ad", 3, None), breed("ac", 4, None)], ..Default::default() };
        let mut rng = rand::rng();
        let cat = new_pet(&data, "ac", &mut rng).unwrap();
        assert_eq!(cat.outfit, 2, "the list's first: the mixed breed");
        assert_eq!(listed(&data, "ac").iter().map(|b| b.outfit).collect::<Vec<_>>(), vec![2, 1], "only the listed, in order");
        // (A kind with no list: any of its breeds.)
        assert_eq!(new_pet(&data, "ad", &mut rng).unwrap().outfit, 3);
        assert!(new_pet(&data, "ah", &mut rng).is_none());
        assert_eq!(cat.traits.len(), 3);
    }
}
