//! The game's real estate dialog (UI.dll's `RealEstateDialog` over its own layout), put up once
//! a Sim has checked on the household's real estate: on the computer ("Check Real Estate", two
//! minutes: the game's `Computer.CheckRealEstate`) or by phone ("Real Estate and Travel
//! Services... › Check Real Estate": `Phone.CallCheckRealEstate`). The game stands still while
//! it's up.
//!
//! What the household owns (`realestate`), under its tabs: All Properties (each one's kind and
//! share, its name, what it'd sell for) and Buildings, the rabbit holes (name, whether it's
//! theirs outright, what was paid, the profits waiting). With a row picked, "Sell" sells it there
//! and then, for what was paid and what's waiting (the game's `TotalValue`), and "Collect
//! Simoleons" takes the profits. "View Purchasable Properties" turns to what's for sale: each
//! rabbit hole not theirs outright, at the price of owning it (the buyout, and the stake too where
//! they've none: `GetPropertiesForSale`); "Buy Property" buys the one picked, if the household
//! can pay (`BuyoutRabbithole`). With nothing owned it opens on those. A building that's theirs
//! outright can be renamed ("Rename": the game's box to type it in, the dialog away meanwhile),
//! and is known by that name about town. The check closes it.

use bevy::prelude::*;

use crate::layout::{SetIcon, Spawned, UiAssets, UiButton};
use crate::realestate::{ForSale, RealEstate};
use crate::{AppState, PlayMode};

pub struct EstateDialogPlugin;

impl Plugin for EstateDialogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EstateDialog>()
            .add_systems(Update, (estate_test, estate_dialog, renamed).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(AppState::InGame), |mut d: ResMut<EstateDialog>| *d = EstateDialog::default());
    }
}

// `RealEstateDialog`'s windows (its `ControlID`).
const OK: u32 = 1;
const ALL_TABLE: u32 = 3;
const BUILDINGS_TABLE: u32 = 4;
const SELL: u32 = 16;
const RENAME: u32 = 17;
const COLLECT: u32 = 18;
const REQUIREMENTS: u32 = 19;
const SWITCH_VIEW: u32 = 20;
const OWNED: u32 = 21;
const FOR_SALE: u32 = 22;
const BUY: u32 = 23;
const SALE_TABLE: u32 = 24;
const TABS: [u32; 2] = [0x1001, 0x1002];
/// What this game hasn't: venues, vacation homes, private lots and resorts (their tabs and
/// tables), a venue's upgrade requirements.
const NOT_HERE: [u32; 9] = [0x1003, 0x1004, 0x1005, 0x1006, 5, 6, 7, 8, REQUIREMENTS];
/// A building's new name's letters at most (the game's).
pub const NAME_LENGTH: usize = 35;

/// The tables' columns, as the layout's `ColumnControl`s lay them out, and their headers' words
/// (the game's keys, and the words where they're not to be had).
const ALL_COLUMNS: [f32; 3] = [250.0, 249.0, 149.0];
const BUILDING_COLUMNS: [f32; 4] = [225.0, 99.0, 124.0, 199.0];
const SALE_COLUMNS: [f32; 3] = [250.0, 199.0, 199.0];
const ALL_HEADERS: [(&str, &str); 3] = [("PropertyType", "Property Type"), ("Name", "Name"), ("TotalValue", "Sale Price")];
const BUILDING_HEADERS: [(&str, &str); 4] = [("Name", "Name"), ("FullOwner", "Full Owner"), ("StoredValue", "Sale Price"), ("CollectibleFunds", "Profits")];
const SALE_HEADERS: [(&str, &str); 3] = [("Name", "Name"), ("PropertyType", "Property Type"), ("Price", "Cost")];
/// The owned tables' size: their window's width, down to the buttons (the game stretches them
/// to it; the layout has them smaller).
const OWNED_TABLE: Vec2 = Vec2::new(678.0, 365.0);
/// The tabs' tooltips (by their words' keys) and where they sit.
const TAB_TIPS: [(u64, &str); 2] = [(0xa84e_9d8d_0e5b_c0c5, "All Properties"), (0x8af2_c56d_6ec4_0219, "Buildings")];
const TAB_STEP: f32 = 54.0;
/// The rows: their height, the header's, how many show.
const ROW: f32 = 30.0;
const HEADER: f32 = 26.0;
const ROWS: usize = 11;
const ROW_TEXT: u32 = 0xff22_2b4d;
const PICKED: u32 = 0x4060_a0ff;
const ALTERNATE: u32 = 0x18ff_ffff;

/// Who's checking, and the dialog: which view and tab, what's picked, how far down.
#[derive(Resource, Default)]
pub struct EstateDialog {
    pub sim: Option<Entity>,
    s: Option<Spawned>,
    for_sale: bool,
    tab: usize,
    picked: Option<usize>,
    picked_sale: Option<usize>,
    scroll: usize,
    rows: Vec<Entity>,
    tabs: Vec<Entity>,
    shown: Option<String>,
}

impl EstateDialog {
    /// Up for a Sim (unless it's up already).
    pub fn open(&mut self, sim: Entity) {
        if self.sim.is_none() {
            *self = EstateDialog { sim: Some(sim), ..default() };
        }
    }
}

/// A row's button: the owned table's, or the for-sale one's.
#[derive(Component, Clone, Copy)]
struct EstateRow(usize, bool);
#[derive(Component, Clone, Copy)]
struct EstateTab(usize);

/// A building's new name typed in (none, or cancelled: as it was).
fn renamed(mut answers: MessageReader<crate::dialog::Answered>, typed: Res<crate::dialog::Typed>, mut re: ResMut<RealEstate>) {
    for a in answers.read() {
        let crate::dialog::Question::RenameProperty { lot, .. } = &a.about else { continue };
        let name: String = typed.0.trim().chars().take(NAME_LENGTH).collect();
        if a.answer != 0 || name.is_empty() || !re.holding(*lot).is_some_and(|h| h.full) {
            continue;
        }
        info!("renamed lot {lot} {name}");
        re.renamed.retain(|r| r.0 != *lot);
        re.renamed.push((*lot, name));
    }
}

/// ESTATE=1 (tests): the dialog up for the selected Sim ten seconds in (ESTATE=sale: turned to
/// what's for sale).
fn estate_test(mut d: ResMut<EstateDialog>, time: Res<Time>, sel: Query<Entity, With<crate::sim::Selected>>, mut done: Local<bool>) {
    let Ok(which) = std::env::var("ESTATE") else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let Ok(e) = sel.single() else { return };
    *done = true;
    info!("estate test: the real estate dialog up");
    d.open(e);
    if which == "sale" {
        d.for_sale = true;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn estate_dialog(
    mut commands: Commands,
    mut d: ResMut<EstateDialog>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (mut re, mut household, world, clock, mut questions): (ResMut<RealEstate>, Option<ResMut<crate::interact::Household>>, Res<crate::loading::CurrentWorld>, Res<crate::clock::GameClock>, ResMut<crate::dialog::Questions>),
    sims: Query<(), With<crate::sim::Sim>>,
    (clicks, rows, tabs, hovered, mut buttons): (
        Query<(Entity, &Interaction), Changed<Interaction>>,
        Query<(&Interaction, &EstateRow), Changed<Interaction>>,
        Query<(&Interaction, &EstateTab), Changed<Interaction>>,
        Query<&Interaction>,
        Query<&mut UiButton>,
    ),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let close = |commands: &mut Commands, d: &mut EstateDialog| {
        if let Some(r) = d.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        *d = EstateDialog::default();
    };
    let Some(me) = d.sim else { return };
    if !sims.contains(me) {
        close(&mut commands, &mut d);
        return;
    }
    let loc = |ui: &UiAssets, k: &str, or: &str| ui.localize(k).unwrap_or_else(|| or.to_string());
    if d.s.is_none() {
        let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "RealEstateDialog") else {
            warn!("no RealEstateDialog layout");
            d.sim = None;
            return;
        };
        for id in NOT_HERE {
            for x in s.all_with(id) {
                commands.entity(x).insert(Visibility::Hidden);
            }
        }
        // The tables' headers: the game's column buttons, its words.
        if let Some(col) = ui.layout("ColumnControl").cloned() {
            for (table, words, widths) in [(ALL_TABLE, &ALL_HEADERS[..], &ALL_COLUMNS[..]), (BUILDINGS_TABLE, &BUILDING_HEADERS[..], &BUILDING_COLUMNS[..]), (SALE_TABLE, &SALE_HEADERS[..], &SALE_COLUMNS[..])] {
                let Some(t) = s.id(table) else { continue };
                commands.entity(t).despawn_children().remove::<Pickable>().insert(Interaction::default());
                if table != SALE_TABLE {
                    commands.entity(t).queue(|mut e: EntityWorldMut| {
                        if let Some(mut n) = e.get_mut::<Node>() {
                            n.width = Val::Px(OWNED_TABLE.x);
                            n.height = Val::Px(OWNED_TABLE.y);
                        }
                    });
                }
                let mut x = 0.0;
                for (k, (key, or)) in words.iter().enumerate() {
                    let mut w = col.clone();
                    w.place = s3bake::ui::UiPlace::Fixed;
                    w.area = [x, 0.0, x + widths[k], HEADER];
                    w.caption = loc(&ui, &format!("Ui/Caption/RealEstateDialog/Column:{key}"), or);
                    ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, t);
                    x += widths[k];
                }
            }
        }
        // The tabs, left to right, each the game's skewer tab with its picture.
        let template = ui.layout("SkewerTabControl").cloned();
        for (i, id) in TABS.iter().enumerate() {
            let (Some(t), Some(mut w)) = (s.id(*id), template.clone()) else { continue };
            commands.entity(t).insert(Node { position_type: PositionType::Absolute, left: Val::Px(i as f32 * TAB_STEP), top: Val::Px(0.0), width: Val::Px(56.0), height: Val::Px(50.0), ..default() });
            w.area = [1.0, 0.0, 55.0, 50.0];
            let b = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, t);
            let Some(r) = b.root else { continue };
            let (key, or) = TAB_TIPS[i];
            commands.entity(r).insert((EstateTab(i), crate::icons::Tooltip(ui.localize_key(key).unwrap_or_else(|| or.to_string()))));
            let icon = ui.find("RealEstateDialog", *id).map_or(0, |w| w.icon);
            if let Some((h, _)) = ui.image(&mut images, icon) {
                commands.entity(r).insert(SetIcon(h));
            }
            d.tabs.push(r);
        }
        if let Some(e) = s.id(OK) {
            commands.entity(e).insert(crate::icons::Tooltip("Close".into()));
        }
        // (Nothing owned: what's for sale first.)
        if re.holdings.is_empty() {
            d.for_sale = true;
        }
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
        d.s = Some(s);
    }
    let Some(s) = d.s.clone() else { return };
    // (Away while a building's being renamed, as the game's is.)
    let renaming = questions.queue.front().is_some_and(|a| matches!(a.about, crate::dialog::Question::RenameProperty { .. }));
    if let Some(r) = s.root {
        commands.entity(r).insert(if renaming { Visibility::Hidden } else { Visibility::Inherited });
    }
    if renaming {
        d.shown = None;
        return;
    }
    let funds = household.as_ref().map_or(0, |h| h.funds);
    let place = |lot: usize| crate::visit::place_name(&world.data, lot);
    let sale: Vec<ForSale> = crate::realestate::for_sale(&world.data, &re);
    // The buttons.
    if crate::livehud::pressed(&clicks, s.id(OK)) {
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    if crate::livehud::pressed(&clicks, s.id(SWITCH_VIEW)) {
        d.for_sale = !d.for_sale;
        d.scroll = 0;
        d.picked_sale = None;
    }
    if let Some((_, t)) = tabs.iter().find(|(i, _)| **i == Interaction::Pressed) {
        if d.tab != t.0 {
            d.tab = t.0;
            d.picked = None;
            d.scroll = 0;
        }
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
    }
    if let Some((_, r)) = rows.iter().find(|(i, _)| **i == Interaction::Pressed) {
        if r.1 { d.picked_sale = Some(r.0) } else { d.picked = Some(r.0) }
    }
    let held = d.picked.and_then(|i| re.holdings.get(i)).cloned();
    let disabled = |buttons: &Query<&mut UiButton>, id: u32| s.id(id).and_then(|e| buttons.get(e).ok()).is_none_or(|b| b.disabled);
    if crate::livehud::pressed(&clicks, s.id(SELL))
        && !disabled(&buttons, SELL)
        && let Some(h) = &held
        && let Some(got) = re.sell(h.lot)
    {
        if let Some(hh) = household.as_mut() {
            hh.funds += got;
        }
        info!("sold the stake in {} for §{got}", place(h.lot));
        d.picked = None;
        play.write(crate::sound::PlaySound::ui("ui_object_sell"));
    }
    if crate::livehud::pressed(&clicks, s.id(RENAME))
        && !disabled(&buttons, RENAME)
        && let Some(h) = &held
    {
        let say = |k: &str, or: &str| loc(&ui, &format!("Gameplay/Abstracts/RabbitHole/RenameRabbitHole:{k}"), or);
        questions.ask(crate::dialog::Ask {
            about: crate::dialog::Question::RenameProperty { sim: me, lot: h.lot, suggested: place(h.lot) },
            icon: String::new(),
            heading: String::new(),
            title: say("MenuText", "Rename Building"),
            text: say("PromptText", "Now that you own this building, you can rename it! What would you like to name your new acquisition?"),
            answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }, crate::dialog::Answer { label: "Cancel".into(), detail: String::new(), icon: String::new() }],
        });
        return;
    }
    if crate::livehud::pressed(&clicks, s.id(COLLECT))
        && !disabled(&buttons, COLLECT)
        && let Some(h) = &held
    {
        let got = re.collect(h.lot);
        if let Some(hh) = household.as_mut() {
            hh.funds += got;
        }
        play.write(crate::sound::PlaySound::ui("ui_object_sell"));
    }
    if crate::livehud::pressed(&clicks, s.id(BUY))
        && !disabled(&buttons, BUY)
        && let Some(x) = d.picked_sale.and_then(|i| sale.get(i))
        && let Some(hh) = household.as_mut()
        && re.buy_outright(x, &mut hh.funds, crate::realestate::payday_week(&clock))
    {
        info!("bought {} outright for §{}", place(x.lot), x.cost);
        d.picked_sale = None;
        play.write(crate::sound::PlaySound::ui("ui_object_buy"));
    }
    // (Scrolled by the wheel over the table shown.)
    let (table, len) = if d.for_sale { (SALE_TABLE, sale.len()) } else { ([ALL_TABLE, BUILDINGS_TABLE][d.tab], re.holdings.len()) };
    let over = s.id(table).and_then(|t| hovered.get(t).ok()).is_some_and(|i| *i != Interaction::None);
    if over && wheel.delta.y != 0.0 {
        let most = len.saturating_sub(ROWS);
        d.scroll = if wheel.delta.y > 0.0 { d.scroll.saturating_sub(1) } else { (d.scroll + 1).min(most) };
    }
    let funds_now = household.as_ref().map_or(funds, |h| h.funds);
    let key = format!("{} {} {:?} {:?} {} {:?} {:?} {funds_now}", d.for_sale, d.tab, d.picked, d.picked_sale, d.scroll, re.holdings, re.renamed);
    if d.shown.as_ref() == Some(&key) {
        return;
    }
    d.shown = Some(key);
    let sale: Vec<ForSale> = crate::realestate::for_sale(&world.data, &re);
    // The view: what's owned, or for sale; the button that turns between them.
    for (id, on) in [(OWNED, !d.for_sale), (FOR_SALE, d.for_sale), (ALL_TABLE, d.tab == 0), (BUILDINGS_TABLE, d.tab == 1)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    let switch = if d.for_sale { loc(&ui, "Ui/Caption/RealEstateDialog/Button:ViewOwnedProperties", "View Owned Properties") } else { loc(&ui, "Ui/Caption/RealEstateDialog/Button:ViewPurchaseableProperties", "View Purchasable Properties") };
    if let Some(t) = s.id(SWITCH_VIEW).and_then(|b| s.text_of(b)) {
        commands.entity(t).insert(Text::new(switch));
    }
    let tab = d.tab;
    for (i, e) in d.tabs.iter().enumerate() {
        if let Ok(mut b) = buttons.get_mut(*e)
            && b.selected != (i == tab)
        {
            b.selected = i == tab;
        }
    }
    // The buttons for what's picked: sold, collected from (if there's something waiting),
    // bought (if the household can pay).
    let held = d.picked.and_then(|i| re.holdings.get(i));
    let buyable = d.picked_sale.and_then(|i| sale.get(i)).is_some_and(|x| x.cost <= funds_now);
    let tips = [
        (SELL, held.is_some(), loc(&ui, "Ui/Caption/RealEstateDialog/Button:Sell", "Sell")),
        (
            RENAME,
            held.is_some_and(|h| h.full),
            if held.is_some_and(|h| !h.full) {
                loc(&ui, "Ui/Caption/RealEstateDialog/Button:RenameGreyedOut", "You can only rename Buildings that you fully own.")
            } else {
                loc(&ui, "Ui/Caption/RealEstateDialog/Button:Rename", "Rename")
            },
        ),
        (
            COLLECT,
            held.is_some_and(|h| h.funds > 0),
            if held.is_some_and(|h| h.funds == 0) {
                loc(&ui, "Ui/Caption/RealEstateDialog/Button:CollectMoneyGreyedOut", "This property has no uncollected funds.")
            } else {
                loc(&ui, "Ui/Caption/RealEstateDialog/Button:CollectMoney", "Collect Simoleons")
            },
        ),
        (BUY, buyable, String::new()),
    ];
    for (id, on, tip) in tips {
        let Some(e) = s.id(id) else { continue };
        if let Ok(mut b) = buttons.get_mut(e)
            && b.disabled == on
        {
            b.disabled = !on;
        }
        if !tip.is_empty() {
            commands.entity(e).insert(crate::icons::Tooltip(tip));
        }
    }
    // The rows, afresh.
    for e in d.rows.drain(..) {
        commands.entity(e).try_despawn();
    }
    let (font, line) = ui.text_font(&mut fonts, 0);
    let colour = crate::layout::color(ROW_TEXT);
    let building = loc(&ui, "UI/RealEstateDialog:RabbitHolePropertyType", "Building");
    let (yes, no) = (loc(&ui, "Ui/Caption/Global:Yes", "Yes"), loc(&ui, "Ui/Caption/Global:No", "No"));
    let share = |full: bool| if full { loc(&ui, "Ui/Caption/RealEstateDialog/RabbitholeOwnerType:FullOwner", "Full Owner") } else { loc(&ui, "Ui/Caption/RealEstateDialog/RabbitholeOwnerType:Partner", "Partner") };
    let money = |n: i64| format!("§{}", crate::lifetime::group(n));
    let lines: Vec<Vec<String>> = if d.for_sale {
        sale.iter().map(|x| vec![place(x.lot), building.clone(), money(x.cost)]).collect()
    } else if tab == 0 {
        re.holdings.iter().map(|h| vec![format!("{building} ({})", share(h.full)), place(h.lot), money(h.stored + h.funds)]).collect()
    } else {
        re.holdings.iter().map(|h| vec![place(h.lot), if h.full { yes.clone() } else { no.clone() }, money(h.stored), money(h.funds)]).collect()
    };
    let widths: &[f32] = if d.for_sale { &SALE_COLUMNS } else if tab == 0 { &ALL_COLUMNS } else { &BUILDING_COLUMNS };
    let picked = if d.for_sale { d.picked_sale } else { d.picked };
    if let Some(t) = s.id(table) {
        let holder = commands
            .spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(HEADER), width: Val::Px(widths.iter().sum()), height: Val::Px(ROWS as f32 * ROW), flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() },
                Pickable::IGNORE,
                ChildOf(t),
            ))
            .id();
        for (n, cells) in lines.iter().enumerate().skip(d.scroll).take(ROWS) {
            let bg = if picked == Some(n) { crate::layout::color(PICKED) } else if n % 2 == 1 { crate::layout::color(ALTERNATE) } else { Color::NONE };
            let row = commands.spawn((Button, EstateRow(n, d.for_sale), Node { height: Val::Px(ROW), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), ChildOf(holder))).id();
            for (w, words) in widths.iter().zip(cells) {
                let cell = commands.spawn((Node { width: Val::Px(*w), padding: UiRect::left(Val::Px(6.0)), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
                commands.spawn((Text::new(words.clone()), font.clone(), line, TextColor(colour), Pickable::IGNORE, ChildOf(cell)));
            }
        }
        d.rows.push(holder);
    }
}
