//! The game's question dialogs in its own look. An opportunity offered is the game's
//! `OpportunityConfirmDialog`, as UI.dll's `OpportunityDialog` fills it: the Sim it's for (their
//! face), the opportunity's name and "for <Sim>", its description on the game's card, the
//! objective and the reward, "Would you like to accept this opportunity?", and the round OK and
//! cancel buttons. Choosing a lifetime wish is the game's `LifetimeWishSelectionDialog`, and the
//! lifetime rewards its shop (`HUDRewardTraitsShopDialog`). (The plain dialogs stay underneath,
//! out of sight, and their buttons' answers are the ones these give.)

use bevy::prelude::*;

use crate::layout::UiAssets;

pub struct GameDialogsPlugin;

impl Plugin for GameDialogsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (opportunity_dialog, question_dialogs).run_if(in_state(crate::PlayMode::Live)));
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

/// A layout's dialog put up on its own, in the middle of the screen (as `ModalDialog` centres
/// its window), above the game.
fn spawn_dialog(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, name: &str) -> Option<crate::layout::Spawned> {
    let mut dialog = ui.layout(name)?.clone();
    let (w, h) = (dialog.area[2] - dialog.area[0], dialog.area[3] - dialog.area[1]);
    dialog.area = [0.0, 0.0, w, h];
    dialog.place = s3bake::ui::UiPlace::Center(0.5);
    dialog.flags |= s3bake::ui::WIN_VISIBLE;
    let s = ui.spawn_root(commands, images, fonts, &dialog);
    let d = s.root?;
    commands.entity(d).insert((GlobalZIndex(21), crate::dialog::Modal, crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(crate::AppState::InGame)));
    Some(s)
}

/// The question shown in the game's own dialog: the plain one's root, the game's, and what's
/// chosen in it.
#[derive(Default)]
struct QuestionShown {
    plain: Option<Entity>,
    dialog: Option<crate::layout::Spawned>,
    chosen: Option<usize>,
    scroll: usize,
    rows: Option<Entity>,
    dirty: bool,
}

// `LifetimeWishSelectionDialog`'s windows.
const WISH_HEADER: u32 = 1;
const WISH_NAME: u32 = 2;
const WISH_DESCRIPTION: u32 = 3;
const WISH_REQUIRED: u32 = 4;
const WISH_SLOT: u32 = 0x10;
const WISH_CUSTOM: u32 = 0x1a;
const WISH_OKAY: u32 = 0x20;
const WISH_CANCEL: u32 = 0x21;
const WISH_OK_CANCEL_BG: u32 = 0x30;
const WISH_OK_ONLY_BG: u32 = 0x31;
// `HUDRewardTraitsShopDialog`'s.
const SHOP_FILTERS: u32 = 0x03e4_e827;
const SHOP_TABLE: u32 = 0x03e4_e810;
const SHOP_FUNDS: u32 = 0x03e4_e825;
const SHOP_PURCHASE: u32 = 0x03e4_e822;
const SHOP_CLOSE: u32 = 0x03e4_e800;
/// The shop's table: rows visible, their height, its header's, the columns' widths, the other
/// rows' colour, the words' colour, a row out of reach's shade (`RewardTraitStoreInventory...`).
const SHOP_ROWS: usize = 8;
const SHOP_ROW_HEIGHT: f32 = 40.0;
const SHOP_HEADER: f32 = 29.0;
const SHOP_COLUMNS: [f32; 2] = [292.0, 100.0];
const SHOP_ALTERNATE: u32 = 0xffd2_ebf7;
const SHOP_TEXT: u32 = 0xff00_3263;
const SHOP_SELECTED: u32 = 0xff9c_c8f0;

/// A row of the shop's table, a slot of the wish chooser (the answer it stands for).
#[derive(Component)]
struct ChoiceRow(usize);

/// The lifetime wish and rewards questions in the game's own dialogs: `LifetimeWishSelectionDialog`
/// (the wishes' icons along its slots, the chosen one's name and what it asks, OK) and
/// `HUDRewardTraitsShopDialog` (the rewards in the game's table with their icons and costs, out
/// of reach ones shaded, the points to spend, Purchase and Close).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn question_dialogs(
    mut commands: Commands,
    questions: Res<crate::dialog::Questions>,
    mut shown: Local<QuestionShown>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (mut game_ui, sims): (Option<ResMut<crate::icons::GameUi>>, Query<(&crate::sim::Sim, Option<&crate::wishes::Wishes>)>),
    (rows, hovered): (Query<(&Interaction, &ChoiceRow), Changed<Interaction>>, Query<&Interaction>),
    (mut buttons, mut wheel): (Query<&mut crate::layout::UiButton>, MessageReader<bevy::input::mouse::MouseWheel>),
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let plain = questions.shown();
    // (Answered: the game's dialog goes with the plain one.)
    if shown.plain.is_some() && shown.plain != plain {
        if let Some(r) = shown.dialog.take().and_then(|d| d.root) {
            commands.entity(r).try_despawn();
        }
        *shown = QuestionShown::default();
    }
    let (Some(plain), Some(ask)) = (plain, questions.queue.front()) else { return };
    let (Some(mut ui), Some(gu)) = (ui, game_ui.as_deref_mut()) else { return };
    let data = gu.data.clone();
    // Put it up.
    if shown.plain.is_none() {
        let name = match &ask.about {
            crate::dialog::Question::LifetimeWish { .. } => "LifetimeWishSelectionDialog",
            crate::dialog::Question::Reward { .. } => "HUDRewardTraitsShopDialog",
            _ => return,
        };
        let Some(s) = spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, name) else { return };
        commands.entity(plain).insert(Visibility::Hidden);
        shown.plain = Some(plain);
        shown.dirty = true;
        match &ask.about {
            crate::dialog::Question::LifetimeWish { sim, .. } => {
                let who = sims.get(*sim).map(|(s, _)| s.full_name()).unwrap_or_default();
                if let Some(t) = s.text(WISH_HEADER) {
                    let h = ui.localize("Ui/Caption/LifetimeWishSelectionDialog:Header").unwrap_or_else(|| "Select a Lifetime Wish for {0.String}".into());
                    commands.entity(t).insert(Text::new(h.replace("{0.String}", &who)));
                }
                // (A wish must be chosen: no cancelling, the OK-only background.)
                for (id, on) in [(WISH_CANCEL, false), (WISH_OK_CANCEL_BG, false), (WISH_OK_ONLY_BG, true)] {
                    if let Some(e) = s.id(id) {
                        commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
                    }
                }
                for e in s.all_with(WISH_CUSTOM) {
                    commands.entity(e).insert(Visibility::Hidden);
                }
                for (k, a) in ask.answers.iter().enumerate().take(6) {
                    if let Some(b) = s.id(WISH_SLOT + k as u32) {
                        commands.entity(b).insert((Visibility::Inherited, ChoiceRow(k), crate::icons::Tooltip(a.label.clone())));
                        if let Some(icon) = gu.icon(&mut images, &a.icon) {
                            commands.entity(b).insert(crate::layout::SetIcon(icon));
                        }
                    }
                }
            }
            crate::dialog::Question::Reward { sim, rewards } => {
                let points = sims.get(*sim).ok().and_then(|(_, w)| w).map_or(0, |w| w.points);
                if let Some(t) = s.text(SHOP_FUNDS) {
                    let f = ui.localize("Ui/Caption/HUD/RewardTraitsShopDialog:Available").unwrap_or_else(|| "Available Points: {0.Number}".into());
                    commands.entity(t).insert(Text::new(f.replace("{0.Number}", &crate::lifetime::group(points as i64))));
                }
                if let Some(e) = s.id(SHOP_CLOSE) {
                    commands.entity(e).insert(crate::dialog::AnswerButton(rewards.len()));
                }
                if let Some(e) = s.id(SHOP_FILTERS) {
                    let all = ui.localize("Gameplay/Excel/traits/LifetimeRewardCategories:All").unwrap_or_else(|| "All".into());
                    commands.entity(e).queue(move |mut w: EntityWorldMut| {
                        if let Some(mut c) = w.get_mut::<crate::layout::UiCombo>() {
                            c.items = vec![all];
                        }
                    });
                }
                // The table's header: the game's column buttons.
                if let (Some(table), Some(col)) = (s.id(SHOP_TABLE), ui.layout("ColumnControl").cloned()) {
                    let mut x = 0.0;
                    for (k, (key, plain_word)) in [("Ui/Caption/HUD/RewardTraitsShopDialog:TraitHeading", "Reward"), ("Ui/Caption/HUD/RewardTraitsShopDialog:PointHeading", "Points Cost")].iter().enumerate() {
                        let mut w = col.clone();
                        w.place = s3bake::ui::UiPlace::Fixed;
                        w.area = [x, 0.0, x + SHOP_COLUMNS[k], SHOP_HEADER];
                        w.caption = ui.localize(key).unwrap_or_else(|| plain_word.to_string());
                        ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, table);
                        x += SHOP_COLUMNS[k];
                    }
                }
            }
            _ => {}
        }
        shown.dialog = Some(s);
    }
    let Some(s) = shown.dialog.clone() else { return };
    // A choice: a wish's slot, or a reward's row.
    for (i, r) in &rows {
        if *i == Interaction::Pressed && shown.chosen != Some(r.0) {
            shown.chosen = Some(r.0);
            shown.dirty = true;
        }
    }
    match &ask.about {
        crate::dialog::Question::LifetimeWish { .. } => {
            if !shown.dirty {
                return;
            }
            shown.dirty = false;
            for k in 0..ask.answers.len().min(6) {
                if let Some(e) = s.id(WISH_SLOT + k as u32)
                    && let Ok(mut b) = buttons.get_mut(e)
                {
                    b.selected = shown.chosen == Some(k);
                }
            }
            let a = shown.chosen.and_then(|k| ask.answers.get(k));
            let mut text = |id: u32, t: &str| {
                if let Some(e) = s.text(id) {
                    commands.entity(e).insert(Text::new(t.to_string()));
                }
            };
            text(WISH_NAME, a.map_or("", |a| a.label.as_str()));
            text(WISH_DESCRIPTION, a.map_or(ask.text.as_str(), |a| a.detail.as_str()));
            text(WISH_REQUIRED, "");
            // OK takes the wish chosen (none yet: greyed).
            if let Some(e) = s.id(WISH_OKAY) {
                match shown.chosen {
                    Some(k) => {
                        commands.entity(e).insert(crate::dialog::AnswerButton(k));
                    }
                    None => {
                        commands.entity(e).remove::<crate::dialog::AnswerButton>();
                    }
                }
                if let Ok(mut b) = buttons.get_mut(e) {
                    b.disabled = shown.chosen.is_none();
                }
            }
        }
        crate::dialog::Question::Reward { sim, rewards } => {
            let points = sims.get(*sim).ok().and_then(|(_, w)| w).map_or(0, |w| w.points);
            // (The rewards cheapest first.)
            let mut list: Vec<(usize, &s3bake::gamedata::TraitInfo)> = rewards.iter().enumerate().filter_map(|(k, r)| Some((k, data.traits.iter().find(|t| t.hex == *r)?))).collect();
            list.sort_by_key(|(_, t)| (t.points, t.name.clone()));
            let over = s.id(SHOP_TABLE).and_then(|t| hovered.get(t).ok()).is_some_and(|i| *i != Interaction::None);
            if over && scrolled != 0.0 {
                let most = list.len().saturating_sub(SHOP_ROWS);
                shown.scroll = if scrolled > 0.0 { shown.scroll.saturating_sub(1) } else { (shown.scroll + 1).min(most) };
                shown.dirty = true;
            }
            // Purchase buys the reward chosen (if it's within reach).
            if let Some(e) = s.id(SHOP_PURCHASE) {
                let ok = shown.chosen.and_then(|k| list.iter().find(|(i, _)| *i == k)).is_some_and(|(_, t)| t.points <= points);
                if let Ok(mut b) = buttons.get_mut(e)
                    && b.disabled == ok
                {
                    b.disabled = !ok;
                }
            }
            if !shown.dirty {
                return;
            }
            shown.dirty = false;
            if let Some(e) = s.id(SHOP_PURCHASE) {
                let ok = shown.chosen.and_then(|k| list.iter().find(|(i, _)| *i == k)).is_some_and(|(_, t)| t.points <= points);
                match shown.chosen.filter(|_| ok) {
                    Some(k) => {
                        commands.entity(e).insert(crate::dialog::AnswerButton(k));
                    }
                    None => {
                        commands.entity(e).remove::<crate::dialog::AnswerButton>();
                    }
                }
            }
            let Some(table) = s.id(SHOP_TABLE) else { return };
            if let Some(r) = shown.rows.take() {
                commands.entity(r).try_despawn();
            }
            let (font, line) = ui.text_font(&mut fonts, 0);
            let holder = commands
                .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(SHOP_HEADER), height: Val::Px(SHOP_ROWS as f32 * SHOP_ROW_HEIGHT), flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() }, Pickable::IGNORE, ChildOf(table)))
                .id();
            for (n, (k, t)) in list.iter().enumerate().skip(shown.scroll).take(SHOP_ROWS) {
                let within = t.points <= points;
                let bg = if shown.chosen == Some(*k) { crate::layout::color(SHOP_SELECTED) } else if n % 2 == 1 { crate::layout::color(SHOP_ALTERNATE) } else { Color::NONE };
                let fade = if within { 1.0 } else { 0.6 };
                let row = commands
                    .spawn((
                        Node { height: Val::Px(SHOP_ROW_HEIGHT), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() },
                        Button,
                        BackgroundColor(bg),
                        ChoiceRow(*k),
                        crate::icons::Tooltip(format!("{}\n{}", t.name, t.desc.replace("{0.SimFirstName}", &sims.get(*sim).map(|(s, _)| s.first.clone()).unwrap_or_default()))),
                        crate::hud::BlocksWorld,
                        ChildOf(holder),
                    ))
                    .id();
                let name_cell = commands.spawn((Node { width: Val::Px(SHOP_COLUMNS[0]), height: Val::Percent(100.0), align_items: AlignItems::Center, column_gap: Val::Px(6.0), padding: UiRect::left(Val::Px(4.0)), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
                if let Some(icon) = gu.icon(&mut images, &t.icon) {
                    commands.spawn((ImageNode { image: icon, color: Color::srgba(1.0, 1.0, 1.0, fade), ..default() }, Node { width: Val::Px(30.0), height: Val::Px(30.0), ..default() }, Pickable::IGNORE, ChildOf(name_cell)));
                }
                let col = crate::layout::color(SHOP_TEXT).with_alpha(fade);
                commands.spawn((Text::new(t.name.clone()), font.clone(), line, TextColor(col), Pickable::IGNORE, ChildOf(name_cell)));
                let price_cell = commands.spawn((Node { width: Val::Px(SHOP_COLUMNS[1]), height: Val::Percent(100.0), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE, ChildOf(row))).id();
                commands.spawn((Text::new(crate::lifetime::group(t.points as i64)), font.clone(), line, TextColor(col), Pickable::IGNORE, ChildOf(price_cell)));
            }
            shown.rows = Some(holder);
        }
        _ => {}
    }
}
