//! The game's question dialogs in its own look. An opportunity offered is the game's
//! `OpportunityConfirmDialog`, as UI.dll's `OpportunityDialog` fills it: the Sim it's for (their
//! face), the opportunity's name and "for <Sim>", its description on the game's card, the
//! objective and the reward, "Would you like to accept this opportunity?", and the round OK and
//! cancel buttons. (The plain dialog stays underneath, out of sight, and its buttons' answers
//! are the ones these give.)

use bevy::prelude::*;

use crate::layout::UiAssets;

pub struct GameDialogsPlugin;

impl Plugin for GameDialogsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, opportunity_dialog.run_if(in_state(crate::PlayMode::Live)));
    }
}

// `OpportunityDialog`'s windows.
const TITLE: u32 = 1;
const OKAY: u32 = 2;
const CANCEL: u32 = 3;
const DESCRIPTION: u32 = 5;
const SIM_HEAD: u32 = 6;
const FOR_SIM: u32 = 7;
const STATIC_BG: u32 = 10;
const SOURCE_HEAD_BG: u32 = 12;
const AREA_WITHOUT_THUMB: u32 = 14;
const OBJECTIVE: u32 = 15;
const REWARD: u32 = 16;
const BUTTON_BUFFER: u32 = 17;
const LOCATION: u32 = 18;
const SCROLL: u32 = 20;
const REWARD_TITLE: u32 = 21;
/// "Would you like to accept this opportunity?"
const ACCEPT_PROMPT: u64 = 16_118_722_810_333_272_253;

/// The game's dialog up for the offer on show: the plain one's root, and the game's.
#[derive(Default)]
struct OfferShown(Option<(Entity, Entity)>);

#[allow(clippy::too_many_arguments)]
fn opportunity_dialog(
    mut commands: Commands,
    board: Res<crate::opportunities::OpportunityBoard>,
    mut shown: Local<OfferShown>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (game_ui, sims, mut portraits): (Option<Res<crate::icons::GameUi>>, Query<&crate::sim::Sim>, ResMut<crate::portraits::Portraits>),
) {
    let offer = board.offer();
    // (Answered: the game's dialog goes with the plain one.)
    if let Some((plain, mine)) = shown.0
        && offer.is_none_or(|o| o.0 != plain)
    {
        commands.entity(mine).try_despawn();
        shown.0 = None;
    }
    let Some((root, sim_e, index)) = offer else { return };
    if shown.0.is_some() {
        return;
    }
    let (Some(mut ui), Some(game_ui), Ok(sim)) = (ui, game_ui, sims.get(sim_e)) else { return };
    let Some(o) = game_ui.data.opportunities.get(index) else { return };
    let Some(mut dialog) = ui.layout("OpportunityConfirmDialog").cloned() else { return };
    // (Centred on the screen, as the game's modal dialogs are.)
    let (w, h) = (dialog.area[2] - dialog.area[0], dialog.area[3] - dialog.area[1]);
    dialog.area = [0.0, 0.0, w, h];
    dialog.place = s3bake::ui::UiPlace::Center(0.5);
    dialog.flags |= s3bake::ui::WIN_VISIBLE;
    // (No source Sim: the description on the plain card, full width.)
    if let Some(area) = dialog.find(AREA_WITHOUT_THUMB).map(|a| a.area) {
        set_area(&mut dialog, SCROLL, area);
    }
    // (A long description: its card grows to show it all, the rest of the dialog moving down.)
    let desc = crate::opportunities::fill(&o.desc, o, crate::opportunities::venue_word(&o.rabbit_hole)).replace("{0.SimFirstName}", &sim.first);
    let desc_extra = ((desc.chars().count() as f32 / 44.0).ceil() * 20.0 - 92.0).max(0.0);
    if desc_extra > 0.0 {
        for id in [SCROLL, STATIC_BG, DESCRIPTION] {
            grow(&mut dialog, id, desc_extra);
        }
        shift(&mut dialog, OBJECTIVE, desc_extra);
        dialog.area[3] += desc_extra;
    }
    // (Longer words take more lines: the objective's window, and so the dialog, grows to fit,
    // as `DeltaSize` grows them.)
    let objective_lines = (objective_text(&ui, o, sim).chars().count() as f32 / 50.0).ceil().max(1.0);
    let extra = (objective_lines - 1.0) * 16.0;
    if extra > 0.0 {
        grow(&mut dialog, OBJECTIVE, extra);
        dialog.area[3] += extra;
    }
    let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &dialog);
    let Some(d) = s.root else { return };
    // The plain dialog goes out of sight; the game's shows in its place.
    commands.entity(root).insert(Visibility::Hidden);
    commands.entity(d).insert((GlobalZIndex(21), crate::dialog::Modal, crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(crate::AppState::InGame)));
    shown.0 = Some((root, d));
    for id in [SOURCE_HEAD_BG, LOCATION, BUTTON_BUFFER] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    if let Some(e) = s.id(STATIC_BG) {
        commands.entity(e).insert(Visibility::Inherited);
    }
    // The words.
    let word = |key: &str, plain: &str| ui.localize(key).unwrap_or_else(|| plain.to_string());
    let place = crate::opportunities::venue_word(&o.rabbit_hole);
    let mut reward = Vec::new();
    if o.money > 0 {
        reward.push(format!("§{}", crate::lifetime::group(o.money)));
    }
    if o.performance > 0.0 {
        reward.push("a better job performance".to_string());
    }
    if o.skill_reward > 0.0 {
        reward.push(format!("{} skill", o.skill));
    }
    let texts = [
        (TITLE, o.name.clone()),
        (FOR_SIM, word("Ui/Caption/OpportunityConfirmDialog:ForSim", "for {0.String}").replace("{0.String}", &sim.full_name())),
        (DESCRIPTION, desc),
        (OBJECTIVE, objective_text(&ui, o, sim)),
        (REWARD_TITLE, if reward.is_empty() { String::new() } else { word("Ui/Caption/OpportunityConfirmDialog:RewardTitle", "Reward:") }),
        (REWARD, reward.join(", ")),
        (19, ui.localize_key(ACCEPT_PROMPT).unwrap_or_else(|| "Would you like to accept this opportunity?".into())),
    ];
    for (id, t) in texts {
        if let Some(e) = s.text(id) {
            commands.entity(e).insert(Text::new(t));
        }
    }
    // The Sim's face; OK and cancel answer as the plain dialog's buttons do.
    // (Behind the title, which the face's window also holds.)
    if let Some(head) = s.id(SIM_HEAD) {
        let pic = portraits.portrait(&mut images, sim_e);
        let face = commands.spawn((ImageNode::new(pic), crate::portraits::PortraitOf(sim_e), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE)).id();
        commands.entity(head).insert_children(0, &[face]);
    }
    for (id, accept) in [(OKAY, true), (CANCEL, false)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::opportunities::OppButton(accept));
        }
    }
}

/// An opportunity's objective, in the game's words ("Objective: ...").
fn objective_text(ui: &UiAssets, o: &s3bake::gamedata::OpportunityInfo, _sim: &crate::sim::Sim) -> String {
    let place = crate::opportunities::venue_word(&o.rabbit_hole);
    let objective = format!("{} at the {} between {} and {}", crate::opportunities::task_for(o).name, place, crate::interact::hour_label(o.open), crate::interact::hour_label(o.close));
    ui.localize("Ui/Caption/OpportunityConfirmDialog:ObjectiveText").unwrap_or_else(|| "Objective: {0.String}".into()).replace("{0.String}", &objective)
}

/// Makes a window taller (found by id anywhere under a layout window).
fn grow(w: &mut s3bake::ui::UiWindow, id: u32, by: f32) {
    if w.id == id {
        w.area[3] += by;
        return;
    }
    for c in &mut w.children {
        grow(c, id, by);
    }
}

/// Moves a window down (found by id anywhere under a layout window).
fn shift(w: &mut s3bake::ui::UiWindow, id: u32, by: f32) {
    if w.id == id {
        w.area[1] += by;
        w.area[3] += by;
        return;
    }
    for c in &mut w.children {
        shift(c, id, by);
    }
}

/// Sets a window's area (found by id anywhere under a layout window).
fn set_area(w: &mut s3bake::ui::UiWindow, id: u32, area: [f32; 4]) {
    if w.id == id {
        w.area = area;
        return;
    }
    for c in &mut w.children {
        set_area(c, id, area);
    }
}
