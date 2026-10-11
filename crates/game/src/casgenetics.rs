//! Making a child of two of the household's Sims, in the game's own screen (UI.dll's
//! `CASGenetics`, from the puck's Add a Sim menu): the household's young adults and older as the
//! fathers and mothers to choose from (each their age's figure), the child's gender and age,
//! Create Baby (the parents' genes mixed: a skin tone between theirs, one's hair colour and one's
//! eyes, a face between their faces; again for another child), and Accept, the child joining the
//! household as the two Sims' child.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::color::Mix;
use bevy::prelude::*;
use rand::{Rng, SeedableRng};

use crate::AppState;
use crate::family::Tie;
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiButton};
use crate::sim::{Age, Sim};

pub struct CasGeneticsPlugin;

impl Plugin for CasGeneticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, genetics.run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<Genetics>();
            OPEN.store(false, Ordering::Relaxed);
        });
    }
}

/// Asked for by the puck's add menu (see `caslook`).
pub static OPEN: AtomicBool = AtomicBool::new(false);

/// For the UI flow (`autotest`): a child made and accepted.
pub static TEST_CHILD: AtomicBool = AtomicBool::new(false);

// `CASGenetics`'s windows.
const CLOSE: u32 = 0x05da_97ff;
const ACCEPT: u32 = 0x05da_4900;
const CANCEL: u32 = 0x05da_4901;
const CREATE_BABY: u32 = 0x05da_4902;
const FATHERS: u32 = 0x3100;
const MOTHERS: u32 = 0x3102;
const PET_PANELS: [u32; 2] = [0x3101, 0x3103];
const FATHER_BUTTONS: u32 = 0x2100;
const MOTHER_BUTTONS: u32 = 0x2200;
const BUTTONS: u32 = 6;
const SCENES: [u32; 3] = [0x1000, 0x1001, 0x1002];
const PICTURES: [u32; 3] = [0x1003, 0x1004, 0x1005];
const CHILD_COVER: u32 = 0x0c13_0e50;
const MALE: u32 = 0x05db_9703;
const FEMALE: u32 = 0x05db_9704;
const AGE_HUMAN: u32 = 0x0c60_dd60;
const AGE_PETS: [u32; 3] = [0x0c60_dd61, 0x0c60_dd62, 0x0c60_dd63];
/// The age buttons, toddler to elder.
const AGES: [(u32, Age); 6] = [(0x05db_9705, Age::Toddler), (0x05db_9706, Age::Child), (0x05db_9707, Age::Teen), (0x05db_9708, Age::YoungAdult), (0x05db_9709, Age::Adult), (0x05db_9710, Age::Elder)];
const MAX_SIMS: usize = 8;

/// The screen up: the parents chosen (by place in the household), the child's gender and age,
/// the child made (and the seed it was made from, to make it again at another age or gender).
#[derive(Resource)]
pub struct Genetics {
    s: Spawned,
    fathers: Vec<usize>,
    mothers: Vec<usize>,
    father: Option<usize>,
    mother: Option<usize>,
    female: bool,
    age: Age,
    seed: Option<u64>,
    child: Option<Sim>,
    shown: Option<(Option<usize>, Option<usize>, bool, Age, Option<u64>)>,
}

/// A parent's button (fathers' or mothers', by place in the household).
#[derive(Component)]
struct ParentButton(bool, usize);

/// Whether a household has the two parents a child needs.
pub fn possible(members: &[Sim]) -> bool {
    let grown = |s: &&Sim| matches!(s.age, Age::YoungAdult | Age::Adult | Age::Elder);
    members.iter().filter(grown).any(|s| !s.female) && members.iter().filter(grown).any(|s| s.female)
}

/// A child of two Sims (the same seed makes the same child, at any age or gender): a skin tone
/// between theirs, one's hair colour and one's eyes, a face between their faces, a build between
/// theirs, the father's surname.
pub fn make_child(father: &Sim, mother: &Sim, female: bool, age: Age, seed: u64) -> Sim {
    // (The genes drawn first and alike whatever the age and gender; the rest after.)
    let mut genes = rand::rngs::StdRng::seed_from_u64(seed);
    let t = genes.random_range(0.0f32..1.0);
    let skin = Color::Srgba(father.skin.to_srgba().mix(&mother.skin.to_srgba(), t));
    let hair = if genes.random_bool(0.5) { father.hair } else { mother.hair };
    let eyes = if genes.random_bool(0.5) { father.eyes } else { mother.eyes };
    let face = crate::simbody::inherited_face(mother, Some(father), &mut genes);
    let build = (genes.random_range(0.0f32..1.0), genes.random_range(-0.2f32..0.2), genes.random_range(0.5f32..1.0));
    let mut c = crate::sim::random_sim(&mut rand::rngs::StdRng::seed_from_u64(seed ^ 0x5EED_C41D), &father.last, Some(female), age);
    (c.skin, c.hair, c.eyes, c.face) = (skin, hair, eyes, face);
    if !age.is_little() {
        c.weight = (father.weight + (mother.weight - father.weight) * build.0 + build.1).clamp(-1.0, 1.0);
        c.fitness = ((father.fitness + mother.fitness) * 0.5 * build.2).clamp(0.0, 1.0);
    }
    c
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn genetics(
    mut commands: Commands,
    screen: Option<ResMut<Genetics>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pending: ResMut<PendingHousehold>,
    (clicks, parents): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &ParentButton), Changed<Interaction>>),
    mut buttons: Query<&mut UiButton>,
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let Some(mut g) = screen else {
        if OPEN.swap(false, Ordering::Relaxed)
            && let Some(g) = open(&mut commands, &mut ui, &mut images, &mut fonts, &pending.members)
        {
            commands.insert_resource(g);
        }
        return;
    };
    let g = &mut *g;
    let test = TEST_CHILD.swap(false, Ordering::Relaxed);
    let pressed = |id: u32| crate::livehud::pressed(&clicks, g.s.id(id));
    let close = |commands: &mut Commands, g: &Genetics| {
        if let Some(r) = g.s.root {
            commands.entity(r).despawn();
        }
        commands.remove_resource::<Genetics>();
    };
    if pressed(CANCEL) || pressed(CLOSE) {
        close(&mut commands, g);
        return;
    }
    // The parents, the child's gender and age.
    for (i, p) in &parents {
        if *i != Interaction::Pressed {
            continue;
        }
        if p.0 {
            g.mother = Some(p.1);
        } else {
            g.father = Some(p.1);
        }
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
    }
    if pressed(MALE) {
        g.female = false;
    }
    if pressed(FEMALE) {
        g.female = true;
    }
    for (id, age) in AGES {
        if pressed(id) {
            g.age = age;
        }
    }
    let (father, mother) = (g.father.and_then(|i| pending.members.get(i)).cloned(), g.mother.and_then(|i| pending.members.get(i)).cloned());
    // Create Baby: a child of theirs (another each time).
    if (pressed(CREATE_BABY) || test)
        && let (Some(_), Some(_)) = (&father, &mother)
    {
        g.seed = Some(rand::rng().random());
        play.write(crate::sound::PlaySound::ui("ui_cas_trait_add"));
    }
    // (Made again, the same child, when the age or gender changes; gone if the parents do.)
    if g.shown.is_some_and(|s| (s.0, s.1) != (g.father, g.mother)) {
        g.seed = None;
    }
    let want = (g.father, g.mother, g.female, g.age, g.seed);
    g.child = match (&father, &mother, g.seed) {
        (Some(f), Some(m), Some(seed)) => Some(make_child(f, m, g.female, g.age, seed)),
        _ => None,
    };
    let room = pending.members.len() < MAX_SIMS;
    // Accept: the child joins the household, the two Sims' child.
    if (pressed(ACCEPT) || test)
        && room
        && let (Some(child), Some(f), Some(m)) = (g.child.clone(), &father, &mother)
    {
        let id = child.id;
        pending.ties.push((f.id, id, Tie::ParentOf));
        pending.ties.push((m.id, id, Tie::ParentOf));
        // (Brothers and sisters of the parents' other children.)
        let siblings: Vec<u64> = pending
            .members
            .iter()
            .filter(|s| {
                let child_of = |p: u64| pending.ties.iter().any(|(a, b, t)| (*a, *b, *t) == (p, s.id, Tie::ParentOf) || (*a, *b, *t) == (s.id, p, Tie::ChildOf));
                child_of(f.id) && child_of(m.id)
            })
            .map(|s| s.id)
            .collect();
        for s in siblings {
            pending.ties.push((s, id, Tie::Siblings));
        }
        pending.members.push(child);
        actions.write(crate::cas::CasActionRequest(crate::cas::CasAction::Select(pending.members.len() - 1)));
        close(&mut commands, g);
        return;
    }
    // Shown: who's chosen, the child's gender and age, the child's picture once made.
    for (id, on) in [(MALE, !g.female), (FEMALE, g.female)].into_iter().chain(AGES.iter().map(|(id, a)| (*id, *a == g.age))) {
        if let Some(mut b) = g.s.id(id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != on
        {
            b.selected = on;
        }
    }
    if let Some(mut b) = g.s.id(ACCEPT).and_then(|e| buttons.get_mut(e).ok()) {
        let off = g.child.is_none() || !room;
        if b.disabled != off {
            b.disabled = off;
        }
    }
    if let Some(mut b) = g.s.id(CREATE_BABY).and_then(|e| buttons.get_mut(e).ok()) {
        let off = father.is_none() || mother.is_none();
        if b.disabled != off {
            b.disabled = off;
        }
    }
    if g.shown == Some(want) {
        return;
    }
    g.shown = Some(want);
    for (k, (list, chosen, base)) in [(&g.fathers, g.father, FATHER_BUTTONS), (&g.mothers, g.mother, MOTHER_BUTTONS)].into_iter().enumerate() {
        let panel = g.s.id(if k == 0 { FATHERS } else { MOTHERS });
        for (n, i) in list.iter().enumerate() {
            if let Some(e) = panel.and_then(|p| g.s.within(p, base + n as u32)) {
                let on = chosen == Some(*i);
                commands.entity(e).queue(move |mut w: EntityWorldMut| {
                    if let Some(mut b) = w.get_mut::<UiButton>() {
                        b.selected = on;
                    }
                });
            }
        }
    }
    let figure = |ui: &mut UiAssets, images: &mut Assets<Image>, s: &Sim| ui.image(images, s3pkg::fnv64(crate::caslook::AGE_ICONS[crate::caslook::age_icon(s.age) as usize])).map(|(h, _)| h);
    for (k, sim) in [father.as_ref(), mother.as_ref(), g.child.as_ref()].into_iter().enumerate() {
        let Some(win) = g.s.id(PICTURES[k]) else { continue };
        match sim.and_then(|s| figure(&mut ui, &mut images, s)) {
            Some(h) => {
                crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
                commands.entity(win).insert((Visibility::Inherited, crate::icons::Tooltip(sim.map(|s| s.full_name()).unwrap_or_default())));
            }
            None => {
                commands.entity(win).insert(Visibility::Hidden);
            }
        }
    }
    if let Some(e) = g.s.id(CHILD_COVER) {
        commands.entity(e).insert(if g.child.is_some() { Visibility::Hidden } else { Visibility::Inherited });
    }
}

/// The screen put up, with the household's would-be parents on their buttons.
fn open(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, members: &[Sim]) -> Option<Genetics> {
    let mut w = ui.layout("CASGenetics")?.clone();
    w.flags |= s3bake::ui::WIN_VISIBLE;
    let s = ui.spawn_root(commands, images, fonts, &w);
    let root = s.root?;
    commands.entity(root).insert((GlobalZIndex(20), crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(AppState::CreateHousehold)));
    // (People, not pets: the pets' panels and ages put away; the 3D previews are the figures.)
    for id in PET_PANELS.into_iter().chain(AGE_PETS).chain(SCENES) {
        for e in s.all_with(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for id in [FATHERS, MOTHERS, AGE_HUMAN] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Inherited);
        }
    }
    let grown = |s: &Sim| matches!(s.age, Age::YoungAdult | Age::Adult | Age::Elder);
    let fathers: Vec<usize> = members.iter().enumerate().filter(|(_, s)| grown(s) && !s.female).map(|(i, _)| i).take(BUTTONS as usize).collect();
    let mothers: Vec<usize> = members.iter().enumerate().filter(|(_, s)| grown(s) && s.female).map(|(i, _)| i).take(BUTTONS as usize).collect();
    for (k, list, panel, base) in [(false, &fathers, FATHERS, FATHER_BUTTONS), (true, &mothers, MOTHERS, MOTHER_BUTTONS)] {
        let panel = s.id(panel);
        for n in 0..BUTTONS {
            let Some(e) = panel.and_then(|p| s.within(p, base + n)) else { continue };
            match list.get(n as usize).and_then(|i| Some((*i, members.get(*i)?))) {
                Some((i, sim)) => {
                    commands.entity(e).insert((ParentButton(k, i), crate::icons::Tooltip(sim.full_name())));
                    // (Their age's figure, in the picture window inside the button.)
                    if let Some((h, _)) = ui.image(images, s3pkg::fnv64(crate::caslook::AGE_ICONS[crate::caslook::age_icon(sim.age) as usize])) {
                        let inner = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(4.0), top: Val::Px(4.0), right: Val::Px(4.0), bottom: Val::Px(4.0), ..default() }, Pickable::IGNORE, ChildOf(e))).id();
                        crate::hudpanels::picture(commands, inner, h, Color::WHITE);
                    }
                }
                None => {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
        }
    }
    for (id, tip) in [(ACCEPT, "Accept"), (CANCEL, "Cancel"), (CLOSE, "Close"), (CREATE_BABY, "Create Baby"), (MALE, "Male"), (FEMALE, "Female")]
        .into_iter()
        .chain([(AGES[0].0, "Toddler"), (AGES[1].0, "Child"), (AGES[2].0, "Teen"), (AGES[3].0, "Young Adult"), (AGES[4].0, "Adult"), (AGES[5].0, "Elder")])
    {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    let (father, mother) = (fathers.first().copied(), mothers.first().copied());
    Some(Genetics { s, fathers, mothers, father, mother, female: rand::rng().random_bool(0.5), age: Age::Toddler, seed: None, child: None, shown: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn children_take_after_their_parents() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let mut f = crate::sim::random_sim(&mut rng, "Goth", Some(false), Age::Adult);
        let mut m = crate::sim::random_sim(&mut rng, "Bachelor", Some(true), Age::Adult);
        f.skin = Color::srgb(0.9, 0.8, 0.7);
        m.skin = Color::srgb(0.5, 0.4, 0.3);
        for seed in 0..20 {
            let c = make_child(&f, &m, true, Age::Child, seed);
            assert_eq!(c.last, "Goth");
            assert!(c.female && c.age == Age::Child);
            let s = c.skin.to_srgba();
            assert!((0.49..=0.91).contains(&s.red), "skin between the parents': {s:?}");
            assert!(c.hair == f.hair || c.hair == m.hair);
            assert!(c.eyes == f.eyes || c.eyes == m.eyes);
        }
        // (The same seed, the same child, at another age.)
        let a = make_child(&f, &m, false, Age::Teen, 7);
        let b = make_child(&f, &m, false, Age::Adult, 7);
        assert_eq!((a.skin, a.hair, a.eyes), (b.skin, b.hair, b.eyes));
        assert!(possible(&[f, m]));
    }
}
