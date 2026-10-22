//! The game's traits picker (UI.dll's `TraitsPickerDialog` over its `TraitsPicker` layout), for
//! Create a Sim's Personality tab and for the traits chosen in play (a Mid-Life Crisis, a new
//! trait at a birthday): the Sim's traits in their slots, the traits to take by group (all,
//! mental, physical, social, lifestyle) in the game's rows, scrolled by the wheel or the grid's
//! bar; a row chosen with a click and taken or given back with a second (or Add and Erase), a
//! random set, and Accept or Cancel. Traits that may not change stay in their slots. Create a
//! Pet's character panel puts it up for a pet's traits (the pack's, for its species), as the game
//! does.

use bevy::prelude::*;

use crate::layout::{Spawned, UiAssets, UiButton, UiScrollBar, edit_windows};
use crate::life::Trait;
use crate::sim::Age;

// `TraitsPicker`'s windows.
const PICK_CURRENT_BG: u32 = 0x06e0_a002;
const PICK_CURRENT: u32 = 4;
const PICK_AVAILABLE: u32 = 7;
const PICK_FILTERS: [u32; 5] = [8, 9, 0xb, 0xa, 0xe];
const PICK_ACCEPT: u32 = 0xd;
const PICK_CANCEL: u32 = 0xc;
const PICK_ADD: u32 = 6;
const PICK_ERASE: u32 = 5;
const PICK_RANDOM: u32 = 0x10;
const PICK_THUMB: u32 = 0xf;
const PICK_NAME: u32 = 2;
// `CASTraitEntry`'s.
const ENTRY_TEXT: u32 = 0x22;
const ENTRY_ICON: u32 = 0x20;
const AVAILABLE_ROWS: usize = 6;
// The item grid's own (layout `ItemGrid`): its scroll bar.
const GRID_SCROLLBAR: u32 = 0x0582_0672;

/// Whether the game's picker can be put up.
pub fn available(ui: &UiAssets) -> bool {
    ui.layout("TraitsPicker").is_some() && ui.layout("CASTraitEntry").is_some()
}

/// A trait in the picker: a Sim's, or a pet's (by the game's name for it).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    Sim(Trait),
    Pet(&'static str),
}

impl Pick {
    fn info(self, gu: Option<&crate::icons::GameUi>) -> Option<s3bake::gamedata::TraitInfo> {
        match self {
            Pick::Sim(t) => gu?.trait_info(t),
            Pick::Pet(h) => gu?.data.traits.iter().find(|i| i.hex == h).cloned(),
        }
    }

    fn name(self) -> String {
        match self {
            Pick::Sim(t) => t.name().to_string(),
            Pick::Pet(h) => h.trim_end_matches("Pet").to_string(),
        }
    }
}

/// Whose traits are picked: a Sim's of an age, or a pet's of a kind.
#[derive(Clone, Copy)]
enum Pool {
    Sim(Age),
    Pet(&'static str),
}

impl Pool {
    /// As many as they can have (the game's `GetMaxTraitsFor`: a young pet three, a grown one
    /// five).
    fn slots(self) -> usize {
        match self {
            Pool::Sim(age) => crate::life::trait_slots(age),
            Pool::Pet(kind) => crate::pettraits::max_traits(kind),
        }
    }

    fn all(self) -> Vec<Pick> {
        match self {
            Pool::Sim(age) => Trait::ALL.iter().filter(|t| t.allowed_at(age)).map(|t| Pick::Sim(*t)).collect(),
            Pool::Pet(kind) => crate::pettraits::for_kind(kind).into_iter().map(Pick::Pet).collect(),
        }
    }

    fn can_take(self, list: &[Pick], p: Pick) -> bool {
        if list.contains(&p) || list.len() >= self.slots() {
            return false;
        }
        match (self, p) {
            (Pool::Sim(age), Pick::Sim(t)) => {
                let sims: Vec<Trait> = list.iter().filter_map(|x| if let Pick::Sim(t) = x { Some(*t) } else { None }).collect();
                t.allowed_at(age) && t.compatible(&sims)
            }
            (Pool::Pet(kind), Pick::Pet(h)) => {
                let pets: Vec<&str> = list.iter().filter_map(|x| if let Pick::Pet(h) = x { Some(*h) } else { None }).collect();
                crate::pettraits::for_kind(kind).contains(&h) && crate::pettraits::compatible(&pets, h)
            }
            _ => false,
        }
    }

    fn random(self, rng: &mut impl rand::Rng) -> Vec<Pick> {
        match self {
            Pool::Sim(age) => crate::life::random_traits(rng, age).into_iter().map(Pick::Sim).collect(),
            Pool::Pet(kind) => crate::pettraits::random_up_to(kind, self.slots(), rng).into_iter().map(Pick::Pet).collect(),
        }
    }
}

/// The picker up: the traits as they're being chosen (taken on Accept), those that may not
/// change, whose they are, the group shown, the row chosen (taken or not) and when it was
/// clicked, and the rows in view.
pub struct Picker {
    pub s: Spawned,
    traits: Vec<Pick>,
    locked: Vec<Pick>,
    pool: Pool,
    group: usize,
    chosen: Option<(bool, Pick)>,
    clicked: Option<(bool, Pick, f32)>,
    scroll: usize,
    current: Option<Entity>,
    available: Option<Entity>,
    bar: Option<Entity>,
    dirty: bool,
}

/// What became of the picker this frame.
pub enum Outcome {
    Open,
    Accepted(Vec<Trait>),
    Cancelled,
}

/// A trait's row (taken, or to take).
#[derive(Component)]
pub struct TraitRow(bool, Pick);

/// The picker put up for a Sim (by name, of an age, with their traits; `locked` stay), at
/// `area`'s top left (or centred), its root given `extra`.
#[allow(clippy::too_many_arguments)]
pub fn open(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, name: &str, (age, portrait): (Age, Option<Handle<Image>>), traits: &[Trait], locked: &[Trait], at: Option<Vec2>, extra: impl Bundle) -> Option<Picker> {
    let picks = |l: &[Trait]| l.iter().map(|t| Pick::Sim(*t)).collect::<Vec<_>>();
    open_for(commands, ui, images, fonts, name, (Pool::Sim(age), portrait), (picks(traits), picks(locked)), at, extra)
}

/// The picker put up for a pet (by name, of a kind, with its traits: the game's names for them).
#[allow(clippy::too_many_arguments)]
pub fn open_pet(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, name: &str, (kind, portrait): (&str, Option<Handle<Image>>), traits: &[String], at: Option<Vec2>, extra: impl Bundle) -> Option<Picker> {
    let kind = crate::pettraits::KINDS.iter().copied().find(|k| *k == kind)?;
    let picks = traits.iter().filter_map(|h| crate::pettraits::PET_TRAITS.iter().find(|p| p.hex == h).map(|p| Pick::Pet(p.hex))).collect();
    open_for(commands, ui, images, fonts, name, (Pool::Pet(kind), portrait), (picks, Vec::new()), at, extra)
}

#[allow(clippy::too_many_arguments)]
fn open_for(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, name: &str, (pool, portrait): (Pool, Option<Handle<Image>>), (traits, locked): (Vec<Pick>, Vec<Pick>), at: Option<Vec2>, extra: impl Bundle) -> Option<Picker> {
    let mut w = ui.layout("TraitsPicker")?.clone();
    let (wd, ht) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
    match at {
        Some(p) => {
            w.area = [p.x, p.y, p.x + wd, p.y + ht];
            w.place = s3bake::ui::UiPlace::Fixed;
        }
        None => {
            w.area = [0.0, 0.0, wd, ht];
            w.place = s3bake::ui::UiPlace::Center(0.5);
        }
    }
    w.flags |= s3bake::ui::WIN_VISIBLE;
    let s = ui.spawn_root(commands, images, fonts, &w);
    let root = s.root?;
    commands.entity(root).insert((crate::hud::BlocksWorld, Interaction::default(), extra));
    for (id, tip) in [(PICK_FILTERS[0], "All"), (PICK_FILTERS[1], "Mental"), (PICK_FILTERS[2], "Physical"), (PICK_FILTERS[3], "Social"), (PICK_FILTERS[4], "Lifestyle"), (PICK_ACCEPT, "Accept"), (PICK_CANCEL, "Cancel")] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    if let Some(e) = s.text(PICK_NAME) {
        commands.entity(e).insert(Text::new(name.to_string()));
    }
    // The Sim's picture (their age's figure without one).
    if let (Some(e), Some(h)) = (s.id(PICK_THUMB), portrait.clone()) {
        crate::hudpanels::picture(commands, e, h, Color::WHITE);
    } else if let (Some(e), Pool::Sim(age)) = (s.id(PICK_THUMB), pool)
        && let Some((h, _)) = ui.image(images, s3pkg::fnv64(crate::caslook::AGE_ICONS[crate::caslook::age_icon(age) as usize]))
    {
        crate::hudpanels::picture(commands, e, h, crate::layout::color(0xff16_3996));
    }
    // The traits to take: the item grid's own scroll bar down its right.
    let mut bar = None;
    if let (Some(grid), Some(mut b)) = (s.id(PICK_AVAILABLE), ui.find("ItemGrid", GRID_SCROLLBAR).cloned()) {
        b.flags |= s3bake::ui::WIN_VISIBLE;
        b.place = s3bake::ui::UiPlace::Fixed;
        b.area = [218.0, 0.0, 236.0, 160.0];
        bar = ui.spawn_under(commands, images, fonts, &b, grid).root;
        commands.entity(grid).insert(bevy::ui::RelativeCursorPosition::default());
    }
    Some(Picker { s, traits, locked, pool, group: 0, chosen: None, clicked: None, scroll: 0, current: None, available: None, bar, dirty: true })
}

impl Picker {
    /// A pet's traits as chosen (the game's names).
    pub fn pet_traits(&self) -> Vec<String> {
        self.traits.iter().filter_map(|p| if let Pick::Pet(h) = p { Some(h.to_string()) } else { None }).collect()
    }

    /// The picker put away.
    pub fn close(&self, commands: &mut Commands) {
        if let Some(r) = self.s.root {
            commands.entity(r).try_despawn();
        }
    }
}

/// The game's group of a trait (1 mental, 2 physical, 3 social, 4 lifestyle), by its category.
fn trait_group(category: &str) -> usize {
    let c = category.to_ascii_lowercase();
    if c.contains("ment") || c.contains("intel") {
        1
    } else if c.contains("phys") {
        2
    } else if c.contains("soc") {
        3
    } else if c.contains("life") {
        4
    } else {
        0
    }
}

/// The picker's clicks, wheel and drawing for a frame: choosing, adding and taking away, a
/// random set, the groups and scrolling; Accept or Cancel ends it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update(
    p: &mut Picker,
    commands: &mut Commands,
    ui: &mut UiAssets,
    (images, fonts): (&mut Assets<Image>, &mut Assets<Font>),
    (clicks, rows, over): (&Query<(Entity, &Interaction), Changed<Interaction>>, &Query<(&Interaction, &TraitRow), Changed<Interaction>>, &Query<&bevy::ui::RelativeCursorPosition>),
    (wheel, now): (&bevy::input::mouse::AccumulatedMouseScroll, f32),
    (buttons, bars): (&mut Query<&mut UiButton>, &mut Query<&mut UiScrollBar>),
    play: &mut MessageWriter<crate::sound::PlaySound>,
    mut gu: Option<&mut crate::icons::GameUi>,
) -> Outcome {
    // (What was clicked: the current traits' buttons are the shown copy's of three alike.)
    let pressed = |id: u32| crate::livehud::pressed(clicks, p.s.id(id));
    let current_bg = p.s.id(PICK_CURRENT_BG);
    let in_bg = |id: u32| current_bg.and_then(|bg| p.s.within(bg, id));
    let (add_pressed, erase_pressed, random_pressed) = (crate::livehud::pressed(clicks, in_bg(PICK_ADD)), crate::livehud::pressed(clicks, in_bg(PICK_ERASE)), crate::livehud::pressed(clicks, in_bg(PICK_RANDOM)));
    let (accept, cancel) = (pressed(PICK_ACCEPT), pressed(PICK_CANCEL));
    let group_pressed = PICK_FILTERS.iter().position(|id| pressed(*id));
    let current_grid = in_bg(PICK_CURRENT);
    let pool = p.pool;
    let slots = pool.slots();
    let can_take = |list: &[Pick], t: Pick| pool.can_take(list, t);
    let add = |p: &mut Picker, t: Pick, play: &mut MessageWriter<crate::sound::PlaySound>| {
        if can_take(&p.traits, t) {
            p.traits.push(t);
            play.write(crate::sound::PlaySound::ui("ui_cas_trait_add"));
        }
        p.chosen = None;
        p.dirty = true;
    };
    let remove = |p: &mut Picker, t: Pick, play: &mut MessageWriter<crate::sound::PlaySound>| {
        if p.locked.contains(&t) {
            return;
        }
        p.traits.retain(|x| *x != t);
        play.write(crate::sound::PlaySound::ui("ui_cas_trait_remove"));
        p.chosen = None;
        p.dirty = true;
    };
    // A row: chosen with a click, added or taken away with a second (or a double click).
    for (i, r) in rows {
        if *i != Interaction::Pressed {
            continue;
        }
        let again = p.chosen == Some((r.0, r.1)) || p.clicked.is_some_and(|(taken, t, at)| (taken, t) == (r.0, r.1) && now - at < 0.4);
        p.clicked = Some((r.0, r.1, now));
        if again {
            if r.0 || p.traits.contains(&r.1) {
                remove(p, r.1, play);
            } else {
                add(p, r.1, play);
            }
        } else {
            p.chosen = Some((r.0, r.1));
            p.dirty = true;
        }
    }
    if add_pressed && let Some((false, t)) = p.chosen {
        add(p, t, play);
    }
    if erase_pressed && let Some((true, t)) = p.chosen {
        remove(p, t, play);
    }
    if random_pressed {
        // (The locked ones kept, the rest drawn at random around them.)
        let mut rng = rand::rng();
        let mut list = p.locked.clone();
        for t in pool.random(&mut rng) {
            if can_take(&list, t) {
                list.push(t);
            }
        }
        p.traits = list;
        p.chosen = None;
        p.dirty = true;
    }
    if let Some(g) = group_pressed
        && p.group != g
    {
        p.group = g;
        p.scroll = 0;
        p.dirty = true;
    }
    if accept {
        return Outcome::Accepted(p.traits.iter().filter_map(|x| if let Pick::Sim(t) = x { Some(*t) } else { None }).collect());
    }
    if cancel {
        return Outcome::Cancelled;
    }
    // The traits to take, in the group shown (as the game's dictionary has them, by name).
    let mut list: Vec<(Pick, Option<s3bake::gamedata::TraitInfo>)> = pool.all().into_iter().map(|t| (t, t.info(gu.as_deref()))).collect();
    list.retain(|(_, i)| p.group == 0 || i.as_ref().is_some_and(|i| trait_group(&i.category) == p.group));
    list.sort_by_key(|(t, i)| i.as_ref().map_or_else(|| t.name().to_string(), |i| i.name.clone()));
    let most = list.len().saturating_sub(AVAILABLE_ROWS);
    let over_list = p.s.id(PICK_AVAILABLE).and_then(|e| over.get(e).ok()).is_some_and(|r| r.cursor_over());
    let wheeled = over_list && wheel.delta.y != 0.0;
    if wheeled {
        p.scroll = (p.scroll as i64 + if wheel.delta.y > 0.0 { -1 } else { 1 }).clamp(0, most as i64) as usize;
        p.dirty = true;
    }
    if let Some(mut b) = p.bar.and_then(|e| bars.get_mut(e).ok()) {
        if (b.total, b.visible) != (list.len(), AVAILABLE_ROWS) {
            (b.total, b.visible) = (list.len(), AVAILABLE_ROWS);
        }
        if b.value != p.scroll {
            if wheeled {
                b.value = p.scroll;
            } else {
                p.scroll = b.value.min(most);
                p.dirty = true;
            }
        }
    }
    if !p.dirty {
        return Outcome::Open;
    }
    p.dirty = false;
    for (g, id) in PICK_FILTERS.iter().enumerate() {
        if let Some(mut b) = p.s.id(*id).and_then(|e| buttons.get_mut(e).ok())
            && b.selected != (p.group == g)
        {
            b.selected = p.group == g;
        }
    }
    let Some(template) = ui.layout("CASTraitEntry").cloned() else { return Outcome::Open };
    // (Rows alternate lighter and darker, as the game's do; names grey where they can't be taken.)
    let row = |size: [f32; 2], y: f32, n: usize, trait_: Option<(Pick, Option<&s3bake::gamedata::TraitInfo>, bool)>| {
        let mut w = template.clone();
        w.area = [0.0, y, size[0], y + size[1]];
        w.cls = "Button".into();
        let shade = if n % 2 == 0 { "cas_traits_row_darker_r2" } else { "cas_traits_row_lighter_r2" };
        if let Some(s3bake::ui::UiDrawable::Std { images: keys, .. }) = &mut w.drawable {
            keys[0] = s3pkg::fnv64(shade);
        }
        if let Some((t, info, ok)) = trait_ {
            let name = info.map_or_else(|| t.name().to_string(), |i| i.name.clone());
            edit_windows(&mut w, ENTRY_TEXT, &mut |tw| {
                tw.caption = name.clone();
                tw.colors = vec![if ok { 0xff16_3996 } else { 0xff96_9696 }];
            });
        } else {
            edit_windows(&mut w, ENTRY_TEXT, &mut |tw| tw.caption.clear());
        }
        w
    };
    // Taken: as many rows as the Sim's age has traits.
    if let Some(grid) = current_grid
        && let Some(g) = ui.find("TraitsPicker", PICK_CURRENT).and_then(|w| w.grid)
    {
        if let Some(old) = p.current.take() {
            commands.entity(old).try_despawn();
        }
        let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(grid))).id();
        p.current = Some(holder);
        let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
        for n in 0..5 {
            let t = p.traits.get(n).copied();
            let info = t.and_then(|t| t.info(gu.as_deref()));
            // (A trait that may not change is greyed in its slot.)
            let w = row(g.cell, g.padding[1] + n as f32 * step, n, t.map(|t| (t, info.as_ref(), !p.locked.contains(&t))));
            let s = ui.spawn_under(commands, images, fonts, &w, holder);
            let Some(r) = s.root else { continue };
            match t {
                Some(t) => {
                    commands.entity(r).insert(TraitRow(true, t));
                    if let Some(i) = &info {
                        commands.entity(r).insert(crate::icons::Tooltip(format!("{}\n{}", i.name, i.desc)));
                    }
                    if p.chosen == Some((true, t)) {
                        lit_row(commands, r);
                    }
                    if let (Some(win), Some(h)) = (s.id(ENTRY_ICON), info.as_ref().and_then(|i| gu.as_deref_mut().and_then(|g| g.icon(images, &i.icon_small)))) {
                        crate::hudpanels::picture(commands, win, h, Color::WHITE);
                    }
                }
                None => {
                    // (Rows past the Sim's age's traits aren't for taking.)
                    if n >= slots {
                        commands.entity(r).insert(Visibility::Hidden);
                    }
                }
            }
        }
    }
    // To take: the group's, scrolled.
    if let Some(grid) = p.s.id(PICK_AVAILABLE)
        && let Some(g) = ui.find("TraitsPicker", PICK_AVAILABLE).and_then(|w| w.grid)
    {
        if let Some(old) = p.available.take() {
            commands.entity(old).try_despawn();
        }
        let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE)).id();
        commands.entity(grid).insert_children(0, &[holder]);
        p.available = Some(holder);
        let step = g.cell[1] + g.cell_padding[1] + g.cell_padding[3];
        for (n, (t, info)) in list.iter().enumerate().skip(p.scroll).take(AVAILABLE_ROWS) {
            let ok = can_take(&p.traits, *t);
            let w = row(g.cell, g.padding[1] + (n - p.scroll) as f32 * step, n, Some((*t, info.as_ref(), ok)));
            let s = ui.spawn_under(commands, images, fonts, &w, holder);
            let Some(r) = s.root else { continue };
            commands.entity(r).insert(TraitRow(false, *t));
            if let Some(i) = info {
                commands.entity(r).insert(crate::icons::Tooltip(format!("{}\n{}", i.name, i.desc)));
            }
            if p.chosen == Some((false, *t)) {
                lit_row(commands, r);
            }
            if let (Some(win), Some(h)) = (s.id(ENTRY_ICON), info.as_ref().and_then(|i| gu.as_deref_mut().and_then(|g| g.icon(images, &i.icon_small)))) {
                crate::hudpanels::picture(commands, win, h, Color::WHITE);
            }
        }
    }
    Outcome::Open
}

fn lit_row(commands: &mut Commands, r: Entity) {
    commands.entity(r).queue(|mut e: EntityWorldMut| {
        if let Some(mut b) = e.get_mut::<UiButton>() {
            b.selected = true;
        }
    });
}
