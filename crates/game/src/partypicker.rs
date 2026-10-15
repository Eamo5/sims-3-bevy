//! The game's party picker (`PartyPicker` layout, `PartyPickerDialog`), put up while the host's
//! on the phone throwing a party: the kind of party, the dress code and the hour it starts (typed
//! in, AM or PM) on the left; on the right everyone the household knows (all, friends or
//! co-workers), each with their picture and how well they're liked, moved across to the guest
//! list with the arrows or a double click, ten at most. Accept stays greyed, its tooltip saying
//! why, until the party's possible (`parties::problem`).

use bevy::prelude::*;

use crate::layout::{Spawned, UiAssets, UiButton, UiCombo};
use crate::parties::{Choice, Dress, MAX_GUESTS, PartyKind, PartyPicker};

// The layout's controls (`PartyPickerDialog.ControlIds`).
const TIME_EDIT: u32 = 0x05ff_1a90;
const CLOTHING: u32 = 0x05ff_1a91;
const BUST: u32 = 0x05ff_1a92;
const FILTER_LABEL: u32 = 0x05ff_1a93;
const LIST_PANEL: u32 = 0x05ff_1a94;
const AM: u32 = 0x05ff_1a98;
const PM: u32 = 0x05ff_1a99;
const KIND: u32 = 0x05ff_1000;
const INVITEES: u32 = 0x05ef_6bd0;
const OKAY: u32 = 0x05ef_6bd1;
const CANCEL: u32 = 0x05ef_6bd2;
const POSSIBLES: u32 = 0x05ef_6bd3;
const REMOVE: u32 = 0x05ef_6bd4;
const ADD: u32 = 0x05ef_6bd5;
const FILTERS: [u32; 3] = [0x05ef_6bd6, 0x05ef_6bd7, 0x05ef_6bd8];
const FULL: u32 = 0x05ef_6bd9;

/// The tables' rows (the game's 40-pixel pictures), their header's height, the columns' widths
/// (name, liking) and the rows a table shows.
const ROW: f32 = 44.0;
const HEADER: f32 = 29.0;
const NAME_COLUMN: f32 = 205.0;
const ROWS: [usize; 2] = [7, 8];
/// Each table's width (its two columns: the guest list's liking column a little wider).
const WIDTHS: [f32; 2] = [249.0, 255.0];
const TEXT: u32 = 0xff00_3263;
const ALTERNATE: u32 = 0xffd2_ebf7;
const SELECTED: u32 = 0xff9c_c8f0;
/// Two clicks this close together (seconds) are a double click.
const DOUBLE_CLICK: f32 = 0.4;

/// A row of one of the tables: which table, whose.
#[derive(Component)]
pub(crate) struct PickRow(usize, Entity);

/// The picker on show, and what's chosen in it.
#[derive(Default)]
pub(crate) struct Shown {
    dialog: Option<Spawned>,
    host: Option<Entity>,
    kind: usize,
    dress: Option<Dress>,
    /// The hour as typed (on the twelve-hour clock), and afternoon.
    typed: String,
    pm: bool,
    invitees: Vec<Entity>,
    filter: usize,
    selected: [Option<Entity>; 2],
    scroll: [usize; 2],
    rows: [Option<Entity>; 2],
    dirty: bool,
    last_click: Option<(Entity, f32)>,
    combos: (usize, usize),
}

impl Shown {
    /// The hour chosen (0-23), if one's typed.
    fn hour(&self) -> Option<u32> {
        let h: u32 = self.typed.parse().ok().filter(|h| (1..=12).contains(h))?;
        Some(match (h, self.pm) {
            (12, false) => 0,
            (12, true) => 12,
            (h, true) => h + 12,
            (h, false) => h,
        })
    }
}

/// The picker: put up for a host's call, its controls read, and the choice made (or not).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn picker(
    mut commands: Commands,
    mut state: ResMut<PartyPicker>,
    mut shown: Local<Shown>,
    clock: Res<crate::clock::GameClock>,
    time: Res<Time>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut portraits: ResMut<crate::portraits::Portraits>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    rows: Query<(&Interaction, &PickRow), Changed<Interaction>>,
    (mut buttons, combos): (Query<&mut UiButton>, Query<&UiCombo>),
    mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    (hovered, mut wheel): (Query<&Interaction>, MessageReader<bevy::input::mouse::MouseWheel>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let Some(req) = state.open.clone() else {
        if let Some(r) = shown.dialog.take().and_then(|d| d.root) {
            commands.entity(r).try_despawn();
        }
        *shown = Shown::default();
        keys.clear();
        return;
    };
    let Some(mut ui) = ui else { return };
    let now_hour = ((clock.minutes / 60.0) % 24.0) as f32;
    // Put it up: the host's face, the kinds of party, the dress codes, two hours from now.
    if shown.host != Some(req.host) {
        if let Some(r) = shown.dialog.take().and_then(|d| d.root) {
            commands.entity(r).try_despawn();
        }
        *shown = Shown::default();
        let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "PartyPicker") else { return };
        shown.host = Some(req.host);
        let hour = (now_hour as u32 + 2) % 24;
        shown.pm = hour >= 12;
        shown.typed = match hour % 12 {
            0 => "12".into(),
            h => h.to_string(),
        };
        if let Some(head) = s.id(BUST) {
            let pic = portraits.portrait(&mut images, req.host);
            let face = commands.spawn((ImageNode::new(pic), crate::portraits::PortraitOf(req.host), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE)).id();
            commands.entity(head).insert_children(0, &[face]);
        }
        // (The guests' list shown from the start.)
        if let Some(p) = s.id(LIST_PANEL) {
            commands.entity(p).insert(Visibility::Inherited);
        }
        let caption = |ui: &UiAssets, (key, plain): (&str, &str)| ui.localize(key).unwrap_or_else(|| plain.to_string());
        let kinds: Vec<String> = req.kinds.iter().map(|k| caption(&ui, k.caption())).collect();
        let mut dresses = vec![caption(&ui, ("Ui/Caption/Party:None", "Select one:"))];
        dresses.extend(Dress::ALL.iter().map(|d| caption(&ui, d.caption())));
        for (id, items) in [(KIND, kinds), (CLOTHING, dresses)] {
            if let Some(e) = s.id(id) {
                commands.entity(e).queue(move |mut w: EntityWorldMut| {
                    if let Some(mut c) = w.get_mut::<UiCombo>() {
                        c.items = items;
                        c.selected = 0;
                    }
                });
            }
        }
        if let Some(t) = s.id(FILTER_LABEL).and_then(|w| s.text_of(w)) {
            commands.entity(t).insert(Text::new(caption(&ui, ("Ui/Caption/Party:All", "All"))));
        }
        // (The tables' own column windows: the rows here draw their columns.)
        for table in [POSSIBLES, INVITEES] {
            if let Some(t) = s.id(table) {
                commands.entity(t).queue(|mut w: EntityWorldMut| {
                    let kids: Vec<Entity> = w.get::<Children>().map(|c| c.iter().collect()).unwrap_or_default();
                    w.world_scope(|world| {
                        for k in kids {
                            if let Ok(mut e) = world.get_entity_mut(k) {
                                e.insert(Visibility::Hidden);
                            }
                        }
                    });
                });
            }
        }
        shown.dialog = Some(s);
        shown.dirty = true;
        return;
    }
    let Some(s) = shown.dialog.clone() else { return };
    let pressed = |id: u32| s.id(id).is_some_and(|e| clicks.get(e).is_ok_and(|(_, i)| *i == Interaction::Pressed));
    // The kind and the dress code, as the combo boxes have them.
    let read = |id: u32| s.id(id).and_then(|e| combos.get(e).ok()).map(|c| c.selected);
    if let (Some(k), Some(d)) = (read(KIND), read(CLOTHING))
        && (k, d) != shown.combos
    {
        shown.combos = (k, d);
        shown.kind = k.min(req.kinds.len().saturating_sub(1));
        shown.dress = d.checked_sub(1).and_then(|i| Dress::ALL.get(i).copied());
        shown.dirty = true;
    }
    // The hour typed (one or two figures, 1 to 12), AM or PM.
    for ev in keys.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        use bevy::input::keyboard::Key;
        match &ev.logical_key {
            Key::Character(c) if c.chars().all(|c| c.is_ascii_digit()) => {
                let t = if shown.typed.len() >= 2 { c.to_string() } else { format!("{}{c}", shown.typed) };
                if t.parse::<u32>().is_ok_and(|h| h <= 12) {
                    shown.typed = t;
                } else {
                    shown.typed = c.to_string();
                }
                shown.dirty = true;
            }
            Key::Backspace => {
                shown.typed.pop();
                shown.dirty = true;
            }
            _ => {}
        }
    }
    for (id, pm) in [(AM, false), (PM, true)] {
        if pressed(id) && shown.pm != pm {
            shown.pm = pm;
            shown.dirty = true;
        }
    }
    for (k, id) in FILTERS.into_iter().enumerate() {
        if pressed(id) && shown.filter != k {
            shown.filter = k;
            shown.scroll[0] = 0;
            shown.dirty = true;
            if let Some(t) = s.id(FILTER_LABEL).and_then(|w| s.text_of(w)) {
                let key = ["Ui/Caption/Party:All", "Ui/Caption/Party:Friends", "Ui/Caption/Party:Coworkers"][k];
                let plain = ["All", "Friends", "Co-Workers"][k];
                commands.entity(t).insert(Text::new(ui.localize(key).unwrap_or_else(|| plain.into())));
            }
        }
    }
    // Those to be chosen from (by the filter), and those asked.
    let listed: Vec<&crate::parties::Candidate> = req
        .candidates
        .iter()
        .filter(|c| !shown.invitees.contains(&c.sim))
        .filter(|c| match shown.filter {
            1 => c.friend,
            2 => c.coworker,
            _ => true,
        })
        .collect();
    // A row clicked: chosen; clicked twice, moved across.
    let mut moved = None;
    for (i, r) in &rows {
        if *i != Interaction::Pressed {
            continue;
        }
        let now = time.elapsed_secs();
        let double = shown.last_click.is_some_and(|(e, t)| e == r.1 && now - t < DOUBLE_CLICK);
        shown.last_click = Some((r.1, now));
        shown.selected[r.0] = Some(r.1);
        shown.dirty = true;
        if double {
            moved = Some((r.0, r.1));
        }
    }
    if pressed(ADD)
        && let Some(e) = shown.selected[0]
    {
        moved = Some((0, e));
    }
    if pressed(REMOVE)
        && let Some(e) = shown.selected[1]
    {
        moved = Some((1, e));
    }
    if let Some((table, e)) = moved {
        if table == 0 && shown.invitees.len() < MAX_GUESTS && !shown.invitees.contains(&e) {
            shown.invitees.push(e);
        } else if table == 1 {
            shown.invitees.retain(|x| *x != e);
        }
        info!("party picker: {} guests asked", shown.invitees.len());
        shown.selected = [None, None];
        play.write(crate::sound::PlaySound::ui("ui_panel_shift"));
        shown.dirty = true;
    }
    // Scrolling the table under the pointer.
    if scrolled != 0.0 {
        for (k, id) in [POSSIBLES, INVITEES].into_iter().enumerate() {
            let over = s.id(id).and_then(|t| hovered.get(t).ok()).is_some_and(|i| *i != Interaction::None);
            if over {
                let len = if k == 0 { listed.len() } else { shown.invitees.len() };
                let most = len.saturating_sub(ROWS[k]);
                shown.scroll[k] = if scrolled > 0.0 { shown.scroll[k].saturating_sub(1) } else { (shown.scroll[k] + 1).min(most) };
                shown.dirty = true;
            }
        }
    }
    let kind = req.kinds.get(shown.kind).copied().unwrap_or(PartyKind::House);
    let problem = crate::parties::problem(kind, shown.dress, shown.hour(), shown.invitees.len(), now_hour);
    // Accept, or called off.
    if pressed(CANCEL) {
        state.done = Some((req.host, None));
        state.open = None;
        return;
    }
    if pressed(OKAY)
        && problem.is_none()
        && let (Some(dress), Some(hour)) = (shown.dress, shown.hour())
    {
        info!("party picker: a {kind:?} at {hour}:00, {dress:?}, {} guests", shown.invitees.len());
        state.done = Some((req.host, Some(Choice { kind, dress, hour, guests: shown.invitees.clone() })));
        state.open = None;
        return;
    }
    if !shown.dirty {
        return;
    }
    shown.dirty = false;
    // The controls as things stand.
    if let Some(e) = s.id(OKAY) {
        let tip = match problem {
            Some(key) => ui.localize(key).unwrap_or_else(|| key.rsplit(':').next().unwrap_or("").to_string()),
            None => ui.localize("Ui/Caption/Global:Accept").unwrap_or_else(|| "Accept".into()),
        };
        commands.entity(e).insert(crate::icons::Tooltip(tip));
        if let Ok(mut b) = buttons.get_mut(e) {
            b.disabled = problem.is_some();
        }
    }
    for (id, on) in [(AM, !shown.pm), (PM, shown.pm)] {
        if let Some(e) = s.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
        {
            b.selected = on;
        }
    }
    for (k, id) in FILTERS.into_iter().enumerate() {
        if let Some(e) = s.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
        {
            b.selected = shown.filter == k;
        }
    }
    for (id, on) in [(ADD, shown.selected[0].is_some() && shown.invitees.len() < MAX_GUESTS), (REMOVE, shown.selected[1].is_some())] {
        if let Some(e) = s.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
        {
            b.disabled = !on;
        }
    }
    if let Some(e) = s.id(FULL) {
        commands.entity(e).insert(if shown.invitees.len() >= MAX_GUESTS { Visibility::Inherited } else { Visibility::Hidden });
    }
    if let Some(t) = s.id(TIME_EDIT).and_then(|w| s.text_of(w)) {
        commands.entity(t).insert(Text::new(shown.typed.clone()));
    }
    // The tables.
    let asked: Vec<&crate::parties::Candidate> = shown.invitees.iter().filter_map(|e| req.candidates.iter().find(|c| c.sim == *e)).collect();
    let (font, line) = ui.text_font(&mut fonts, 0);
    for (k, (id, list)) in [(POSSIBLES, &listed), (INVITEES, &asked)].into_iter().enumerate() {
        let Some(table) = s.id(id) else { continue };
        if let Some(r) = shown.rows[k].take() {
            commands.entity(r).try_despawn();
        }
        let holder = commands
            .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(3.0), width: Val::Px(WIDTHS[k]), top: Val::Px(HEADER), height: Val::Px(ROWS[k] as f32 * ROW), flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() }, Pickable::IGNORE, ChildOf(table)))
            .id();
        for (n, c) in list.iter().enumerate().skip(shown.scroll[k]).take(ROWS[k]) {
            let bg = if shown.selected[k] == Some(c.sim) { crate::layout::color(SELECTED) } else if n % 2 == 1 { crate::layout::color(ALTERNATE) } else { Color::NONE };
            let row = commands
                .spawn((
                    Node { height: Val::Px(ROW), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() },
                    Button,
                    BackgroundColor(bg),
                    PickRow(k, c.sim),
                    crate::icons::Tooltip(c.relation.clone()),
                    crate::hud::BlocksWorld,
                    ChildOf(holder),
                ))
                .id();
            let name_cell = commands.spawn((Node { width: Val::Px(NAME_COLUMN), height: Val::Percent(100.0), align_items: AlignItems::Center, column_gap: Val::Px(6.0), padding: UiRect::left(Val::Px(2.0)), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
            let pic = portraits.portrait(&mut images, c.sim);
            commands.spawn((ImageNode::new(pic), crate::portraits::PortraitOf(c.sim), Node { width: Val::Px(40.0), height: Val::Px(40.0), ..default() }, Pickable::IGNORE, ChildOf(name_cell)));
            commands.spawn((Text::new(c.name.clone()), font.clone(), line, TextColor(crate::layout::color(TEXT)), Pickable::IGNORE, ChildOf(name_cell)));
            let liking = commands.spawn((Node { flex_grow: 1.0, height: Val::Percent(100.0), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE, ChildOf(row))).id();
            let tint = if c.liking >= 0.0 { Color::srgb(0.1, 0.5, 0.15) } else { Color::srgb(0.7, 0.12, 0.1) };
            commands.spawn((Text::new(format!("{:.0}", c.liking)), font.clone(), line, TextColor(tint), Pickable::IGNORE, ChildOf(liking)));
        }
        shown.rows[k] = Some(holder);
    }
}
