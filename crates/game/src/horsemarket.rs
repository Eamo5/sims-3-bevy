//! The Equestrian Center's horses for sale, the Pets pack's (`EquestrianCenter.BuyHorse`,
//! `PetAdoption.BuyHorse` and UI.dll's `BuyHorseDialog` over its own layout). "Buy Horse" on the
//! center's menu (greyed with the game's words where the household has no room for another pet)
//! takes the Sim there; inside, the game's dialog shows the horses for sale, each its picture and
//! name (`HudPetAdoptionEntry`) and, pointed at, its breed, traits and price. One picked (Accept
//! greyed where the household can't afford it: "You don't have enough funds to buy this horse."),
//! the game asks if they're sure at its price, then for its name ("Name Your Horse"); bought, the
//! price is paid and the horse comes home with them, fonder of its buyer
//! (`kRelationshipBoostOnBuy`).
//!
//! The horses for sale are the center's pool (`PetPool`'s `BuySellHorse`: four to eight in
//! Appaloosa Plains, topped up each day), grown horses of the breeds Create a Pet lists, two to
//! four traits each drawn with the agile, fast and genius ones half as likely again
//! (`EquestrainHorseCreator`), priced as the game prices a horse bought
//! (`HorseManager.GetHorseCost`).

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{Household, Notifications};
use crate::layout::{Spawned, UiAssets, UiButton};
use crate::rabbitholes::{Activity, AtRabbitHole};
use crate::sim::Sim;

pub struct HorseMarketPlugin;

impl Plugin for HorseMarketPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HorsePool>()
            .init_resource::<Shop>()
            .add_systems(Update, (buy_test, restock, open_shop, shop_dialog, answers, done).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(crate::AppState::InGame), |mut s: ResMut<Shop>, mut p: ResMut<HorsePool>| {
                *s = Shop::default();
                *p = HorsePool::default();
            });
    }
}

/// The visit's activity number, and the visit (`kBuyHorseInteractionLength`).
pub const BUY_HORSE_TASK: usize = 7300;
pub static BUY_HORSE: Activity = crate::rabbitholes::act("Buy Horse", 5.0, 0, [0.0; 6], None);

/// The pool's size (`PetPool`'s `BuySellHorse` in the pack's own town), its traits (two to four,
/// the favoured ones weighted half as likely again: `kMaxNumberOfTraits`,
/// `kMultiplierForBuyHorsePoolTraits`, `kBuyHorseTraits`), and the buyer's start with it
/// (`kRelationshipBoostOnBuy`).
const POOL: (usize, usize) = (4, 8);
const TRAITS: (usize, usize) = (2, 4);
const FAVOURED: [&str; 3] = ["AgilePet", "FastPet", "GeniusPet"];
const FAVOURED_WEIGHT: f32 = 1.5;
const BOOST_ON_BUY: f32 = 10.0;

/// A horse for sale.
#[derive(Clone, Debug)]
pub struct ForSale {
    pub name: String,
    pub female: bool,
    /// Its breed (an index into the pets' data's breeds).
    pub breed: usize,
    pub traits: Vec<String>,
    pub price: i64,
}

/// The center's horses for sale, and the day they were last topped up.
#[derive(Resource, Default)]
pub struct HorsePool {
    pub day: Option<u32>,
    pub horses: Vec<ForSale>,
}

/// A horse for the pool: a listed breed, grown, its traits, its price to buy.
fn new_horse(data: &s3bake::pets::PetsBaked, gu: Option<&crate::icons::GameUi>, rng: &mut impl Rng) -> Option<ForSale> {
    let breeds: Vec<usize> = data.breeds.iter().enumerate().filter(|(_, b)| b.kind == "ah" && b.listed.is_some()).map(|(i, _)| i).collect();
    let breed = *breeds.get(rng.random_range(0..breeds.len().max(1)))?;
    let n = rng.random_range(TRAITS.0..=TRAITS.1);
    let traits: Vec<String> = crate::pettraits::weighted("ah", n, &FAVOURED, FAVOURED_WEIGHT, rng).into_iter().map(str::to_string).collect();
    let scores: Vec<i32> = traits.iter().filter_map(|t| gu.and_then(|g| g.data.traits.iter().find(|x| x.hex == *t)).map(|x| x.score)).collect();
    let price = crate::equestrian::horse_cost(0, 0, &scores, rng.random(), true);
    Some(ForSale { name: crate::pets::random_pet_name(rng), female: rng.random(), breed, traits, price })
}

/// The pool topped up each day (to between four and eight), as the game's pools are refreshed.
fn restock(clock: Res<crate::clock::GameClock>, pets: Res<crate::pets::PetData>, gu: Option<Res<crate::icons::GameUi>>, mut pool: ResMut<HorsePool>) {
    let day = clock.day();
    if pool.day == Some(day) {
        return;
    }
    let Some(data) = pets.0.as_deref() else { return };
    let mut rng = rand::rng();
    pool.day = Some(day);
    if pool.horses.len() < POOL.0 {
        let want = rng.random_range(POOL.0..=POOL.1);
        while pool.horses.len() < want {
            let Some(h) = new_horse(data, gu.as_deref(), &mut rng) else { break };
            pool.horses.push(h);
        }
    }
    info!("horse market: {} horses for sale", pool.horses.len());
}

/// "Buy Horse" on the center's menu: greyed where there's no room for another pet.
pub fn option(ui: Option<&UiAssets>, lot: usize, pets: usize, household: usize) -> (String, crate::interact::ActionKind) {
    const KEY: &str = "Gameplay/Objects/RabbitHoles/EquestrianCenter/BuyHorse:";
    let label = words(ui, &format!("{KEY}InteractionName"), "Buy Horse");
    if pets >= crate::cap::MAX_PETS || household >= crate::cap::MAX_IN_HOUSEHOLD {
        return (label, crate::interact::ActionKind::Greyed(words(ui, &format!("{KEY}TooManyPetsInHousehold"), "There is no space in the household to buy a new horse.")));
    }
    (label, crate::interact::ActionKind::Visit { lot, activity: BUY_HORSE_TASK })
}

fn words(ui: Option<&UiAssets>, key: &str, or: &str) -> String {
    ui.and_then(|u| u.localize(key)).filter(|s| !s.is_empty()).unwrap_or_else(|| or.to_string())
}

/// A Sim at the center to buy a horse: whether the dialog's been put up.
#[derive(Component, Default)]
pub struct BuyingHorse {
    opened: bool,
}

/// The dialog: who's buying, its windows, the horse picked, the entries.
#[derive(Resource, Default)]
pub struct Shop {
    pub sim: Option<Entity>,
    s: Option<Spawned>,
    picked: Option<usize>,
    cells: Vec<Entity>,
    shown: Option<(Option<usize>, i64)>,
}

/// An entry of the grid: which horse.
#[derive(Component)]
struct HorseCell(usize);

/// Once they're in, the dialog up (or, with no horses for sale, the game's words for it).
fn open_shop(clock: Res<crate::clock::GameClock>, mut sims: Query<(Entity, &AtRabbitHole, &mut BuyingHorse)>, mut shop: ResMut<Shop>, pool: Res<HorsePool>, ui: Option<Res<UiAssets>>, mut notes: ResMut<Notifications>) {
    for (e, at, mut b) in &mut sims {
        if b.opened || clock.minutes < at.inside_from || shop.sim.is_some() {
            continue;
        }
        b.opened = true;
        if pool.horses.is_empty() {
            notes.push(words(ui.as_deref(), "Gameplay/Objects/RabbitHoles/EquestrianCenter/BuyHorse:PoolEmpty", "There aren't any horses currently for sale, try checking back tomorrow."));
            continue;
        }
        *shop = Shop { sim: Some(e), ..default() };
    }
}

// `BuyHorseDialog`'s windows (`ControlIDs`), and its entries' (`HudPetAdoptionEntry`).
const CAPTION: u32 = 0x05ef_6bd4;
const ACCEPT: u32 = 0x0bdb_e520;
const CANCEL: u32 = 0x0bdb_9fa0;
const GRID: u32 = 0x0bdb_f8b0;
const ENTRY_THUMB: u32 = 0x0c0f_44a0;
const ENTRY_NAME: u32 = 3;
/// The grid's entries: their size, how many across.
const CELL: Vec2 = Vec2::new(145.0, 190.0);
const ACROSS: usize = 4;
/// The picked entry's light.
const PICKED: Color = Color::srgba(0.45, 0.75, 1.0, 0.45);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shop_dialog(
    mut commands: Commands,
    mut shop: ResMut<Shop>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (pool, pets, gu, household): (Res<HorsePool>, Res<crate::pets::PetData>, Option<Res<crate::icons::GameUi>>, Option<Res<Household>>),
    (clicks, cells, mut buttons, mut backgrounds): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &HorseCell), Changed<Interaction>>, Query<&mut UiButton>, Query<&mut BackgroundColor>),
    mut questions: ResMut<crate::dialog::Questions>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let Some(sim) = shop.sim else { return };
    let close = |commands: &mut Commands, shop: &mut Shop| {
        if let Some(r) = shop.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        *shop = Shop::default();
    };
    let Some(data) = pets.0.as_deref() else { return };
    if shop.s.is_none() {
        let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "HUDBuyHorseDialog") else {
            warn!("no HUDBuyHorseDialog layout");
            shop.sim = None;
            return;
        };
        if let Some(t) = s.text(CAPTION) {
            commands.entity(t).insert(Text::new(words(Some(&*ui), "Ui/Caption/HUD/BuyHorseDialog:BuyHorseName", "Buy Horse")));
        }
        if let Some(c) = s.id(CANCEL) {
            commands.entity(c).insert(crate::icons::Tooltip(words(Some(&*ui), "Ui/Caption/Global:Cancel", "Cancel")));
        }
        // The horses: each the game's entry, its breed's picture and its name, its details on the
        // pointer.
        if let (Some(grid), Some(entry)) = (s.id(GRID), ui.export("HudPetAdoptionEntry", 1).cloned()) {
            commands.entity(grid).despawn_children().queue(|mut ew: EntityWorldMut| {
                if let Some(mut n) = ew.get_mut::<Node>() {
                    n.overflow = Overflow::clip();
                }
            });
            let trait_words = |t: &String| crate::pettraits::name(t, gu.as_deref());
            for (i, h) in pool.horses.iter().enumerate() {
                let mut w = entry.clone();
                let (col, row) = ((i % ACROSS) as f32, (i / ACROSS) as f32);
                w.place = s3bake::ui::UiPlace::Fixed;
                w.area = [col * CELL.x, row * CELL.y, (col + 1.0) * CELL.x, (row + 1.0) * CELL.y];
                let e = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, grid);
                let Some(root) = e.root else { continue };
                let breed = data.breeds.get(h.breed);
                let tip = format!(
                    "{}\n{}{}\n{}\n§{}",
                    h.name,
                    breed.map_or("Horse".to_string(), |b| b.name.clone()),
                    if h.female { " (female)" } else { " (male)" },
                    h.traits.iter().map(trait_words).collect::<Vec<_>>().join(", "),
                    h.price
                );
                commands.entity(root).insert((Button, Interaction::default(), HorseCell(i), BackgroundColor(Color::NONE), crate::icons::Tooltip(tip)));
                if let Some(t) = e.id(ENTRY_NAME).and_then(|n| e.text_of(n)) {
                    commands.entity(t).insert(Text::new(h.name.clone()));
                }
                if let (Some(win), Some(b)) = (e.id(ENTRY_THUMB), breed)
                    && let Some(img) = breed_picture(&mut images, b)
                {
                    commands.entity(win).despawn_children();
                    commands.spawn((ImageNode::new(img), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE, ChildOf(win)));
                }
                shop.cells.push(root);
            }
        }
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
        shop.s = Some(s);
    }
    let Some(s) = shop.s.clone() else { return };
    if let Some((_, c)) = cells.iter().find(|(i, _)| **i == Interaction::Pressed)
        && shop.picked != Some(c.0)
    {
        shop.picked = Some(c.0);
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
    }
    let funds = household.as_ref().map_or(0, |h| h.funds);
    let picked = shop.picked.and_then(|i| pool.horses.get(i).map(|h| (i, h.price)));
    if crate::livehud::pressed(&clicks, s.id(CANCEL)) {
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut shop);
        return;
    }
    if crate::livehud::pressed(&clicks, s.id(ACCEPT))
        && let Some((i, price)) = picked.filter(|(_, p)| *p <= funds)
    {
        // (Sure? at its price; then its name.)
        let text = words(Some(&*ui), "Ui/Feedback/HUD/BuyHorseDialog:BuyHorseConfirm", "Are you sure you want to buy this horse for {0.Money}? Horses need a lot of attention and care!").replace("{0.Money}", &format!("§{price}"));
        let (yes, no) = (words(Some(&*ui), "Ui/Caption/Global:Yes", "Yes"), words(Some(&*ui), "Ui/Caption/Global:No", "No"));
        questions.ask(crate::dialog::Ask {
            about: crate::dialog::Question::BuyHorse { sim, horse: i, price },
            icon: String::new(),
            heading: String::new(),
            title: String::new(),
            text,
            answers: vec![crate::dialog::Answer { label: yes, detail: String::new(), icon: String::new() }, crate::dialog::Answer { label: no, detail: String::new(), icon: String::new() }],
        });
        close(&mut commands, &mut shop);
        return;
    }
    if shop.shown == Some((shop.picked, funds)) {
        return;
    }
    shop.shown = Some((shop.picked, funds));
    for (k, e) in shop.cells.iter().enumerate() {
        if let Ok(mut bg) = backgrounds.get_mut(*e) {
            bg.0 = if shop.picked == Some(k) { PICKED } else { Color::NONE };
        }
    }
    if let Some(a) = s.id(ACCEPT) {
        let can = picked.is_some_and(|(_, p)| p <= funds);
        if let Ok(mut b) = buttons.get_mut(a)
            && b.disabled == can
        {
            b.disabled = !can;
        }
        let tip = if picked.is_some() && !can { words(Some(&*ui), "Ui/Caption/HUD/BuyHorseDialog:NotEnoughFundsBuyTooltip", "You don't have enough funds to buy this horse.") } else { words(Some(&*ui), "Ui/Caption/Global:Accept", "Accept") };
        commands.entity(a).insert(crate::icons::Tooltip(tip));
    }
}

/// A breed's picture (the game's, as baked).
fn breed_picture(images: &mut Assets<Image>, breed: &s3bake::pets::PetBreed) -> Option<Handle<Image>> {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let (w, h, px) = s3bake::gamedata::decode_icon(&breed.thumb)?;
    Some(images.add(Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)))
}

/// Sure: its name asked; named: bought, paid for, and home with them.
#[allow(clippy::type_complexity)]
fn answers(
    mut answers: MessageReader<crate::dialog::Answered>,
    (mut questions, typed): (ResMut<crate::dialog::Questions>, Res<crate::dialog::Typed>),
    (mut pool, pets): (ResMut<HorsePool>, Res<crate::pets::PetData>),
    sims: Query<&Sim>,
    (mut household, mut pending, ui): (Option<ResMut<Household>>, ResMut<crate::pets::PendingPets>, Option<Res<UiAssets>>),
    (mut notes, mut play): (ResMut<Notifications>, MessageWriter<crate::sound::PlaySound>),
) {
    for a in answers.read() {
        match &a.about {
            crate::dialog::Question::BuyHorse { sim, horse, .. } if a.answer == 0 => {
                let Some(h) = pool.horses.get(*horse) else { continue };
                questions.ask(crate::dialog::Ask {
                    about: crate::dialog::Question::NameHorse { sim: *sim, horse: *horse, suggested: h.name.clone() },
                    icon: String::new(),
                    heading: String::new(),
                    title: words(ui.as_deref(), "Gameplay/Actors/Sim/BuyHorse:BuyHorseNameTitle", "Name Your Horse"),
                    text: words(ui.as_deref(), "Gameplay/Actors/Sim/BuyHorse:BuyHorseNameDescription", "Name your new Horse:"),
                    answers: vec![crate::dialog::Answer { label: "OK".into(), detail: String::new(), icon: String::new() }],
                });
            }
            crate::dialog::Question::NameHorse { sim, horse, suggested } => {
                let (Some(h), Some(data)) = (pool.horses.get(*horse).cloned(), pets.0.as_deref()) else { continue };
                let Some(hh) = household.as_deref_mut().filter(|hh| hh.funds >= h.price) else { continue };
                let Some(breed) = data.breeds.get(h.breed) else { continue };
                hh.funds -= h.price;
                pool.horses.remove(*horse);
                let name = Some(typed.0.trim()).filter(|n| !n.is_empty()).map_or_else(|| suggested.clone(), str::to_string);
                let buyer = sims.get(*sim).ok();
                let bonds = buyer.map(|s| vec![(s.id, crate::pets::FAMILY_LIKING + BOOST_ON_BUY)]).unwrap_or_default();
                let mut rng = rand::rng();
                pending.0.push(crate::pets::SavedPet { traits: h.traits.clone(), female: h.female, bonds, ..crate::pets::SavedPet::new(rng.random(), name.clone(), "ah", breed.outfit) });
                play.write(crate::sound::PlaySound::ui("sting_pet_adopt"));
                notes.push(format!("{} bought {name} for §{}. {name} is on {} way home.", buyer.map_or("They".to_string(), |s| s.first.clone()), h.price, if h.female { "her" } else { "his" }));
                info!("horse market: {name} bought for §{}", h.price);
            }
            _ => {}
        }
    }
}

/// `BUY_HORSE_TEST=1` (tests): ten seconds in, the selected Sim off to the Equestrian Center to buy
/// a horse.
fn buy_test(time: Res<Time>, mut sel: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>, world: Res<crate::loading::CurrentWorld>, mut done: Local<bool>) {
    if *done || std::env::var_os("BUY_HORSE_TEST").is_none() || time.elapsed_secs() < 10.0 {
        return;
    }
    let Ok(mut q) = sel.single_mut() else { return };
    *done = true;
    let Some(lot) = (0..world.data.lots.len()).find(|l| crate::equestrian::is_center(&world.data, *l)) else { return };
    info!("buy horse test: off to buy a horse");
    q.push_player(crate::interact::Action::new("Buy Horse", crate::interact::ActionKind::Visit { lot, activity: BUY_HORSE_TASK }, false));
}

/// Done at the center: the visit's marker gone once they're home.
fn done(mut commands: Commands, sims: Query<Entity, (With<BuyingHorse>, Without<AtRabbitHole>)>) {
    for e in &sims {
        commands.entity(e).remove::<BuyingHorse>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_horses_have_their_traits_drawn_as_the_games() {
        let mut rng = rand::rng();
        for _ in 0..50 {
            let t = crate::pettraits::weighted("ah", 4, &FAVOURED, FAVOURED_WEIGHT, &mut rng);
            assert_eq!(t.len(), 4);
            assert!(t.iter().all(|h| crate::pettraits::for_kind("ah").contains(h)), "horse traits only");
            assert!(!(t.contains(&"AgilePet") && t.contains(&"HatesJumpingPet")), "no opposites");
            let mut d = t.clone();
            d.dedup();
            assert_eq!(d.len(), t.len());
        }
    }

    #[test]
    fn there_must_be_room_for_another_pet() {
        let (_, k) = option(None, 3, crate::cap::MAX_PETS, 8);
        assert!(matches!(k, crate::interact::ActionKind::Greyed(ref w) if w.contains("no space")));
        let (label, k) = option(None, 3, 2, 5);
        assert_eq!(label, "Buy Horse");
        assert!(matches!(k, crate::interact::ActionKind::Visit { lot: 3, activity: BUY_HORSE_TASK }));
    }
}