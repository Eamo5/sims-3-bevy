//! Create a Pet: the Pets pack's half of Create a Sim, in the game's own panels (UI.dll's
//! `CASPuck` pet menu, `CAPPetSheet`, `CAPBasics`, `CAPBreeds` and `CAPCharacter`). The puck's
//! add button has the pack's menu (a Sim, a horse, a dog or a cat, or genetics); a new pet joins
//! the household's skewer after its Sims (up to six, ten in all: `kMaxPetsPerHousehold`) and
//! stands in the dressing room in the Sim's place, with the pet sheet beside it: Basics (its
//! name, the household's, male or female; grown, as Create a Pet makes them), Breed (the game's
//! breeds list, `CAPPetBreeds`: the mixed breed first, then by name, each with the game's
//! picture of it; dogs big or little), and Character (its traits in their five sockets, the
//! game's traits picker for the pack's traits of its species, or a random set). Randomize draws
//! another pet of its species. The pets come home with the household.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::AppState;
use crate::cas::CasAction;
use crate::home::PendingHousehold;
use crate::layout::{SetIcon, Spawned, UiAssets, UiButton, UiScrollBar};
use crate::pets::SavedPet;

pub struct CapPlugin;

impl Plugin for CapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cap>()
            .add_message::<CapRequest>()
            .add_systems(Update, (requests, pet_stage, pet_sheet, basics_panel, breeds_panel, character_panel).chain().run_if(in_state(AppState::CreateHousehold)))
            .add_systems(Update, cap_test)
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
    let want = pet.map(|p| p.outfit);
    if cap.model.map(|m| m.1) != want {
        if let Some((e, _)) = cap.model.take() {
            commands.entity(e).try_despawn();
        }
        let (Some(p), Some(d), Some(b)) = (pet, data.0.as_deref(), baked) else { return };
        let Some(breed) = d.breeds.iter().position(|x| x.outfit == p.outfit) else { return };
        let mut ctx = crate::objects::AssetCtx { baked: &b.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
        if let Some(e) = crate::pets::spawn_pet(&mut commands, d, breed, (p.name.clone(), p.female), Vec3::ZERO, 0.0, &mut ctx, &mut assets, &mut bindposes) {
            let mut anim = crate::pets::PetAnim::default();
            anim.play(format!("{}_idle_stand_breathe_x", d.breeds[breed].kind));
            commands.entity(e).insert((anim, DespawnOnExit(AppState::CreateHousehold)));
            cap.model = Some((e, p.outfit));
        }
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
    if open != cap.panel {
        cap.panel = open;
        cap.typing = None;
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
    // (A coat of its own, its face's shapes and its collar aren't made here yet.)
    for id in [SHEET_COAT, SHEET_FACE, SHEET_ACCESSORIES] {
        set(within(id), false, true);
    }
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
                _ => None,
            };
            cap.test_picker = panel == "picker";
            info!("create a pet test: a {kind}, {panel}");
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
    fn a_new_pet_is_the_mixed_breed_of_its_kind() {
        let breed = |kind: &str, outfit: u64, listed: Option<u32>| s3bake::pets::PetBreed { outfit, kind: kind.into(), listed, ..Default::default() };
        let data = s3bake::PetsBaked { version: 0, kinds: Vec::new(), breeds: vec![breed("ac", 1, Some(3)), breed("ac", 2, Some(0)), breed("ad", 3, None), breed("ac", 4, None)] };
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
