//! What the household's Sims are to each other, in the game's own screen (UI.dll's
//! `CASFamilyScreen`), put up when a household of more than one is accepted: the Sims as a family
//! tree (each generation a row, couples side by side, a line down from parents to their
//! children, partners' lines in their own colour), a click on one Sim and then another asking
//! what the first is to the second (`Add Relationship Dialog`: housemates, or parent, child,
//! sibling, spouse or partner, as their ages allow), and accept (the household moves in) or
//! cancel (back to Create a Sim).

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::AppState;
use crate::cas::CasAction;
use crate::family::Tie;
use crate::home::PendingHousehold;
use crate::layout::{Spawned, UiAssets, UiButton, edit_windows};

pub struct CasFamilyPlugin;

impl Plugin for CasFamilyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, family_screen.run_if(in_state(AppState::CreateHousehold))).add_systems(OnExit(AppState::CreateHousehold), |mut commands: Commands| {
            commands.remove_resource::<FamilyScreen>();
            OPEN.store(false, Ordering::Relaxed);
        });
    }
}

/// Asked for by the puck's Accept (see `caslook`).
pub static OPEN: AtomicBool = AtomicBool::new(false);

/// For the UI flow (`autotest`): the dialog asked for the first two Sims.
pub static TEST_DIALOG: AtomicBool = AtomicBool::new(false);

// `CASFamilyScreen`'s windows.
const ACCEPT: u32 = 0x05da_4900;
const CANCEL: u32 = 0x05da_4901;
const TREE: u32 = 0x05da_4904;
const INSTRUCTIONS: u32 = 0x05da_4905;
const DIALOG: u32 = 0x0639_6900;
/// The dialog's choices: housemates, then up to four others.
const CHOICES: [u32; 5] = [0x0639_6901, 0x0639_6902, 0x0639_6903, 0x0639_6908, 0x0639_6909];
const DIALOG_ACCEPT: u32 = 0x0639_6904;
const DIALOG_CANCEL: u32 = 0x0639_6905;
const DIALOG_SIM1: u32 = 0x0639_6906;
const DIALOG_SIM2: u32 = 0x0639_6907;
const THUMB: u32 = 1;
/// The lines' colours (`DEFAULT_RELATIONSHIP_COLOR`, `BGFRIEND_COLOR`).
const LINE: u32 = 0xff36_7cdd;
const PARTNER_LINE: u32 = 0xffdb_4dc5;
const SIM: f32 = 50.0;

/// The screen up: the Sim chosen first, the dialog's two Sims and choices (with the one lit),
/// the tree drawn, and what it was drawn for.
#[derive(Resource)]
pub struct FamilyScreen {
    s: Spawned,
    from: Option<usize>,
    dialog: Option<(usize, usize, Vec<Tie>, usize)>,
    tree: Option<Entity>,
    shown: Option<(Vec<(u64, crate::sim::Age)>, Vec<(u64, u64, Tie)>, Option<usize>)>,
}

/// A Sim in the tree (by their place in the household).
#[derive(Component)]
struct TreeSim(usize);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn family_screen(
    mut commands: Commands,
    screen: Option<ResMut<FamilyScreen>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    pending: Res<PendingHousehold>,
    (clicks, sims, hovered, mouse): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &TreeSim), Changed<Interaction>>, Query<(&Interaction, &TreeSim)>, Res<ButtonInput<MouseButton>>),
    (mut vis, mut buttons, mut texts): (Query<&mut Visibility>, Query<&mut UiButton>, Query<&mut Text>),
    mut actions: MessageWriter<crate::cas::CasActionRequest>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let Some(mut f) = screen else {
        if OPEN.swap(false, Ordering::Relaxed) {
            open(&mut commands, &mut ui, &mut images, &mut fonts);
        }
        return;
    };
    let f = &mut *f;
    let pressed = |id: u32| crate::livehud::pressed(&clicks, f.s.id(id));
    let close = |commands: &mut Commands, f: &FamilyScreen| {
        if let Some(r) = f.s.root {
            commands.entity(r).despawn();
        }
        commands.remove_resource::<FamilyScreen>();
    };
    // Accepted: the household moves in; cancelled: back to making them.
    if pressed(ACCEPT) {
        actions.write(crate::cas::CasActionRequest(CasAction::Done));
        close(&mut commands, f);
        return;
    }
    if pressed(CANCEL) {
        close(&mut commands, f);
        return;
    }
    let tie_of = |a: u64, b: u64| {
        pending.ties.iter().find_map(|(x, y, t)| {
            if (*x, *y) == (a, b) {
                Some(*t)
            } else if (*x, *y) == (b, a) {
                Some(match t {
                    Tie::ParentOf => Tie::ChildOf,
                    Tie::ChildOf => Tie::ParentOf,
                    t => *t,
                })
            } else {
                None
            }
        })
    };
    // The dialog: a choice lit; accepted, what the first is to the second set.
    if let Some((a, b, choices, lit)) = f.dialog.as_mut() {
        for (n, id) in CHOICES.iter().enumerate() {
            if crate::livehud::pressed(&clicks, f.s.id(*id)) && n < choices.len() {
                *lit = n;
            }
        }
        for (n, id) in CHOICES.iter().enumerate() {
            if let Some(mut bt) = f.s.id(*id).and_then(|e| buttons.get_mut(e).ok())
                && bt.selected != (n == *lit)
            {
                bt.selected = n == *lit;
            }
        }
        let (a, b, tie) = (*a, *b, choices[*lit]);
        if crate::livehud::pressed(&clicks, f.s.id(DIALOG_ACCEPT)) {
            actions.write(crate::cas::CasActionRequest(CasAction::SetTie(a, b, Tie::ALL.iter().position(|t| *t == tie).unwrap_or(0) as u8)));
            f.dialog = None;
        } else if crate::livehud::pressed(&clicks, f.s.id(DIALOG_CANCEL)) {
            f.dialog = None;
        }
        crate::livehud::set_visible(&mut vis, f.s.id(DIALOG), f.dialog.is_some());
    } else {
        // A Sim dragged onto another (or clicked, then another): what's the first to the second?
        let mut asked: Vec<usize> = sims.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, t)| t.0).collect();
        if mouse.just_released(MouseButton::Left)
            && let Some(a) = f.from
            && let Some((_, t)) = hovered.iter().find(|(i, t)| **i == Interaction::Hovered && t.0 != a)
        {
            asked.push(t.0);
        }
        if TEST_DIALOG.swap(false, Ordering::Relaxed) && pending.members.len() > 1 {
            f.from = Some(0);
            asked.push(1);
        }
        for t in asked {
            let t = TreeSim(t);
            match f.from {
                Some(a) if a != t.0 => {
                    let (Some(sa), Some(sb)) = (pending.members.get(a), pending.members.get(t.0)) else { continue };
                    let mut choices = vec![Tie::Roommates];
                    if Tie::ParentOf.fits(sa.age, sb.age) {
                        choices.push(Tie::ParentOf);
                    } else if Tie::ChildOf.fits(sa.age, sb.age) {
                        choices.push(Tie::ChildOf);
                    } else {
                        choices.push(Tie::Siblings);
                        for t in [Tie::Spouses, Tie::Partners] {
                            if t.fits(sa.age, sb.age) {
                                choices.push(t);
                            }
                        }
                    }
                    let now = tie_of(sa.id, sb.id).unwrap_or(Tie::Roommates);
                    let lit = choices.iter().position(|c| *c == now).unwrap_or(0);
                    show_dialog(&mut commands, &mut ui, &mut images, &f.s, (sa, sb), &choices, &mut texts, &mut vis);
                    f.dialog = Some((a, t.0, choices, lit));
                    f.from = None;
                    play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
                }
                // (A press on the one chosen: let go of it, unless it's being dragged.)
                Some(_) if !mouse.pressed(MouseButton::Left) => f.from = None,
                Some(_) => {}
                None => f.from = Some(t.0),
            }
        }
    }
    // The tree, drawn again when the household, their ties or the Sim chosen change.
    let want = (pending.members.iter().map(|m| (m.id, m.age)).collect::<Vec<_>>(), pending.ties.clone(), f.from);
    if f.shown.as_ref() == Some(&want) {
        return;
    }
    f.shown = Some(want);
    let Some(tree) = f.s.id(TREE) else { return };
    if let Some(old) = f.tree.take() {
        commands.entity(old).despawn();
    }
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(tree))).id();
    f.tree = Some(holder);
    let members = &pending.members;
    let n = members.len();
    // Each Sim's generation: below their parents in the household.
    let mut generation = vec![0usize; n];
    for _ in 0..4 {
        for (i, m) in members.iter().enumerate() {
            for (j, p) in members.iter().enumerate() {
                if tie_of(p.id, m.id) == Some(Tie::ParentOf) {
                    generation[i] = generation[i].max(generation[j] + 1);
                }
            }
        }
    }
    let rows = generation.iter().copied().max().unwrap_or(0) + 1;
    // Each row in order, couples side by side.
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); rows];
    for i in 0..n {
        if order[generation[i]].contains(&i) {
            continue;
        }
        order[generation[i]].push(i);
        for j in 0..n {
            if j != i && generation[j] == generation[i] && !order[generation[i]].contains(&j) && matches!(tie_of(members[i].id, members[j].id), Some(Tie::Spouses | Tie::Partners)) {
                order[generation[i]].push(j);
            }
        }
    }
    let size = ui.find("CASFamilyScreen", TREE).map_or(Vec2::new(432.0, 265.0), |w| Vec2::new(w.area[2] - w.area[0], w.area[3] - w.area[1]));
    let gap = 34.0;
    let row_step = (size.y / rows as f32).min(110.0);
    let mut at = vec![Vec2::ZERO; n];
    for (r, row) in order.iter().enumerate() {
        let width = row.len() as f32 * SIM + (row.len().saturating_sub(1)) as f32 * gap;
        let x0 = (size.x - width) * 0.5;
        for (k, &i) in row.iter().enumerate() {
            at[i] = Vec2::new(x0 + k as f32 * (SIM + gap), 10.0 + r as f32 * row_step);
        }
    }
    // The lines: couples' across, parents' down to their children.
    let line = |commands: &mut Commands, a: Vec2, b: Vec2, colour: u32| {
        let (lo, hi) = (a.min(b), a.max(b));
        commands.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(lo.x - 1.5), top: Val::Px(lo.y - 1.5), width: Val::Px(hi.x - lo.x + 3.0), height: Val::Px(hi.y - lo.y + 3.0), border_radius: BorderRadius::all(Val::Px(1.5)), ..default() },
            BackgroundColor(crate::layout::color(colour)),
            Pickable::IGNORE,
            ChildOf(holder),
        ));
    };
    let mid = |i: usize| at[i] + Vec2::splat(SIM * 0.5);
    for i in 0..n {
        for j in (i + 1)..n {
            match tie_of(members[i].id, members[j].id) {
                Some(t @ (Tie::Spouses | Tie::Partners | Tie::Siblings)) if generation[i] == generation[j] => {
                    // (Siblings' a little lower, beneath couples'.)
                    let dy = if t == Tie::Siblings { SIM * 0.5 + 8.0 } else { 0.0 };
                    let (a, b) = (mid(i) + Vec2::Y * dy, mid(j) + Vec2::Y * dy);
                    if t == Tie::Siblings {
                        line(&mut commands, mid(i), a, LINE);
                        line(&mut commands, mid(j), b, LINE);
                    }
                    line(&mut commands, a, b, if t == Tie::Partners { PARTNER_LINE } else { LINE });
                }
                Some(Tie::ParentOf | Tie::ChildOf) => {
                    let (p, c) = if tie_of(members[i].id, members[j].id) == Some(Tie::ParentOf) { (i, j) } else { (j, i) };
                    let (top, bottom) = (mid(p) + Vec2::Y * SIM * 0.5, mid(c) - Vec2::Y * SIM * 0.5);
                    let y = (top.y + bottom.y) * 0.5;
                    line(&mut commands, top, Vec2::new(top.x, y), LINE);
                    line(&mut commands, Vec2::new(top.x, y), Vec2::new(bottom.x, y), LINE);
                    line(&mut commands, Vec2::new(bottom.x, y), bottom, LINE);
                }
                _ => {}
            }
        }
    }
    // The Sims: the game's frame each, their age's figure in it, their name beneath.
    let Some(template) = ui.find("CASFamilyScreen", DIALOG_SIM1).cloned() else { return };
    for (i, m) in members.iter().enumerate() {
        let mut w = template.clone();
        w.id = 0;
        w.cls = "Button".into();
        w.area = [at[i].x, at[i].y, at[i].x + SIM, at[i].y + SIM];
        let s = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, holder);
        let Some(e) = s.root else { continue };
        commands.entity(e).insert((TreeSim(i), crate::icons::Tooltip(m.full_name())));
        if f.from == Some(i) {
            commands.entity(e).queue(|mut e: EntityWorldMut| {
                if let Some(mut b) = e.get_mut::<UiButton>() {
                    b.selected = true;
                }
            });
        }
        if let (Some(win), Some((h, _))) = (s.id(THUMB), ui.image(&mut images, s3pkg::fnv64(crate::caslook::AGE_ICONS[crate::caslook::age_icon(m.age) as usize]))) {
            crate::hudpanels::picture(&mut commands, win, h, crate::layout::color(0xff16_3996));
        }
        let (font, lh) = ui.text_font(&mut fonts, 0);
        commands.spawn((
            Text::new(m.first.clone()),
            TextFont { font_size: bevy::text::FontSize::Px(11.0), ..font },
            lh,
            TextColor(crate::layout::color(0xff16_3996)),
            TextLayout::new(Justify::Center, LineBreak::NoWrap),
            Node { position_type: PositionType::Absolute, left: Val::Px(at[i].x - 20.0), top: Val::Px(at[i].y + SIM + 1.0), width: Val::Px(SIM + 40.0), justify_content: JustifyContent::Center, ..default() },
            Pickable::IGNORE,
            ChildOf(holder),
        ));
    }
}

/// The screen put up (its relationship dialog hidden till asked), with its words.
fn open(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>) {
    let Some(mut w) = ui.layout("CASFamilyScreen").cloned() else { return };
    // (The dialog's choices' words set as they're asked; housemates always.)
    let housemates = ui.localize("Ui/Caption/CAF/AddRelationship:Housemates").unwrap_or_else(|| "Housemates".into());
    edit_windows(&mut w, CHOICES[0], &mut |b| {
        for c in &mut b.children {
            if c.id == 1 {
                c.caption = housemates.clone();
            }
        }
    });
    let s = ui.spawn_root(commands, images, fonts, &w);
    let Some(root) = s.root else { return };
    commands.entity(root).insert((GlobalZIndex(20), crate::hud::BlocksWorld, DespawnOnExit(AppState::CreateHousehold)));
    let instructions = ui.localize("Ui/Caption/CAF/InstructionsText:Sims").unwrap_or_else(|| "Click a Sim, then another, to say what they are to each other".into());
    if let Some(t) = s.text(INSTRUCTIONS) {
        commands.entity(t).insert(Text::new(format!("({instructions})")));
    }
    if let Some(t) = s.comment("Sims and Relationships").and_then(|e| s.text_of(e)) {
        commands.entity(t).insert(Text::new(ui.localize("Ui/Caption/CAF/Misc:SimsAndRelationships").unwrap_or_else(|| "Sims and Relationships".into())));
    }
    for (id, tip) in [(ACCEPT, "Accept"), (CANCEL, "Cancel"), (DIALOG_ACCEPT, "Accept"), (DIALOG_CANCEL, "Cancel")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    commands.insert_resource(FamilyScreen { s, from: None, dialog: None, tree: None, shown: None });
}

/// The relationship dialog for two Sims: their figures, and the choices their ages allow.
#[allow(clippy::too_many_arguments)]
fn show_dialog(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, s: &Spawned, (a, b): (&crate::sim::Sim, &crate::sim::Sim), choices: &[Tie], texts: &mut Query<&mut Text>, vis: &mut Query<&mut Visibility>) {
    for (id, sim) in [(DIALOG_SIM1, a), (DIALOG_SIM2, b)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(sim.full_name()));
            if let (Some(win), Some((h, _))) = (s.within(e, THUMB), ui.image(images, s3pkg::fnv64(crate::caslook::AGE_ICONS[crate::caslook::age_icon(sim.age) as usize]))) {
                crate::hudpanels::picture(commands, win, h, crate::layout::color(0xff16_3996));
            }
        }
    }
    for (n, id) in CHOICES.iter().enumerate() {
        let Some(e) = s.id(*id) else { continue };
        crate::livehud::set_visible(vis, Some(e), n < choices.len());
        if n == 0 || n >= choices.len() {
            continue;
        }
        let (key, plain) = match choices[n] {
            Tie::ParentOf => ("Parent", "Parent"),
            Tie::ChildOf => ("Child", "Child"),
            Tie::Siblings => ("Sibling", "Sibling"),
            Tie::Spouses => ("Spouse", "Spouse"),
            Tie::Partners => ("BFriend", if a.female { "Girlfriend" } else { "Boyfriend" }),
            Tie::Roommates => ("Housemates", "Housemates"),
        };
        let word = ui.localize(&format!("Ui/Caption/CAF/AddRelationship:{key}")).map_or_else(|| plain.to_string(), |w| crate::layout::gendered(&w, a.female));
        crate::livehud::set_text(texts, s.within(e, 1).and_then(|w| s.text_of(w)).or_else(|| s.within(e, 1)), &word);
    }
    crate::livehud::set_visible(vis, s.id(DIALOG), true);
}
