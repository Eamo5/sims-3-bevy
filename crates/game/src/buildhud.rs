//! Original Build-mode house diagram, puck, wall tools and pool tools.
//! Catalogue browsers continue to use the shared Buy/Build catalogue above the navigation.

use bevy::prelude::*;

use crate::build::BuildTool;
use crate::buy::{BuyButton, BuyMode, BUILD_TAB, DOORS_TAB, FENCES_TAB, FLOORS_TAB, ROOFS_TAB, TERRAIN_TAB, WALLPAPER_TAB, WINDOWS_TAB};
use crate::layout::{Spawned, UiAssets, UiButton};
use crate::livehud::{set_visible, LiveHud};
use crate::{AppState, PlayMode};

pub struct BuildHudPlugin;

impl Plugin for BuildHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PlayMode::Live), (spawn, crate::buildcatalog::spawn).chain().after(crate::buy::reset_buy_mode))
            .add_systems(Update, (controls, show).chain().after(crate::buyhud::show).before(crate::buyhistory::update)
                .run_if(in_state(PlayMode::Live)).run_if(resource_exists::<BuildHud>))
            .add_systems(Update, (crate::buildcatalog::controls, crate::buildcatalog::draw).chain().after(show).before(crate::buyhistory::update)
                .run_if(in_state(PlayMode::Live)).run_if(resource_exists::<crate::buildcatalog::BuildCatalog>))
            .add_systems(PreUpdate, crate::buildcatalog::probe.after(bevy::ui::UiSystems::Focus).run_if(in_state(PlayMode::Live)).run_if(resource_exists::<crate::buildcatalog::BuildCatalog>));
    }
}

#[derive(Resource)]
pub struct BuildHud {
    pub(crate) puck: Spawned,
}

const HOME: u32 = 0x06e7_7680;
const MIDDLE: u32 = 0x06e7_70f8;
const BACK: u32 = 0x06e8_9130;
const WALLS: u32 = 0x300;
const POOL: u32 = 0x06ee_34e8;
/// The Foundations panel (its square and diagonal tools, and its catalogue of styles).
const FOUNDATIONS: u32 = 0x000a_a300;
const FOUNDATION_SQUARE: u32 = 0x000a_a305;
const FOUNDATION_DIAGONAL: u32 = 0x000a_a306;
/// The Basement panel (its square and diagonal tools).
const BASEMENTS: u32 = 0x500;
const BASEMENT_SQUARE: u32 = 0x501;
const BASEMENT_DIAGONAL: u32 = 0x502;
const HAND: u32 = 0x2e4;
const HAMMER: u32 = 0x2e5;
const DESIGN: u32 = 0x2e6;
const CLONE: u32 = 0x2e7;
const LIGHT: u32 = 0x2e8;
const GRID: u32 = 0x2e9;

#[derive(Component, Clone, Copy)]
enum Action { Home, Tool(BuildTool), Category(usize), Objects(crate::buildobjects::ObjTab), HalfWalls, Hand, Lighting, Grid, Undo, Redo }

fn home_action(id: u32) -> Option<Action> {
    use crate::buildobjects::ObjTab;
    Some(match id {
        0x1002 => Action::Tool(BuildTool::Wall),
        0x1003 => Action::Category(WALLPAPER_TAB),
        0x1012 => Action::Category(FLOORS_TAB),
        // (Doors, windows, arches, columns and fireplaces: the game's construction catalogue;
        // trees, shrubs, flowers and rocks its gardening catalogue.)
        0x1005 => Action::Objects(ObjTab::Doors),
        0x1014 => Action::Objects(ObjTab::Windows),
        0x1015 => Action::Objects(ObjTab::Arches),
        0x1007 => Action::Objects(ObjTab::Columns),
        0x1010 => Action::Objects(ObjTab::Gates),
        0x1000 => Action::Objects(ObjTab::Fireplaces),
        0x1001 => Action::Objects(ObjTab::Trees),
        0x100c => Action::Objects(ObjTab::Shrubs),
        0x100a => Action::Objects(ObjTab::Flowers),
        0x100e => Action::Objects(ObjTab::Rocks),
        0x1009 => Action::Category(ROOFS_TAB),
        0x100f => Action::Category(FENCES_TAB),
        // (Half walls: the fences tab's other half, drawn with the fence tool.)
        0x1017 => Action::HalfWalls,
        0x1008 => Action::Tool(BuildTool::Stairs),
        0x1013 => Action::Tool(BuildTool::Pool),
        0x1018 => Action::Tool(BuildTool::Fountain),
        0x1004 => Action::Tool(BuildTool::Foundation),
        0x1016 => Action::Tool(BuildTool::Basement),
        0x100b | 0x1011 => Action::Category(TERRAIN_TAB),
        _ => return None,
    })
}

pub fn native_panel(buy: &BuyMode) -> bool {
    buy.build_look && buy.active && buy.category == BUILD_TAB
        && !(buy.styling() && buy.placing.is_some())
        && matches!(buy.tool, None | Some(BuildTool::Wall | BuildTool::Room | BuildTool::DiagonalRoom | BuildTool::Pool | BuildTool::Fountain | BuildTool::Foundation | BuildTool::Basement | BuildTool::Sledgehammer))
}

fn spawn(
    mut commands: Commands, ui: Option<ResMut<UiAssets>>, mut images: ResMut<Assets<Image>>, mut fonts: ResMut<Assets<Font>>,
    mut buy: ResMut<BuyMode>, old: Option<Res<BuildHud>>,
) {
    let Some(mut ui) = ui else { return };
    if !crate::livehud::active(Some(&ui)) { return; }
    if let Some(root) = old.and_then(|h| h.puck.root) { commands.entity(root).try_despawn(); }
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "Build") else { return };
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(AppState::InGame), GlobalZIndex(5), Visibility::Hidden));
    // These are alternative subpanels of the original controller, not simultaneous windows.
    if let Some(layout) = ui.layout("Build") {
        for child in &layout.children {
            if let Some(e) = s.id(child.id) {
                commands.entity(e).insert(if child.id == 0x200 { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
        if let Some(middle) = layout.find(MIDDLE) {
            for child in &middle.children {
                if let Some(e) = s.within(s.id(MIDDLE).unwrap(), child.id) {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
        }
    }
    for id in [0x0dbb_63c0, 0x06e0_9710, 0x4a0, 0x06f4_d828, 0x8fff_fc00, 0x8fff_fa17] {
        if let Some(e) = s.id(id) { commands.entity(e).insert(Visibility::Hidden); }
    }
    for id in [0x200, HOME, MIDDLE, 0x06f4_d818] {
        if let Some(e) = s.id(id) { commands.entity(e).insert((Interaction::default(), crate::hud::BlocksWorld)); }
    }
    for id in 0x1000..=0x1018 {
        if let Some(e) = s.id(id) {
            if let Some(action) = home_action(id) { commands.entity(e).insert(action); }
            else { commands.entity(e).insert(Unavailable); }
        }
    }
    for (id, action) in [
        (BACK, Action::Home), (HAND, Action::Hand), (HAMMER, Action::Tool(BuildTool::Sledgehammer)),
        (LIGHT, Action::Lighting), (GRID, Action::Grid),
        (0x2e2, Action::Undo), (0x2e3, Action::Redo),
        (0x301, Action::Tool(BuildTool::Wall)), (0x302, Action::Tool(BuildTool::Room)), (0x304, Action::Tool(BuildTool::DiagonalRoom)),
        (0x321a, Action::Tool(BuildTool::Wall)), (0x321b, Action::Category(WALLPAPER_TAB)),
        (0x321c, Action::Tool(BuildTool::Wall)), (0x321d, Action::Category(WALLPAPER_TAB)),
        (0x06ee_34a0, Action::Tool(BuildTool::Pool)), (0x06ee_34af, Action::Tool(BuildTool::Pool)),
        (FOUNDATION_SQUARE, Action::Tool(BuildTool::Foundation)),
        (BASEMENT_SQUARE, Action::Tool(BuildTool::Basement)),
    ] {
        let e = if matches!(id, 0x301 | 0x302 | 0x304) { s.id(WALLS).and_then(|p| s.within(p, id)) } else { s.id(id) };
        if let Some(e) = e { commands.entity(e).insert(action); }
    }
    // (The diagonal foundation isn't laid here yet, nor the foundation styles offered.)
    for id in [FOUNDATION_DIAGONAL, BASEMENT_DIAGONAL] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Unavailable);
        }
    }
    for id in [0x000a_a302, 0x000a_a307] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (id, button) in [(CLONE, BuyButton::Eyedropper), (DESIGN, BuyButton::Styling)] {
        if let Some(e) = s.id(id) { commands.entity(e).insert(button); }
    }
    for (id, tooltip) in [(0x2e2, "Undo (Ctrl+Z)"), (0x2e3, "Redo (Ctrl+Y)")] {
        if let Some(e) = s.id(id) { commands.entity(e).insert(crate::icons::Tooltip(tooltip.into())); }
    }
    for id in [0x2ea, 0x2ef, 0x2f1, 0x305, 0x06ee_34a2, 0x0a67_d7f0] {
        if let Some(e) = s.id(id) { commands.entity(e).insert(Unavailable); }
    }
    // Hide fountain/curved-pool alternatives inside the rectangular-pool panel.
    for id in [0x0a67_db20, 0x06ee_34df, 0x06ee_34d2, 0x06ee_34d0, 0x06ee_34d6, 0x06ee_34bf, 0x06ee_34a7] {
        if let Some(e) = s.id(id) { commands.entity(e).insert(Visibility::Hidden); }
    }
    buy.build_look = true;
    // Build's puck is one id-range above the Live puck (Buy's is one below it).
    let puck = s.renamed(|id| if (0x8fff_fa00..0x8fff_fd00).contains(&id) { id - 0x0010_0000 } else { id });
    commands.insert_resource(BuildHud { puck });
}

#[derive(Component)]
struct Unavailable;

fn controls(
    mut commands: Commands, mut buy: ResMut<BuyMode>, clock: Res<crate::clock::GameClock>,
    actions: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut history: ResMut<crate::buyhistory::BuyHistory>,
    mut objects: Option<ResMut<crate::buildobjects::BuildObjects>>,
) {
    if !buy.active || buy.category < WALLPAPER_TAB { return; }
    for (interaction, action) in &actions {
        if *interaction != Interaction::Pressed { continue; }
        match *action {
            Action::Home => { buy.drop_tools(&mut commands); buy.show(BUILD_TAB); }
            Action::Tool(tool) => { buy.drop_tools(&mut commands); buy.show(BUILD_TAB); buy.tool = Some(tool); }
            Action::Category(category) => { buy.drop_tools(&mut commands); buy.show(category); }
            Action::HalfWalls => { buy.drop_tools(&mut commands); buy.show(FENCES_TAB); buy.half_walls = true; }
            Action::Objects(tab) => {
                buy.drop_tools(&mut commands);
                match objects.as_deref_mut() {
                    Some(o) => {
                        o.choose(tab);
                        buy.show(tab.catalogue());
                    }
                    // (Without the game's catalogues: the plain one's doors and windows.)
                    None => buy.show(if tab == crate::buildobjects::ObjTab::Windows { WINDOWS_TAB } else { DOORS_TAB }),
                }
            }
            Action::Hand => buy.drop_tools(&mut commands),
            Action::Lighting => buy.toggle_lighting(clock.hour_f()),
            Action::Grid => buy.hide_grid = !buy.hide_grid,
            Action::Undo => history.request = Some(crate::buyhistory::Request::Undo),
            Action::Redo => history.request = Some(crate::buyhistory::Request::Redo),
        }
    }
}

fn show(
    hud: Res<BuildHud>, live: Option<Res<LiveHud>>, buy: Res<BuyMode>, clock: Res<crate::clock::GameClock>,
    mut visibility: Query<&mut Visibility>, mut buttons: Query<(&mut UiButton, Has<Unavailable>)>,
    history: Res<crate::buyhistory::BuyHistory>,
) {
    let on = buy.active && buy.category >= WALLPAPER_TAB;
    let s = &hud.puck;
    set_visible(&mut visibility, s.root, on);
    if let Some(live) = live {
        // The Buy layout owns the puck in Buy Mode; this controller owns it in Build Mode.
        if on || !buy.active { set_visible(&mut visibility, live.puck.root, !on); }
    }
    if !on { return; }
    let wall = buy.category == BUILD_TAB && matches!(buy.tool, Some(BuildTool::Wall | BuildTool::Room | BuildTool::DiagonalRoom | BuildTool::Sledgehammer));
    let pool = buy.category == BUILD_TAB && buy.tool == Some(BuildTool::Pool);
    let foundation = buy.category == BUILD_TAB && buy.tool == Some(BuildTool::Foundation);
    let basement = buy.category == BUILD_TAB && buy.tool == Some(BuildTool::Basement);
    let covers = crate::buildcatalog::showing(&buy) || crate::buildobjects::showing(&buy);
    set_visible(&mut visibility, s.id(HOME), !wall && !pool && !foundation && !basement && !covers);
    set_visible(&mut visibility, s.id(MIDDLE), wall || pool || foundation || basement || covers);
    set_visible(&mut visibility, s.id(BACK), wall || pool || foundation || basement || covers);
    set_visible(&mut visibility, s.id(FOUNDATIONS), foundation);
    set_visible(&mut visibility, s.id(BASEMENTS), basement);
    set_visible(&mut visibility, s.id(0x390), crate::buildobjects::showing(&buy) && buy.category == crate::buy::CONSTRUCTION_TAB);
    set_visible(&mut visibility, s.id(0x3a0), crate::buildobjects::showing(&buy) && buy.category == crate::buy::GARDENING_TAB);
    set_visible(&mut visibility, s.id(0x320), covers && buy.category == WALLPAPER_TAB);
    set_visible(&mut visibility, s.id(0x340), covers && buy.category == FLOORS_TAB);
    set_visible(&mut visibility, s.id(WALLS), wall);
    set_visible(&mut visibility, s.id(POOL), pool);
    for id in [0x06ee_34af, 0x06ee_34a2, 0x06ee_34a0, 0x0a67_d7f0, 0x06ee_34a9, 0x06ee_34be] {
        set_visible(&mut visibility, s.id(id), pool);
    }
    for (mut button, unavailable) in &mut buttons {
        if unavailable && !button.disabled { button.disabled = true; }
    }
    let holding = buy.placing.as_ref().is_some_and(|p| p.owned);
    for (id, enabled) in [(0x2e2, history.can_undo()), (0x2e3, history.can_redo())] {
        if let Some(e) = s.id(id) && let Ok((mut button, _)) = buttons.get_mut(e) {
            button.disabled = holding || !enabled;
        }
    }
    for (id, selected) in [
        (HAND, buy.tool.is_none() && !buy.eyedropper && !buy.styling()), (HAMMER, buy.tool == Some(BuildTool::Sledgehammer)),
        (CLONE, buy.eyedropper), (DESIGN, buy.styling()), (GRID, !buy.hide_grid),
        (LIGHT, !(6.0..20.0).contains(&buy.lighting_hour(clock.hour_f()))),
    ] {
        if let Some(e) = s.id(id) && let Ok((mut button, _)) = buttons.get_mut(e) { button.selected = selected; }
    }
    for (id, tool) in [(0x301, BuildTool::Wall), (0x302, BuildTool::Room), (0x304, BuildTool::DiagonalRoom)] {
        if let Some(e) = s.id(WALLS).and_then(|p| s.within(p, id))
            && let Ok((mut button, _)) = buttons.get_mut(e)
        {
            button.selected = buy.tool == Some(tool);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_tools_and_catalogue_browsers_have_distinct_panels() {
        let mut buy = BuyMode::default();
        buy.build_look = true;
        buy.show(BUILD_TAB);
        assert!(native_panel(&buy));
        for tool in [BuildTool::Wall, BuildTool::Room, BuildTool::Pool, BuildTool::Foundation, BuildTool::Basement, BuildTool::Sledgehammer] {
            buy.tool = Some(tool);
            assert!(native_panel(&buy));
        }
        buy.tool = Some(BuildTool::Stairs);
        assert!(!native_panel(&buy));
        buy.show(WALLPAPER_TAB);
        assert!(!native_panel(&buy));
        assert!(matches!(home_action(0x1013), Some(Action::Tool(BuildTool::Pool))));
        assert!(matches!(home_action(0x1005), Some(Action::Objects(crate::buildobjects::ObjTab::Doors))));
        assert!(matches!(home_action(0x1001), Some(Action::Objects(crate::buildobjects::ObjTab::Trees))));
        assert!(matches!(home_action(0x1004), Some(Action::Tool(BuildTool::Foundation))));
        assert!(matches!(home_action(0x1016), Some(Action::Tool(BuildTool::Basement))));
        assert!(home_action(0x1006).is_none());
    }

    #[test]
    fn diagram_buttons_select_tools_and_back_returns_home() {
        let mut app = App::new();
        app.init_resource::<BuyMode>().init_resource::<crate::clock::GameClock>().init_resource::<crate::buyhistory::BuyHistory>().add_systems(Update, controls);
        app.world_mut().resource_mut::<BuyMode>().show(BUILD_TAB);
        let button = app.world_mut().spawn((Interaction::Pressed, home_action(0x1013).unwrap())).id();
        app.update();
        assert_eq!(app.world().resource::<BuyMode>().tool, Some(BuildTool::Pool));
        app.world_mut().entity_mut(button).insert((Interaction::Pressed, Action::Home));
        app.update();
        let buy = app.world().resource::<BuyMode>();
        assert_eq!(buy.category, BUILD_TAB);
        assert!(buy.tool.is_none());
        app.world_mut().entity_mut(button).insert((Interaction::Pressed, home_action(0x1003).unwrap()));
        app.update();
        assert_eq!(app.world().resource::<BuyMode>().category, WALLPAPER_TAB);
    }
}
