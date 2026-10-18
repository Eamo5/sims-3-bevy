//! Map View's filter, as the game's `MapViewController` (its layout `HUDMapViewSort`): in map
//! view, the button by the puck opens the list of what the map shows (All, Home and Work,
//! Friends' Homes, Community Locations, Opportunities, Careers, Real Estate, None), each a row of the game's
//! `HUDMapviewSortEntry`; a row the pointer is over shows its tags meanwhile, a row clicked keeps
//! them (for the rest of the game, as the game's `sLastFilterType`), its name beside the button.
//! Which tags go with which is the game's map-tag tuning (`MapTags`' `FilterType`), in `maptags`.

use bevy::prelude::*;

use crate::PlayMode;
use crate::layout::{Spawned, UiAssets, UiButton};

pub struct MapFilterPlugin;

impl Plugin for MapFilterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapFilter>()
            .add_systems(Update, (spawn_sort, sort_test, sort_panel).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(PlayMode::Live), |mut commands: Commands, mut filter: ResMut<MapFilter>| {
                commands.remove_resource::<MapSort>();
                filter.preview = None;
            });
    }
}

/// `MapTagFilterType`'s bits: what a tag shows under (`ALL_ONLY`: only when everything's shown).
pub const HOUSEHOLD_AND_WORK: u32 = 0x1;
pub const FRIENDS_HOMES: u32 = 0x2;
pub const PUBLIC_SPACES: u32 = 0x4;
pub const WORK: u32 = 0x8;
pub const OPPORTUNITIES: u32 = 0x10;
pub const REAL_ESTATE: u32 = 0x20;
pub const ALL_ONLY: u32 = 0x200;
pub const ALL: u32 = u32::MAX;

/// What the map shows: the filter chosen, and the one a row the pointer's over shows meanwhile.
#[derive(Resource)]
pub struct MapFilter {
    pub chosen: u32,
    pub preview: Option<u32>,
}

impl Default for MapFilter {
    fn default() -> Self {
        Self { chosen: ALL, preview: None }
    }
}

impl MapFilter {
    /// Whether a tag with these bits shows (the game's `FilterCull`).
    pub fn shows(&self, bits: u32) -> bool {
        bits & self.preview.unwrap_or(self.chosen) != 0
    }
}

/// The base game's sorts, in the game's order, by their captions' keys.
const SORTS: [(Caption, u32); 8] = [
    (Caption::Key(11920693074533536663), ALL),
    (Caption::Key(16967628283016774991), HOUSEHOLD_AND_WORK),
    (Caption::Key(2859540664055699868), FRIENDS_HOMES),
    (Caption::Key(17596890173930856818), PUBLIC_SPACES),
    (Caption::Name("Ui/Caption/Hud/Mapview:Opportunities"), OPPORTUNITIES),
    (Caption::Key(7509108683761921616), WORK),
    (Caption::Name("Ui/Caption/Hud/Mapview:RealEstate"), REAL_ESTATE),
    (Caption::Name("Ui/Caption/Hud/Mapview:None"), 0),
];
const FALLBACK: [&str; 8] = ["All", "Home and Work", "Friends' Homes", "Community Locations", "Opportunities", "Careers", "Real Estate", "None"];

#[derive(Clone, Copy)]
enum Caption {
    Key(u64),
    Name(&'static str),
}

// `HUDMapViewSort`'s windows: the button that opens the list, the list's grid, the chosen sort's
// name, and the list's background (hidden till opened).
const OPEN: u32 = 0x06bd_2001;
const GRID: u32 = 0x06bd_2002;
const NAME: u32 = 0x06bd_2003;
const LIST: u32 = 0x06bd_2004;

#[derive(Resource)]
struct MapSort {
    s: Spawned,
    rows: Vec<Entity>,
    open: bool,
    shown: Option<(bool, bool, u32)>,
}

/// A sort's row (its place in `SORTS`).
#[derive(Component)]
struct SortRow(usize);

fn caption(ui: &UiAssets, i: usize) -> String {
    match SORTS[i].0 {
        Caption::Key(k) => ui.localize_key(k),
        Caption::Name(n) => ui.localize(n),
    }
    .unwrap_or_else(|| FALLBACK[i].to_string())
}

fn spawn_sort(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), sort: Option<Res<MapSort>>, filter: Res<MapFilter>) {
    let Some(mut ui) = ui else { return };
    if sort.is_some() {
        return;
    }
    let (Some(layout), Some(entry)) = (ui.layout("HUDMapViewSort").cloned(), ui.export("HUDMapviewSortEntry", 1).cloned()) else { return };
    let Some(grid) = layout.find(GRID).and_then(|w| w.grid) else { return };
    // (The list as long as there are sorts, grown upward, as the game's `Init` does.)
    let more = SORTS.len() as f32 - grid.rows as f32;
    let mut w = layout;
    crate::layout::edit_windows(&mut w, LIST, &mut |l| l.area[1] -= more * grid.cell[1]);
    crate::layout::edit_windows(&mut w, GRID, &mut |g| g.area[3] += more * grid.cell[1]);
    let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(PlayMode::Live), Visibility::Hidden, GlobalZIndex(4)));
    let mut rows = Vec::new();
    if let Some(holder) = s.id(GRID) {
        for i in 0..SORTS.len() {
            let mut row = entry.clone();
            let y = grid.padding[1] + i as f32 * grid.cell[1];
            row.area = [grid.padding[0], y, grid.padding[0] + grid.cell[0], y + grid.cell[1]];
            row.caption = caption(&ui, i);
            row.caption_key = 0;
            let r = ui.spawn_under(&mut commands, &mut images, &mut fonts, &row, holder);
            if let Some(e) = r.root {
                commands.entity(e).insert(SortRow(i));
                rows.push(e);
            }
        }
    }
    if let Some(t) = s.text(NAME) {
        let i = SORTS.iter().position(|x| x.1 == filter.chosen).unwrap_or(0);
        commands.entity(t).insert(Text::new(caption(&ui, i)));
    }
    commands.insert_resource(MapSort { s, rows, open: false, shown: None });
}

/// MAP_SORT=<row>[,open] (tests): that sort chosen ten seconds in, and the list left open.
fn sort_test(time: Res<Time>, mut done: Local<bool>, mut filter: ResMut<MapFilter>, sort: Option<ResMut<MapSort>>, ui: Option<Res<UiAssets>>, mut texts: Query<&mut Text>) {
    let (Some(mut sort), Some(ui), Ok(v)) = (sort, ui, std::env::var("MAP_SORT")) else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    *done = true;
    let i = v.split(',').next().and_then(|i| i.trim().parse::<usize>().ok()).unwrap_or(0).min(SORTS.len() - 1);
    filter.chosen = SORTS[i].1;
    sort.open = v.contains("open");
    if let Some(mut t) = sort.s.text(NAME).and_then(|t| texts.get_mut(t).ok()) {
        t.0 = caption(&ui, i);
    }
    info!("map sort test: {} (open {})", caption(&ui, i), sort.open);
}

/// Shown in map view (not in buy or build): the list opened and closed by its button, a row
/// hovered shown, a row clicked chosen.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sort_panel(
    sort: Option<ResMut<MapSort>>,
    ui: Option<Res<UiAssets>>,
    mut filter: ResMut<MapFilter>,
    cams: Query<&crate::camera::SimsCamera>,
    buy: Res<crate::buy::BuyMode>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    rows: Query<(&Interaction, &SortRow)>,
    mut buttons: Query<&mut UiButton>,
    mut vis: Query<&mut Visibility>,
    mut texts: Query<&mut Text>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    let (Some(mut sort), Some(ui)) = (sort, ui) else { return };
    let sort = &mut *sort;
    let on = !buy.active && cams.single().is_ok_and(|c| c.distance > crate::maptags::MAP_DISTANCE);
    if crate::livehud::pressed(&clicks, sort.s.id(OPEN)) {
        sort.open = !sort.open;
    }
    // (A row clicked: that sort kept, its name by the button, the list closed.)
    let mut chosen = None;
    for (e, i) in &clicks {
        if *i == Interaction::Pressed
            && let Ok((_, row)) = rows.get(e)
        {
            chosen = Some(row.0);
        }
    }
    if let Some(i) = chosen {
        filter.chosen = SORTS[i].1;
        sort.open = false;
        if let Some(mut t) = sort.s.text(NAME).and_then(|t| texts.get_mut(t).ok()) {
            t.0 = caption(&ui, i);
        }
    }
    // (A click anywhere else closes it, as the list losing focus does.)
    let over_list = rows.iter().any(|(i, _)| *i != Interaction::None) || sort.s.id(OPEN).and_then(|e| clicks.get(e).ok()).is_some();
    if sort.open && mouse.just_pressed(MouseButton::Left) && !over_list && chosen.is_none() {
        sort.open = false;
    }
    if !on {
        sort.open = false;
    }
    // (The row the pointer's over shown meanwhile.)
    let hovered = if sort.open { rows.iter().find(|(i, _)| **i == Interaction::Hovered).map(|(_, r)| SORTS[r.0].1) } else { None };
    if filter.preview != hovered {
        filter.preview = hovered;
    }
    let want = (on, sort.open, filter.chosen);
    if sort.shown == Some(want) {
        return;
    }
    sort.shown = Some(want);
    crate::livehud::set_visible(&mut vis, sort.s.root, on);
    crate::livehud::set_visible(&mut vis, sort.s.id(LIST), sort.open);
    if let Some(mut b) = sort.s.id(OPEN).and_then(|e| buttons.get_mut(e).ok())
        && b.selected != sort.open
    {
        b.selected = sort.open;
    }
    for (k, e) in sort.rows.iter().enumerate() {
        if let Ok(mut b) = buttons.get_mut(*e)
            && b.selected != (SORTS[k].1 == filter.chosen)
        {
            b.selected = SORTS[k].1 == filter.chosen;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_show_under_their_sorts() {
        let mut f = MapFilter::default();
        for bits in [HOUSEHOLD_AND_WORK, FRIENDS_HOMES, PUBLIC_SPACES, ALL_ONLY, ALL] {
            assert!(f.shows(bits), "everything under All");
        }
        f.chosen = FRIENDS_HOMES;
        assert!(f.shows(FRIENDS_HOMES) && f.shows(ALL) && !f.shows(PUBLIC_SPACES) && !f.shows(ALL_ONLY));
        f.chosen = 0;
        assert!(!f.shows(ALL), "None hides even the home");
        f.preview = Some(PUBLIC_SPACES);
        assert!(f.shows(PUBLIC_SPACES), "a hovered row shows its tags meanwhile");
    }
}
