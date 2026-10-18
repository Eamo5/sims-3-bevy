//! The grocery's shop, in the game's own dialog (UI.dll's `ShoppingController` over its `Shop`
//! layout), put up when a Sim goes in to "Shop" (the game's `ShopByRecipe`), the game paused
//! while it's up:
//!
//! - "Shop": what the grocery sells, by its tabs (the ingredients' `UiTab`: produce, meat, fish),
//!   at its prices (`StoreItem.GetPrice`: produce at half again, being Nice);
//! - "Shop by Recipe": the recipes the shopper knows that the grocery has all of, a recipe's
//!   ingredients bought together;
//! - "Sell": the shopper's produce and fish, at a tenth over their worth (`kSellBackPriceModifier`).
//!
//! The bookstore's shop (the game's `Bookstore.ShopForBooks`) is the same dialog over its own
//! tabs: "General", the books read for their own sake it sells; "Skill", the skill books; and
//! "Recipe", the recipe books a Sim could learn from; at their prices (less for a Bookshop
//! Bargainer or a Haggler), each a book for the household's shelves (a recipe's, learned).
//!
//! A row is picked, and added ("Add to Cart", or picked again); a line of the cart, clicked,
//! comes off it. A coupon the shopper holds for something (`coupons`) makes one of it the
//! coupon's price, and is used up on paying. The cart shows each line's count and cost, the total
//! and the household's funds; "Purchase" pays and puts what's bought in the shopper's inventory, Nice
//! (`kQualityOfPurchesdIngredients`), or sells what's on the list. The close button leaves.

use bevy::prelude::*;
use s3bake::GameDataBaked;

use crate::inventory::{Inventory, ItemKind, Stack};
use crate::layout::{Spawned, UiAssets};
use crate::sim::Sim;
use crate::{AppState, PlayMode};

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShopDialog>()
            .add_systems(Update, (shop_test, open_shop, shop_dialog).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(AppState::InGame), |mut d: ResMut<ShopDialog>| *d = ShopDialog::default());
    }
}

// `Shop`'s windows (`ShoppingController.ControlID`).
const CLOSE: u32 = 0x05e4_e800;
const BY_ITEM: u32 = 0x05e4_e801;
const BY_RECIPE: u32 = 0x05e4_e802;
const BY_SELL: u32 = 0x05e4_e803;
const STORE_TABLE: u32 = 0x05e4_e810;
const TABS: u32 = 0x05e4_e811;
const STORE_TITLE: u32 = 0x05e4_e812;
const MODE_TEXT: u32 = 0x05e4_e813;
const ADD: u32 = 0x05e4_e814;
const STORE_NAME: u32 = 0x05e4_e815;
const CART_TABLE: u32 = 0x05e4_e820;
const TOTAL: u32 = 0x05e4_e821;
const PURCHASE: u32 = 0x05e4_e822;
const CLEAR: u32 = 0x05e4_e823;
const CART_TITLE: u32 = 0x05e4_e824;
const FUNDS: u32 = 0x05e4_e825;
const CART_BUTTON: u32 = 0x05e4_e828;
const SIM_THUMB: u32 = 0x05ac_1606;
/// What this shop hasn't: the saved shopping lists.
const NOT_HERE: [u32; 4] = [0x05e4_e82a, 0x05e4_e833, 0x05e4_e834, 0x05e4_e835];

/// The tables' rows: their height, how many show, their columns.
const ROW: f32 = 30.0;
const HEADER: f32 = 26.0;
const STORE_ROWS: usize = 10;
const STORE_COLUMNS: [f32; 2] = [245.0, 91.0];
const CART_ROWS: usize = 10;
const CART_COLUMNS: [f32; 3] = [203.0, 61.0, 120.0];
const ROW_TEXT: u32 = 0xff22_2b4d;
const PICKED: u32 = 0x4060_a0ff;
const ALTERNATE: u32 = 0x18ff_ffff;
/// What a sold one fetches over its worth.
const SELL_BACK: f64 = 1.1;

/// What the store's table shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Buy,
    Recipe,
    Sell,
}

/// The buying tabs: the ingredients' own (`UiTab`), and everything.
const TABS_SHOWN: [(&str, &str); 4] = [("All", ""), ("Produce", "Produce"), ("Meat", "Meat"), ("Fish", "Fish")];
/// The bookstore's (its `mItemDictionary`'s).
const BOOK_TABS: [(&str, &str); 3] = [("General", "General"), ("Skill", "Skill"), ("Recipe", "Recipe")];

/// Which shop it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Store {
    #[default]
    Grocery,
    Books,
}

impl Store {
    fn tabs(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Store::Grocery => &TABS_SHOWN,
            Store::Books => &BOOK_TABS,
        }
    }
}

/// A line of a list: an ingredient bought, or a stack sold; a book (read for its own sake, a
/// skill's, a recipe's) bought.
#[derive(Clone, PartialEq, Debug)]
pub enum Line {
    Buy(String),
    Sell(ItemKind, String, u8),
    Novel(String),
    SkillBook(String),
    RecipeBook(String),
}

/// What the household has of books, and what the shopper knows, for the bookstore to sell by.
#[derive(Default)]
pub struct Shelves<'a> {
    pub novels: Vec<String>,
    pub skill_books: Vec<String>,
    pub cooking: u32,
    pub known: Option<&'a crate::meals::KnownRecipes>,
    pub wishes: Option<&'a crate::wishes::Wishes>,
}

/// What the bookstore sells under a tab: the books the household hasn't, at their prices (a
/// recipe's, those the shopper could learn from).
pub fn books_for_sale(data: &GameDataBaked, tab: &str, s: &Shelves) -> Vec<Row> {
    let price = |p: i64| crate::wishes::book_price(s.wishes, p as i32);
    match tab {
        "Skill" => crate::skillbooks::for_sale(&data.skill_books, &s.skill_books)
            .into_iter()
            .map(|(i, _)| {
                let b = &data.skill_books[i];
                Row { label: b.name.clone(), price: price(b.price), adds: vec![Line::SkillBook(b.key.clone())] }
            })
            .collect(),
        "Recipe" => crate::meals::books_for(data, s.cooking, s.known)
            .into_iter()
            .map(|r| {
                let rec = &data.recipes[r];
                Row { label: rec.name.clone(), price: price(rec.book_price as i64), adds: vec![Line::RecipeBook(rec.key.clone())] }
            })
            .collect(),
        _ => {
            let mut v: Vec<&s3bake::gamedata::NovelInfo> = data.novels.iter().filter(|b| b.in_store && b.price > 0 && !s.novels.contains(&b.key)).collect();
            v.sort_by(|a, b| a.genre.cmp(&b.genre).then(a.name.cmp(&b.name)));
            v.into_iter().map(|b| Row { label: format!("{} ({})", b.name, b.genre), price: price(b.price), adds: vec![Line::Novel(b.key.clone())] }).collect()
        }
    }
}

/// A row of the store's table: what it's called, what it costs (or fetches), and what it puts
/// on the list.
#[derive(Clone, PartialEq, Debug)]
pub struct Row {
    pub label: String,
    pub price: i64,
    pub adds: Vec<Line>,
}

/// What the grocery sells under a tab (`""`: all), cheapest first by tab.
pub fn for_sale(data: &GameDataBaked, tab: &str) -> Vec<Row> {
    let mut v: Vec<&s3bake::gamedata::IngredientInfo> = data.ingredients.iter().filter(|i| !i.kind && i.can_buy && i.price > 0 && (tab.is_empty() || i.tab == tab)).collect();
    v.sort_by(|a, b| a.tab.cmp(&b.tab).then(a.name.cmp(&b.name)));
    v.into_iter().map(|i| Row { label: i.name.clone(), price: crate::ingredients::store_price(i), adds: vec![Line::Buy(i.key.clone())] }).collect()
}

/// The recipes to shop for: those known (`knows`) whose ingredients the grocery has all of,
/// each its ingredients (the cheapest of a kind) together.
pub fn by_recipe(data: &GameDataBaked, knows: impl Fn(&s3bake::gamedata::RecipeInfo) -> bool) -> Vec<Row> {
    data.recipes
        .iter()
        .filter(|r| !r.snack && !r.ingredients.is_empty() && knows(r))
        .filter_map(|r| {
            let items: Vec<&s3bake::gamedata::IngredientInfo> = r.ingredients.iter().map(|k| crate::ingredients::cheapest(data, k)).collect::<Option<_>>()?;
            Some(Row { label: r.name.clone(), price: items.iter().map(|i| crate::ingredients::store_price(i)).sum(), adds: items.iter().map(|i| Line::Buy(i.key.clone())).collect() })
        })
        .collect()
}

/// What the shopper can sell here: their produce and fish.
pub fn to_sell(inv: Option<&Inventory>) -> Vec<Row> {
    inv.map_or(Vec::new(), |inv| {
        inv.0
            .iter()
            .filter(|s| matches!(s.kind, ItemKind::Produce | ItemKind::Fish) && s.count > 0)
            .map(|s| Row { label: format!("{} ({})", s.name, s.count), price: sell_price(s), adds: vec![Line::Sell(s.kind, s.key.clone(), s.quality)] })
            .collect()
    })
}

fn sell_price(s: &Stack) -> i64 {
    (s.each() as f64 * SELL_BACK).round() as i64
}

/// A list's line: its name and what one costs (or fetches).
fn line_info(data: &GameDataBaked, inv: Option<&Inventory>, wishes: Option<&crate::wishes::Wishes>, l: &Line) -> Option<(String, i64)> {
    let book = |p: i64| crate::wishes::book_price(wishes, p as i32);
    match l {
        Line::Buy(k) => data.ingredients.iter().find(|i| i.key == *k).map(|i| (i.name.clone(), crate::ingredients::store_price(i))),
        Line::Sell(kind, k, q) => inv?.0.iter().find(|s| s.kind == *kind && s.key == *k && s.quality == *q).map(|s| (s.name.clone(), sell_price(s))),
        Line::Novel(k) => data.novels.iter().find(|b| b.key == *k).map(|b| (b.name.clone(), book(b.price))),
        Line::SkillBook(k) => data.skill_books.iter().find(|b| b.key == *k).map(|b| (b.name.clone(), book(b.price))),
        Line::RecipeBook(k) => data.recipes.iter().find(|r| r.key == *k).map(|r| (r.name.clone(), book(r.book_price as i64))),
    }
}

/// How many more of a stack can go on the list to sell (no more than are held); a book, the one.
fn sellable(inv: Option<&Inventory>, l: &Line, listed: u32) -> bool {
    match l {
        Line::Buy(_) => true,
        Line::Sell(kind, k, q) => inv.and_then(|inv| inv.0.iter().find(|s| s.kind == *kind && s.key == *k && s.quality == *q)).is_some_and(|s| s.count > listed),
        Line::Novel(_) | Line::SkillBook(_) | Line::RecipeBook(_) => listed == 0,
    }
}

/// Puts a line on a list (one more of it).
pub fn add_line(list: &mut Vec<(Line, u32)>, l: Line) {
    match list.iter_mut().find(|(x, _)| *x == l) {
        Some((_, n)) => *n += 1,
        None => list.push((l, 1)),
    }
}

/// The dialog: who's shopping (and the lot), its windows, and what's chosen.
#[derive(Resource, Default)]
pub struct ShopDialog {
    pub shopper: Option<Entity>,
    pub store: Store,
    s: Option<Spawned>,
    mode: Mode,
    tab: usize,
    picked: Option<usize>,
    scroll: usize,
    buy: Vec<(Line, u32)>,
    sell: Vec<(Line, u32)>,
    rows: Vec<Entity>,
    tabs: Vec<Entity>,
    shown: Option<String>,
    /// The clock's speed before it was paused for the dialog.
    speed: usize,
}

/// A store row's button.
#[derive(Component, Clone, Copy)]
struct StoreRow(usize);
/// A cart line's button.
#[derive(Component, Clone, Copy)]
struct CartLine(usize);
/// A buying tab's button.
#[derive(Component, Clone, Copy)]
struct ShopTab(usize);

/// Gone in to shop: the dialog up, for the household's Sim (once a visit).
#[derive(Component)]
pub struct Shopped;

fn open_shop(
    mut commands: Commands,
    mut d: ResMut<ShopDialog>,
    clock: Res<crate::clock::GameClock>,
    sims: Query<(Entity, &crate::rabbitholes::AtRabbitHole), (With<crate::sim::HouseholdMember>, Without<Shopped>)>,
    gone: Query<Entity, (With<Shopped>, Without<crate::rabbitholes::AtRabbitHole>)>,
) {
    for e in &gone {
        commands.entity(e).remove::<Shopped>();
    }
    if d.shopper.is_some() {
        return;
    }
    let shop = |effect| crate::venues::ACTS.iter().find(|a| a.effect == effect).map(|a| &a.activity);
    let (Some(grocery), Some(books)) = (shop(crate::venues::Effect::Shop), shop(crate::venues::Effect::ShopBooks)) else { return };
    if let Some((e, at)) = sims.iter().find(|(_, at)| (std::ptr::eq(at.activity, grocery) || std::ptr::eq(at.activity, books)) && clock.minutes >= at.inside_from) {
        commands.entity(e).insert(Shopped);
        d.shopper = Some(e);
        d.store = if std::ptr::eq(at.activity, books) { Store::Books } else { Store::Grocery };
        d.mode = Mode::Buy;
        d.tab = 0;
        d.picked = None;
        d.scroll = 0;
        d.buy.clear();
        d.sell.clear();
        d.shown = None;
    }
}

/// SHOP=1 (tests): the shop put up for the selected Sim ten seconds in, as if they'd gone in
/// (SHOP=books: the bookstore's).
fn shop_test(mut d: ResMut<ShopDialog>, time: Res<Time>, sel: Query<Entity, With<crate::sim::Selected>>, mut done: Local<bool>) {
    let Ok(which) = std::env::var("SHOP") else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    let Ok(e) = sel.single() else { return };
    *done = true;
    info!("shop test: the shop up");
    d.shopper = Some(e);
    d.store = if which == "books" { Store::Books } else { Store::Grocery };
}

/// `{0}` (or `{0.Money}`...) in the game's words filled.
fn fill(template: &str, value: &str) -> String {
    match (template.find("{0"), template.find('}')) {
        (Some(a), Some(b)) if b > a => format!("{}{value}{}", &template[..a], &template[b + 1..]),
        _ => format!("{template} {value}"),
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shop_dialog(
    mut commands: Commands,
    mut d: ResMut<ShopDialog>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (mut clock, mut household, gu): (ResMut<crate::clock::GameClock>, Option<ResMut<crate::interact::Household>>, Option<ResMut<crate::icons::GameUi>>),
    (mut pictures, baked, mut thumbs): (ResMut<crate::paintings::PaintingImages>, Option<Res<crate::baked::Baked>>, ResMut<crate::thumbs::ModelThumbs>),
    mut shoppers: Query<(
        &Sim,
        Option<&mut Inventory>,
        &crate::interact::Skills,
        Option<&mut crate::meals::KnownRecipes>,
        (Option<&mut crate::novels::Novels>, Option<&mut crate::skillbooks::SkillBooks>, Option<&crate::wishes::Wishes>, Has<crate::sim::HouseholdMember>),
    )>,
    (clicks, rows, lines, tabs, hovered): (
        Query<(Entity, &Interaction), Changed<Interaction>>,
        Query<(&Interaction, &StoreRow), Changed<Interaction>>,
        Query<(&Interaction, &CartLine), Changed<Interaction>>,
        Query<(&Interaction, &ShopTab), Changed<Interaction>>,
        Query<&Interaction>,
    ),
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut portraits: ResMut<crate::portraits::Portraits>,
    (mut notes, mut play): (ResMut<crate::interact::Notifications>, MessageWriter<crate::sound::PlaySound>),
) {
    let (Some(mut ui), Some(mut gu)) = (ui, gu) else { return };
    let data = gu.data.clone();
    let close = |commands: &mut Commands, d: &mut ShopDialog, clock: &mut crate::clock::GameClock| {
        if let Some(r) = d.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        if d.shopper.take().is_some() && clock.speed == 0 {
            clock.set_speed(d.speed.max(1));
        }
        d.rows.clear();
        d.tabs.clear();
        d.shown = None;
    };
    let Some(me) = d.shopper else { return };
    let Ok((sim, _, skills, known, (_, _, wishes, _))) = shoppers.get(me) else {
        close(&mut commands, &mut d, &mut clock);
        return;
    };
    let sim = sim.clone();
    let cooking = skills.level("Cooking");
    let known = known.cloned();
    let wishes = wishes.cloned();
    let store_kind = d.store;
    let tabs_shown = store_kind.tabs();
    // (The household's books, for the bookstore not to sell them again.)
    let novels = crate::novels::shelved(shoppers.iter().filter(|s| s.4.3).filter_map(|s| s.4.0));
    let skill_books = crate::skillbooks::shelved(shoppers.iter().filter(|s| s.4.3).filter_map(|s| s.4.1));
    // Put up, in the middle of the screen, the game paused.
    if d.s.is_none() {
        let Some(mut w) = ui.layout("Shop").cloned() else {
            warn!("no Shop layout");
            d.shopper = None;
            return;
        };
        let (wd, ht) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
        w.area = [0.0, 0.0, wd, ht];
        w.place = s3bake::ui::UiPlace::Center(0.5);
        w.flags |= s3bake::ui::WIN_VISIBLE;
        let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
        let Some(root) = s.root else { return };
        commands.entity(root).insert((GlobalZIndex(21), crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(AppState::InGame)));
        for id in NOT_HERE {
            for x in s.all_with(id) {
                commands.entity(x).insert(Visibility::Hidden);
            }
        }
        for t in [STORE_TABLE, CART_TABLE] {
            if let Some(e) = s.id(t) {
                commands.entity(e).remove::<Pickable>().insert(Interaction::default());
            }
        }
        if let Some(t) = s.id(SIM_THUMB) {
            let h = portraits.portrait(&mut images, me);
            commands.entity(t).despawn_children();
            commands.entity(t).with_children(|c| {
                c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(me), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
            });
            commands.entity(t).insert(crate::icons::Tooltip(sim.full_name()));
        }
        if let Some(e) = s.id(CLOSE) {
            commands.entity(e).insert(crate::icons::Tooltip("Close".into()));
        }
        // (The bookstore neither shops by recipe nor buys back.)
        if store_kind == Store::Books {
            for id in [BY_RECIPE, BY_SELL] {
                for x in s.all_with(id) {
                    commands.entity(x).insert(Visibility::Hidden);
                }
            }
        }
        // The table's headers: the game's column buttons.
        if let Some(col) = ui.layout("ColumnControl").cloned() {
            for (table, words, widths) in [(STORE_TABLE, vec!["Item", "Price"], STORE_COLUMNS.to_vec()), (CART_TABLE, vec!["Item", "Count", "Price"], CART_COLUMNS.to_vec())] {
                let Some(t) = s.id(table) else { continue };
                commands.entity(t).despawn_children();
                let mut x = 0.0;
                for (k, word) in words.iter().enumerate() {
                    let mut w = col.clone();
                    w.place = s3bake::ui::UiPlace::Fixed;
                    w.area = [x, 0.0, x + widths[k], HEADER];
                    w.caption = word.to_string();
                    ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, t);
                    x += widths[k];
                }
            }
        }
        // The buying tabs.
        if let Some(holder) = s.id(TABS) {
            let (font, line) = ui.text_font(&mut fonts, 0);
            commands.entity(holder).despawn_children();
            for (i, (word, _)) in tabs_shown.iter().enumerate() {
                let b = commands
                    .spawn((
                        Button,
                        ShopTab(i),
                        Node { position_type: PositionType::Absolute, left: Val::Px(i as f32 * 86.0), top: Val::Px(4.0), width: Val::Px(82.0), height: Val::Px(28.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border_radius: BorderRadius::top(Val::Px(6.0)), ..default() },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                        ChildOf(holder),
                    ))
                    .id();
                commands.spawn((Text::new(*word), font.clone(), line, TextColor(crate::layout::color(ROW_TEXT)), Pickable::IGNORE, ChildOf(b)));
                d.tabs.push(b);
            }
        }
        d.speed = clock.speed;
        clock.set_speed(0);
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
        d.s = Some(s);
    }
    let Some(s) = d.s.clone() else { return };
    let inv = shoppers.get(me).ok().and_then(|q| q.1.map(|i| i.clone()));
    // The buttons.
    let mut changed = false;
    if crate::livehud::pressed(&clicks, s.id(CLOSE)) {
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d, &mut clock);
        return;
    }
    for (id, mode) in [(BY_ITEM, Mode::Buy), (BY_RECIPE, Mode::Recipe), (BY_SELL, Mode::Sell)] {
        if crate::livehud::pressed(&clicks, s.id(id)) && d.mode != mode {
            d.mode = mode;
            d.picked = None;
            d.scroll = 0;
            changed = true;
        }
    }
    if let Some((_, t)) = tabs.iter().find(|(i, _)| **i == Interaction::Pressed) {
        d.tab = t.0;
        d.picked = None;
        d.scroll = 0;
        changed = true;
    }
    let shelves = Shelves { novels, skill_books, cooking, known: known.as_ref(), wishes: wishes.as_ref() };
    let store: Vec<Row> = match d.mode {
        Mode::Buy if store_kind == Store::Books => books_for_sale(&data, tabs_shown[d.tab.min(tabs_shown.len() - 1)].1, &shelves),
        Mode::Buy => for_sale(&data, tabs_shown[d.tab.min(tabs_shown.len() - 1)].1),
        Mode::Recipe => by_recipe(&data, |r| crate::meals::knows(r, cooking, known.as_ref())),
        Mode::Sell => to_sell(inv.as_ref()),
    };
    let selling = d.mode == Mode::Sell;
    // A row picked; picked again (or "Add to Cart"), added.
    let mut add: Option<usize> = None;
    if let Some((_, r)) = rows.iter().find(|(i, _)| **i == Interaction::Pressed) {
        if d.picked == Some(r.0) {
            add = Some(r.0);
        }
        d.picked = Some(r.0);
        changed = true;
    }
    if crate::livehud::pressed(&clicks, s.id(ADD)) {
        add = d.picked;
    }
    if let Some(row) = add.and_then(|i| store.get(i)) {
        let list = if selling { &mut d.sell } else { &mut d.buy };
        for l in &row.adds {
            let listed = list.iter().find(|(x, _)| x == l).map_or(0, |x| x.1);
            if sellable(inv.as_ref(), l, listed) {
                add_line(list, l.clone());
            }
        }
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        changed = true;
    }
    // A line clicked: one fewer of it.
    if let Some((_, c)) = lines.iter().find(|(i, _)| **i == Interaction::Pressed) {
        let list = if selling { &mut d.sell } else { &mut d.buy };
        if let Some((_, n)) = list.get_mut(c.0) {
            *n -= 1;
            if *n == 0 {
                list.remove(c.0);
            }
        }
        changed = true;
    }
    if crate::livehud::pressed(&clicks, s.id(CLEAR)) {
        if selling { d.sell.clear() } else { d.buy.clear() }
        changed = true;
    }
    let list = if selling { d.sell.clone() } else { d.buy.clone() };
    // (One of what there's a coupon for at the coupon's price.)
    let coupon = |l: &Line| match (selling, inv.as_ref(), l) {
        (false, Some(inv), Line::Buy(k)) => crate::coupons::held(inv, crate::coupons::GROCERY, k),
        (false, Some(inv), Line::Novel(k) | Line::SkillBook(k) | Line::RecipeBook(k)) => crate::coupons::held(inv, crate::coupons::BOOKSTORE, k),
        _ => None,
    };
    let priced: Vec<(String, u32, i64)> = list
        .iter()
        .filter_map(|(l, n)| {
            line_info(&data, inv.as_ref(), wishes.as_ref(), l).map(|(name, each)| match coupon(l) {
                Some((_, with)) => (format!("{name} (Coupon)"), *n, with + each * (*n as i64 - 1)),
                None => (name, *n, each * *n as i64),
            })
        })
        .collect();
    let used: Vec<usize> = list.iter().filter_map(|(l, _)| coupon(l).map(|c| c.0)).collect();
    let total: i64 = priced.iter().map(|p| p.2).sum();
    let funds = household.as_ref().map_or(0, |h| h.funds);
    // Purchase: paid, and in their inventory; or sold.
    if crate::livehud::pressed(&clicks, s.id(PURCHASE)) && !list.is_empty() {
        if selling {
            if let Ok((_, Some(mut inv), ..)) = shoppers.get_mut(me) {
                for (l, n) in &list {
                    if let Line::Sell(kind, k, q) = l
                        && let Some(i) = inv.0.iter().position(|s| s.kind == *kind && s.key == *k && s.quality == *q)
                    {
                        for _ in 0..*n {
                            if inv.0.get(i).is_some_and(|s| s.key == *k) {
                                inv.take_one(i);
                            }
                        }
                    }
                }
            }
            if let Some(h) = household.as_mut() {
                h.funds += total;
            }
            notes.push(format!("{} sold {} for §{total}.", sim.first, plural(list.iter().map(|l| l.1).sum())));
            d.sell.clear();
            play.write(crate::sound::PlaySound::ui("ui_object_sell"));
        } else if total > funds {
            notes.push(format!("{} can't afford that (§{total}).", sim.first));
        } else {
            if let Some(h) = household.as_mut() {
                h.funds -= total;
            }
            // (The coupons used, gone.)
            if !used.is_empty()
                && let Ok((_, Some(mut inv), ..)) = shoppers.get_mut(me)
            {
                let mut used = used.clone();
                used.sort_unstable();
                for i in used.into_iter().rev() {
                    inv.take_one(i);
                }
            }
            // (Books: on the household's shelves, a recipe's learned.)
            if let Ok((_, _, _, known, (novels, skill_books, _, _))) = shoppers.get_mut(me) {
                let mut novels = novels.as_deref().cloned().unwrap_or_default();
                let mut skill_books = skill_books.as_deref().cloned().unwrap_or_default();
                let mut known = known.as_deref().cloned().unwrap_or_default();
                for (l, _) in &list {
                    match l {
                        Line::Novel(k) => novels.own(k),
                        Line::SkillBook(k) if skill_books.pages(k).is_none() => skill_books.0.push((k.clone(), 0.0)),
                        Line::RecipeBook(k) if !known.0.contains(k) => known.0.push(k.clone()),
                        _ => {}
                    }
                }
                if store_kind == Store::Books {
                    commands.entity(me).insert((novels, skill_books, known));
                }
            }
            for (l, n) in &list {
                let Line::Buy(k) = l else { continue };
                let Some(i) = data.ingredients.iter().find(|i| i.key == *k) else { continue };
                let kind = if crate::ingredients::is_of_kind(&data, k, "Fish") { ItemKind::Fish } else { ItemKind::Produce };
                let tier = crate::ingredients::STORE_TIER;
                let word = crate::gardening::QUALITIES[tier as usize].0;
                crate::inventory::give(&mut commands, me, kind, k.clone(), format!("{word} {}", i.name), tier, i.price as i64, *n);
            }
            notes.push(format!("{} bought {} for §{total}.", sim.first, plural(list.iter().map(|l| l.1).sum())));
            d.buy.clear();
            play.write(crate::sound::PlaySound::ui("ui_object_buy"));
        }
        changed = true;
    }
    // (Scrolled by the wheel over the store's table.)
    let over = s.id(STORE_TABLE).and_then(|t| hovered.get(t).ok()).is_some_and(|i| *i != Interaction::None);
    if over && wheel.delta.y != 0.0 {
        let most = store.len().saturating_sub(STORE_ROWS);
        d.scroll = if wheel.delta.y > 0.0 { d.scroll.saturating_sub(1) } else { (d.scroll + 1).min(most) };
        changed = true;
    }
    let key = format!("{:?} {} {:?} {} {:?} {funds} {}", d.mode, d.tab, d.picked, d.scroll, list, inv.as_ref().map_or(0, |i| i.0.len()));
    if !changed && d.shown.as_ref() == Some(&key) {
        return;
    }
    d.shown = Some(key);
    // The words: the store's, the mode's, the cart's.
    let say = |commands: &mut Commands, id: u32, t: String| {
        if let Some(x) = s.text(id) {
            commands.entity(x).insert(Text::new(t));
        }
    };
    let loc = |k: &str, or: &str| ui.localize(k).unwrap_or_else(|| or.to_string());
    let (title, mode, add_word, buy_word, cart) = match d.mode {
        Mode::Buy => (tabs_shown[d.tab.min(tabs_shown.len() - 1)].0.to_string(), loc("Ui/Caption/Shopping:ByItem", "Shop"), loc("Ui/Caption/Shopping:AddToCart", "Add to Cart"), loc("Ui/Caption/Shopping/Cart:Buy", "Purchase"), loc("Ui/Caption/Shopping/Cart:ShoppingCart", "Shopping Cart")),
        Mode::Recipe => (loc("Ui/Caption/Shopping:Recipes", "Recipes"), loc("Ui/Caption/Shopping:ByRecipe", "Shop by Recipe"), loc("Ui/Caption/Shopping:AddToCart", "Add to Cart"), loc("Ui/Caption/Shopping/Cart:Buy", "Purchase"), loc("Ui/Caption/Shopping/Cart:ShoppingCart", "Shopping Cart")),
        Mode::Sell => (loc("Ui/Caption/Shopping:SellableItems", "Sellable Items"), loc("Ui/Caption/Shopping:Sell", "Sell"), loc("Ui/Caption/Shopping:AddToSellList", "Add to Sell List"), loc("Ui/Caption/Shopping/Cart:SellItems", "Sell Items"), loc("Ui/Caption/Shopping/Cart:ListToSell", "List to Sell")),
    };
    say(&mut commands, STORE_TITLE, title);
    say(&mut commands, MODE_TEXT, mode);
    say(&mut commands, STORE_NAME, if store_kind == Store::Books { "Bookstore" } else { "Grocery" }.to_string());
    say(&mut commands, CART_TITLE, cart);
    say(&mut commands, TOTAL, fill(&loc("Ui/Caption/Shopping/Cart:Total", "Total: {0}"), &format!("§{total}")));
    say(&mut commands, FUNDS, fill(&loc("Ui/Caption/Shopping/Cart:AvailableFunds", "Available Funds: {0}"), &format!("§{}", crate::lifetime::group(funds))));
    for (id, word) in [(ADD, add_word), (PURCHASE, buy_word)] {
        if let Some(b) = s.id(id)
            && let Some(t) = s.text_of(b)
        {
            commands.entity(t).insert(Text::new(word));
        }
    }
    // The mode's button and the tab shown chosen; tabs only to shop by item.
    let mode_now = d.mode;
    for (id, m) in [(BY_ITEM, Mode::Buy), (BY_RECIPE, Mode::Recipe), (BY_SELL, Mode::Sell), (CART_BUTTON, mode_now)] {
        if let Some(b) = s.id(id) {
            let on = m == mode_now;
            commands.entity(b).queue(move |mut e: EntityWorldMut| {
                if let Some(mut u) = e.get_mut::<crate::layout::UiButton>()
                    && u.selected != on
                {
                    u.selected = on;
                }
            });
        }
    }
    if let Some(h) = s.id(TABS) {
        commands.entity(h).insert(if d.mode == Mode::Buy { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (i, b) in d.tabs.iter().enumerate() {
        let a = if i == d.tab { 0.85 } else { 0.3 };
        commands.entity(*b).insert(BackgroundColor(Color::srgba(1.0, 1.0, 1.0, a)));
    }
    // The tables' rows, afresh.
    for e in d.rows.drain(..) {
        commands.entity(e).try_despawn();
    }
    let (font, line) = ui.text_font(&mut fonts, 0);
    let colour = crate::layout::color(ROW_TEXT);
    let mut tables = Vec::new();
    if let Some(t) = s.id(STORE_TABLE) {
        let width: f32 = STORE_COLUMNS.iter().sum();
        let holder = table_rows(&mut commands, t, width, STORE_ROWS);
        for (n, r) in store.iter().enumerate().skip(d.scroll).take(STORE_ROWS) {
            let bg = if d.picked == Some(n) { crate::layout::color(PICKED) } else if n % 2 == 1 { crate::layout::color(ALTERNATE) } else { Color::NONE };
            let row = commands.spawn((Button, StoreRow(n), Node { height: Val::Px(ROW), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), ChildOf(holder))).id();
            // (Its picture: the produce's or fish's own; a recipe's, its first ingredient's.)
            let shown = match r.adds.first() {
                Some(Line::Buy(k)) => {
                    let kind = if crate::ingredients::is_of_kind(&data, k, "Fish") { ItemKind::Fish } else { ItemKind::Produce };
                    Some(Stack { kind, key: k.clone(), name: String::new(), quality: 0, count: 1, worth: 0 })
                }
                Some(Line::Sell(kind, k, q)) => Some(Stack { kind: *kind, key: k.clone(), name: String::new(), quality: *q, count: 1, worth: 0 }),
                _ => None,
            };
            let picture = shown.and_then(|st| crate::inventory::stack_picture(&st, Some(&mut gu), &mut pictures, &mut images, baked.as_deref(), &mut thumbs));
            for (k, (w, words)) in STORE_COLUMNS.iter().zip([r.label.clone(), format!("§{}", r.price)]).enumerate() {
                let cell = commands.spawn((Node { width: Val::Px(*w), padding: UiRect::left(Val::Px(6.0)), align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
                if k == 0 {
                    let mut pic = commands.spawn((Node { width: Val::Px(ROW - 4.0), height: Val::Px(ROW - 4.0), flex_shrink: 0.0, ..default() }, Pickable::IGNORE, ChildOf(cell)));
                    if let Some((h, rect)) = picture.clone() {
                        pic.insert(ImageNode { image: h, rect, ..default() });
                    }
                }
                commands.spawn((Text::new(words), font.clone(), line, TextColor(colour), Pickable::IGNORE, ChildOf(cell)));
            }
        }
        tables.push(holder);
    }
    if let Some(t) = s.id(CART_TABLE) {
        let width: f32 = CART_COLUMNS.iter().sum();
        let holder = table_rows(&mut commands, t, width, CART_ROWS);
        for (n, (name, count, cost)) in priced.iter().enumerate().take(CART_ROWS) {
            let bg = if n % 2 == 1 { crate::layout::color(ALTERNATE) } else { Color::NONE };
            let row = commands.spawn((Button, CartLine(n), Node { height: Val::Px(ROW), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), crate::icons::Tooltip("Click to take one off".into()), ChildOf(holder))).id();
            for (w, words) in CART_COLUMNS.iter().zip([name.clone(), count.to_string(), format!("§{cost}")]) {
                let cell = commands.spawn((Node { width: Val::Px(*w), padding: UiRect::left(Val::Px(6.0)), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
                commands.spawn((Text::new(words), font.clone(), line, TextColor(colour), Pickable::IGNORE, ChildOf(cell)));
            }
        }
        tables.push(holder);
    }
    d.rows = tables;
}

/// A table's rows' holder, under its header.
fn table_rows(commands: &mut Commands, table: Entity, width: f32, rows: usize) -> Entity {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(HEADER), width: Val::Px(width), height: Val::Px(rows as f32 * ROW), flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() },
            Pickable::IGNORE,
            ChildOf(table),
        ))
        .id()
}

fn plural(n: u32) -> String {
    if n == 1 { "1 item".into() } else { format!("{n} items") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use s3bake::gamedata::IngredientInfo;

    fn data() -> GameDataBaked {
        let ing = |key: &str, parent: &str, tab: &str, price: i32, grown: bool| IngredientInfo { key: key.into(), name: key.into(), parent: parent.into(), kind: price == 0, price, can_buy: price > 0, grown, tab: tab.into(), fertilizer: None };
        GameDataBaked {
            ingredients: vec![ing("Ingredient", "ROOT", "", 0, false), ing("Vegetable", "Ingredient", "", 0, false), ing("Tomato", "Vegetable", "Produce", 3, true), ing("Onion", "Vegetable", "Produce", 7, true), ing("Hot Dogs", "Ingredient", "Meat", 8, false)],
            recipes: vec![s3bake::gamedata::RecipeInfo { key: "Stew".into(), name: "Stew".into(), ingredients: vec!["Vegetable".into(), "Hot Dogs".into()], ..Default::default() }],
            ..Default::default()
        }
    }

    #[test]
    fn the_grocery_sells_by_tab_and_recipe_and_buys_back() {
        let d = data();
        let produce = for_sale(&d, "Produce");
        assert_eq!(produce.iter().map(|r| (r.label.as_str(), r.price)).collect::<Vec<_>>(), vec![("Onion", 11), ("Tomato", 5)]);
        assert_eq!(for_sale(&d, "").len(), 3);
        // (A recipe: the cheapest vegetable and the hot dogs, together.)
        let r = by_recipe(&d, |_| true);
        assert_eq!((r[0].price, r[0].adds.len()), (13, 2));
        assert!(by_recipe(&d, |_| false).is_empty());
        let mut list = Vec::new();
        for l in r[0].adds.iter().chain(r[0].adds.iter()) {
            add_line(&mut list, l.clone());
        }
        assert_eq!(list, vec![(Line::Buy("Tomato".into()), 2), (Line::Buy("Hot Dogs".into()), 2)]);
        // (Sold at a tenth over their worth; no more than are held.)
        let mut inv = Inventory::default();
        inv.add(ItemKind::Produce, "Tomato", "Perfect Tomato", 9, 20);
        let sell = to_sell(Some(&inv));
        assert_eq!(sell[0].price, 22);
        assert!(sellable(Some(&inv), &sell[0].adds[0], 0) && !sellable(Some(&inv), &sell[0].adds[0], 1));
        assert_eq!(fill("Total: {0.Money}", "§5"), "Total: §5");
    }

    #[test]
    fn the_bookstore_sells_the_books_the_household_hasnt() {
        let mut d = data();
        let novel = |key: &str, name: &str, genre: &str, price: i64, in_store: bool| s3bake::gamedata::NovelInfo { key: key.into(), name: name.into(), genre: genre.into(), price, in_store, pages: 100, ..Default::default() };
        d.novels = vec![novel("AB", "Zed", "Fiction", 20, true), novel("CD", "Alpha", "Fiction", 30, true), novel("EF", "Handbook", "Nonfiction", 100, false)];
        // (Not the household's own, nor a boss's handbook the store hasn't.)
        let s = Shelves { novels: vec!["AB".into()], ..Default::default() };
        let rows = books_for_sale(&d, "General", &s);
        assert_eq!(rows.iter().map(|r| (r.label.as_str(), r.price)).collect::<Vec<_>>(), vec![("Alpha (Fiction)", 30)]);
        assert_eq!(rows[0].adds, vec![Line::Novel("CD".into())]);
        assert_eq!(line_info(&d, None, None, &rows[0].adds[0]), Some(("Alpha".to_string(), 30)));
        // (A book's bought the once.)
        assert!(sellable(None, &rows[0].adds[0], 0) && !sellable(None, &rows[0].adds[0], 1));
        assert_eq!(Store::Books.tabs().len(), 3);
    }
}
