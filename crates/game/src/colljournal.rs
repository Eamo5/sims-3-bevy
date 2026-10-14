//! The Collection Journal (J) in the game's own dialog (UI.dll's `CollectionJournalDialog` over
//! `HUDCollectionJournalDialog`): its tabs along the top (statistics, metals, gems, insects,
//! fish: the game's skewer tabs and pictures), the tab's heading, and the game's table of the
//! kind's finds: each with its picture and name (those not found yet greyed), its rarity, how
//! many the household holds, and what one fetches. The statistics tab sums up what's been found
//! and what's held. The plain panel stands aside.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use s3bake::gamedata::{CollectKind, CollectibleInfo};

use crate::AppState;
use crate::layout::{SetIcon, Spawned, UiAssets, UiButton};

pub struct CollJournalPlugin;

impl Plugin for CollJournalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CollDialog>()
            .add_systems(Update, (scripted, journal_dialog).chain().run_if(in_state(crate::PlayMode::Live)))
            .add_systems(OnExit(AppState::InGame), |mut d: ResMut<CollDialog>| *d = CollDialog::default());
    }
}

/// Whether the game's dialog is in use (the plain panel stays away).
pub static GAME_JOURNAL: AtomicBool = AtomicBool::new(false);

// `HUDCollectionJournalDialog`'s windows.
const OKAY: u32 = 0x05ef_6bd1;
const TITLE: u32 = 0x05ef_6bd4;
const HEADER: u32 = 0x0dcd_cf50;
const TABS: u32 = 0x05ef_6bd6;
const TABLE: u32 = 0x0d93_e620;
const SCROLL: u32 = 0x05ef_6bd8;
const DESCRIPTION: u32 = 0x05ef_6be0;
const STATISTICS: u32 = 0x05ef_6be1;
const OPPORTUNITIES: u32 = 0x05ef_6be3;
/// The journal's picture (the tab's, here), and the Sim's: not for the household's journal.
const ICON: u32 = 0x05ef_6bd3;
const NOT_HERE: [u32; 1] = [0x05ef_6bd2];
const TAB_STEP: f32 = 54.0;
/// The table's columns (as its `ColumnControl`s are laid out): name, the pack's icon, rarity,
/// how many are held, and what one fetches.
const COLUMNS: [f32; 5] = [213.0, 41.0, 147.0, 100.0, 235.0];
/// The table's white list (the rest of its width is the scroll bar's).
const LIST_WIDTH: f32 = 736.0;
/// Lines of the statistics tab.
const STAT_LINES: f32 = 6.0;
const LINE: f32 = 17.0;
const HEADER_HEIGHT: f32 = 25.0;
const ROW: f32 = 52.0;
const ROWS: usize = 7;
/// Rows' text, and greyed for finds not found yet (`DisabledColor`).
const ROW_TEXT: u32 = 0xff22_2b4d;
const DISABLED: u32 = 0xff63_6380;
const ALTERNATE: u32 = 0x18ff_ffff;

/// COLL_JOURNAL=1 (tests): the journal opened (12 seconds in, the first gems and metals counted
/// found), then its gems and statistics tabs, two seconds apart, with screenshots into
/// COLL_JOURNAL_SHOTS.
fn scripted(
    mut commands: Commands,
    time: Res<Time<crate::autotest::InputTimeline>>,
    mut panel: ResMut<crate::collecting::JournalPanel>,
    mut collection: ResMut<crate::collecting::Collection>,
    mut d: ResMut<CollDialog>,
    gu: Option<Res<crate::icons::GameUi>>,
    mut step: Local<u8>,
) {
    if std::env::var("COLL_JOURNAL").is_err() {
        return;
    }
    let shot = |commands: &mut Commands, name: &str| {
        let dir = std::env::var("COLL_JOURNAL_SHOTS").unwrap_or_else(|_| ".".into());
        let _ = std::fs::create_dir_all(&dir);
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(format!("{dir}/{name}.png")));
    };
    let t = time.elapsed_secs();
    match *step {
        0 if t > 12.0 => {
            if let Some(gu) = gu {
                for k in [CollectKind::Metal, CollectKind::Gem] {
                    for c in gu.data.collectibles.iter().filter(|c| c.kind == k).take(3) {
                        collection.found.insert(c.key.clone());
                    }
                }
            }
            panel.open = true;
        }
        1 if t > 14.0 => shot(&mut commands, "cj1_metals"),
        2 if t > 15.0 => d.tab = Tab::Gems,
        3 if t > 17.0 => shot(&mut commands, "cj2_gems"),
        4 if t > 18.0 => d.tab = Tab::Stats,
        5 if t > 20.0 => shot(&mut commands, "cj3_stats"),
        _ => return,
    }
    *step += 1;
}

/// A tab of the journal.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Stats,
    Metals,
    Gems,
    Insects,
    Fish,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::Stats, Tab::Metals, Tab::Gems, Tab::Insects, Tab::Fish];

    fn name(self) -> &'static str {
        match self {
            Tab::Stats => "Stats",
            Tab::Metals => "Metals",
            Tab::Gems => "Gems",
            Tab::Insects => "Insects",
            Tab::Fish => "Fish",
        }
    }

    /// Its picture (`JournalTab<Name>`).
    fn icon(self) -> String {
        format!("JournalTab{}", self.name())
    }

    fn keeps(self, k: CollectKind) -> bool {
        match self {
            Tab::Stats => false,
            Tab::Metals => k == CollectKind::Metal,
            Tab::Gems => matches!(k, CollectKind::Gem | CollectKind::SpaceRock),
            Tab::Insects => matches!(k, CollectKind::Butterfly | CollectKind::Beetle),
            Tab::Fish => k == CollectKind::Fish,
        }
    }
}

/// The dialog up: its windows, tabs and table rows, the tab chosen, how far it's scrolled.
#[derive(Resource)]
pub struct CollDialog {
    s: Option<Spawned>,
    tabs: Vec<(Entity, Tab)>,
    rows: Option<Entity>,
    tab: Tab,
    scroll: usize,
    shown: Option<(Tab, usize, usize, u32)>,
}

impl Default for CollDialog {
    fn default() -> Self {
        Self { s: None, tabs: Vec::new(), rows: None, tab: Tab::Metals, scroll: 0, shown: None }
    }
}

/// A find's picture: the game's catalogue picture of what shows it, or one drawn of it.
fn picture(c: &CollectibleInfo, gu: Option<&mut crate::icons::GameUi>, thumbs: &mut crate::thumbs::ModelThumbs, images: &mut Assets<Image>, baked: &crate::baked::Baked) -> Option<Handle<Image>> {
    let objd = baked.0.catalog.iter().find(|e| e.instance_name.eq_ignore_ascii_case(&c.model))?.objd;
    crate::thumbs::catalogue_thumb(gu, thumbs, images, objd)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn journal_dialog(
    mut commands: Commands,
    mut d: ResMut<CollDialog>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut panel: ResMut<crate::collecting::JournalPanel>,
    collection: Res<crate::collecting::Collection>,
    inventories: Query<&crate::inventory::Inventory, With<crate::sim::HouseholdMember>>,
    (mut gu, baked, mut thumbs): (Option<ResMut<crate::icons::GameUi>>, Res<crate::baked::Baked>, ResMut<crate::thumbs::ModelThumbs>),
    (clicks, tabs, hovered): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &Tab), Changed<Interaction>>, Query<&Interaction>),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let ready = ui.layout("HUDCollectionJournalDialog").is_some() && ui.layout("SkewerTabControl").is_some() && ui.layout("ColumnControl").is_some();
    GAME_JOURNAL.store(ready, Ordering::Relaxed);
    if !ready {
        return;
    }
    let close = |commands: &mut Commands, d: &mut CollDialog| {
        if let Some(r) = d.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        d.tabs.clear();
        d.rows = None;
        d.shown = None;
    };
    if !panel.open {
        if d.s.is_some() {
            close(&mut commands, &mut d);
        }
        return;
    }
    let Some(gdata) = gu.as_deref().map(|g| g.data.clone()) else { return };
    // Put up, in the middle of the screen.
    if d.s.is_none() {
        let Some(mut w) = ui.layout("HUDCollectionJournalDialog").cloned() else { return };
        // (The statistics as tall as their lines, under a one-line description.)
        crate::layout::edit_windows(&mut w, STATISTICS, &mut |t| t.area[3] = t.area[1] + STAT_LINES * LINE);
        let (wd, ht) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
        w.area = [0.0, 0.0, wd, ht];
        w.place = s3bake::ui::UiPlace::Center(0.5);
        w.flags |= s3bake::ui::WIN_VISIBLE;
        let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
        let Some(root) = s.root else { return };
        commands.entity(root).insert((GlobalZIndex(21), crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(AppState::InGame)));
        for id in NOT_HERE.into_iter().chain([OPPORTUNITIES]) {
            for x in s.all_with(id) {
                commands.entity(x).insert(Visibility::Hidden);
            }
        }
        if let Some(t) = s.text(TITLE) {
            commands.entity(t).insert(Text::new(ui.localize("UI/Caption/CollectionJournal:Title").unwrap_or_else(|| "Collection Journal".into())));
        }
        if let Some(e) = s.id(OKAY) {
            commands.entity(e).insert(crate::icons::Tooltip("Close".into()));
        }
        if let Some(t) = s.id(TABLE) {
            commands.entity(t).remove::<Pickable>().insert(Interaction::default());
        }
        // The tabs (the game's `SkewerTabControl`s), each with its picture.
        if let (Some(row), Some(tab)) = (s.id(TABS), ui.layout("SkewerTabControl").cloned()) {
            for (n, t) in Tab::ALL.iter().enumerate() {
                let mut w = tab.clone();
                let x = n as f32 * TAB_STEP;
                w.area = [x + 1.0, 0.0, x + 55.0, 58.0];
                w.place = s3bake::ui::UiPlace::Fixed;
                w.flags |= s3bake::ui::WIN_VISIBLE;
                let ts = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, row);
                let Some(te) = ts.root else { continue };
                let tip = ui.localize(&format!("UI/Comment/CollectionJournal:{}TabTooltip", t.name())).unwrap_or_else(|| t.name().to_string());
                commands.entity(te).insert((*t, crate::icons::Tooltip(tip)));
                if let Some((h, _)) = ui.image(&mut images, s3pkg::fnv64(&t.icon())) {
                    commands.entity(te).insert(SetIcon(h));
                }
                d.tabs.push((te, *t));
            }
        }
        // The table's header: the game's column buttons.
        if let (Some(table), Some(col)) = (s.id(TABLE), ui.layout("ColumnControl").cloned()) {
            let mut x = 0.0;
            for (k, word) in ["Name", "", "Rarity", "Held", "Value"].iter().enumerate() {
                let mut w = col.clone();
                w.place = s3bake::ui::UiPlace::Fixed;
                w.area = [x, 0.0, x + COLUMNS[k], HEADER_HEIGHT];
                w.caption = word.to_string();
                ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, table);
                x += COLUMNS[k];
            }
        }
        d.s = Some(s);
    }
    let Some(s) = d.s.clone() else { return };
    if crate::livehud::pressed(&clicks, s.id(OKAY)) {
        panel.open = false;
        close(&mut commands, &mut d);
        return;
    }
    if let Some((_, t)) = tabs.iter().find(|(i, _)| **i == Interaction::Pressed) {
        if d.tab != *t {
            d.tab = *t;
            d.scroll = 0;
        }
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
    }
    let tab = d.tab;
    let items: Vec<&CollectibleInfo> = gdata.collectibles.iter().filter(|c| tab.keeps(c.kind)).collect();
    let over = s.id(TABLE).and_then(|t| hovered.get(t).ok()).is_some_and(|i| *i != Interaction::None);
    if over && wheel.delta.y != 0.0 {
        let most = items.len().saturating_sub(ROWS);
        d.scroll = if wheel.delta.y > 0.0 { d.scroll.saturating_sub(1) } else { (d.scroll + 1).min(most) };
    }
    let held: u32 = inventories.iter().map(|i| i.collectibles().0).sum();
    let want = (tab, d.scroll, collection.found.len(), held);
    if d.shown == Some(want) {
        return;
    }
    d.shown = Some(want);
    for (e, t) in &d.tabs {
        let on = *t == tab;
        commands.entity(*e).queue(move |mut w: EntityWorldMut| {
            if let Some(mut b) = w.get_mut::<UiButton>()
                && b.selected != on
            {
                b.selected = on;
            }
        });
    }
    if let Some(t) = s.text(HEADER) {
        let h = ui.localize(&format!("UI/Caption/CollectionJournal:Header{}", tab.name())).unwrap_or_default();
        commands.entity(t).insert(Text::new(h));
    }
    if let (Some(win), Some((h, _))) = (s.id(ICON), ui.image(&mut images, s3pkg::fnv64(&tab.icon()))) {
        crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
    }
    let stats = tab == Tab::Stats;
    for (id, on) in [(TABLE, !stats), (SCROLL, stats)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    if let Some(old) = d.rows.take() {
        commands.entity(old).try_despawn();
    }
    let held_of = |key: &str| -> u32 { inventories.iter().map(|i| i.held(key)).sum() };
    if stats {
        // What's been found of each kind, and what the household holds.
        let worth: i64 = inventories.iter().map(|i| i.collectibles().1).sum();
        let mut lines = Vec::new();
        for t in &Tab::ALL[1..] {
            let all: Vec<&CollectibleInfo> = gdata.collectibles.iter().filter(|c| t.keeps(c.kind)).collect();
            let found = all.iter().filter(|c| collection.found.contains(&c.key)).count();
            lines.push(format!("{}: {found} of {} found", t.name(), all.len()));
        }
        lines.push(format!("Finds held by the household: {held}, worth §{}", crate::lifetime::group(worth)));
        let say = |commands: &mut Commands, id: u32, t: String| {
            if let Some(x) = s.text(id) {
                commands.entity(x).insert(Text::new(t));
            }
        };
        say(&mut commands, DESCRIPTION, format!("Found so far: {} of the town's {} collectibles.", collection.found.len(), gdata.collectibles.len()));
        say(&mut commands, STATISTICS, lines.join("\n"));
        return;
    }
    // The finds of this kind: their pictures and names, rarity, how many held, and worth.
    let Some(table) = s.id(TABLE) else { return };
    let (font, line) = ui.text_font(&mut fonts, 0);
    let rows = commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), width: Val::Px(LIST_WIDTH), top: Val::Px(HEADER_HEIGHT), height: Val::Px(ROWS as f32 * ROW), flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() },
            Pickable::IGNORE,
            ChildOf(table),
        ))
        .id();
    d.rows = Some(rows);
    let mut ordered = items.clone();
    // (Commonest first, as the game sorts it by rarity.)
    let rank = |r: &str| match r.to_ascii_lowercase().as_str() {
        "common" => 0,
        "uncommon" => 1,
        "rare" => 2,
        _ => 3,
    };
    ordered.sort_by_key(|c| (rank(&c.rarity), c.min_price));
    for (n, c) in ordered.iter().enumerate().skip(d.scroll).take(ROWS) {
        let found = collection.found.contains(&c.key);
        let colour = crate::layout::color(if found { ROW_TEXT } else { DISABLED });
        let bg = if n % 2 == 1 { crate::layout::color(ALTERNATE) } else { Color::NONE };
        let row = commands.spawn((Node { height: Val::Px(ROW), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), Pickable::IGNORE, ChildOf(rows))).id();
        let cell = |commands: &mut Commands, w: f32| commands.spawn((Node { width: Val::Px(w), height: Val::Percent(100.0), align_items: AlignItems::Center, column_gap: Val::Px(6.0), padding: UiRect::left(Val::Px(4.0)), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
        let name_cell = cell(&mut commands, COLUMNS[0]);
        if let Some(h) = picture(c, gu.as_deref_mut(), &mut thumbs, &mut images, &baked) {
            commands.spawn((ImageNode { image: h, color: Color::srgba(1.0, 1.0, 1.0, if found { 1.0 } else { 0.35 }), ..default() }, Node { width: Val::Px(48.0), height: Val::Px(48.0), flex_shrink: 0.0, ..default() }, Pickable::IGNORE, ChildOf(name_cell)));
        }
        let name = if found { c.name.clone() } else { "???".to_string() };
        let held = held_of(&c.key);
        let worth = if c.min_price == c.max_price { format!("§{}", c.min_price) } else { format!("§{}–§{}", c.min_price, c.max_price) };
        let texts = [(name_cell, name), (cell(&mut commands, COLUMNS[1]), String::new()), (cell(&mut commands, COLUMNS[2]), c.rarity.clone()), (cell(&mut commands, COLUMNS[3]), if found { held.to_string() } else { "—".into() }), (cell(&mut commands, COLUMNS[4]), if found { worth } else { "—".into() })];
        for (e, t) in texts {
            if t.is_empty() {
                continue;
            }
            commands.spawn((Text::new(t), font.clone(), line, TextColor(colour), Pickable::IGNORE, ChildOf(e)));
        }
    }
}
