//! The game's small popup menu (`PopupMenu`, UI.package): a column of the game's menu items
//! (`PopupMenuItem`) rising from where it was opened, as wide as its longest words, with its
//! cancel button; a click outside it closes it, as UI.dll's `PopupMenu` does (the main menu's
//! Options, Credits and Quit).

use bevy::prelude::*;

use crate::layout::{Spawned, UiAssets};

pub struct PopupMenuPlugin;

impl Plugin for PopupMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PopupChoice>().add_systems(Update, popup_clicks);
    }
}

/// What was chosen from a popup menu (by the id it was opened with): an item, or none.
#[derive(Message, Clone, Copy, Debug)]
pub struct PopupChoice {
    pub id: u32,
    pub index: Option<usize>,
}

/// The popup on screen.
#[derive(Component)]
pub struct OpenPopup {
    id: u32,
    s: Spawned,
}

/// One of its items.
#[derive(Component)]
struct PopupItem(usize);

const CONTAINER: u32 = 0x3960_c000;
const GRID: u32 = 0x3960_c001;
const CANCEL: u32 = 0x3960_c004;
const ITEM_TEXT: u32 = 2;
/// An item's height (`kItemHeight`), and about how wide the words are, a character.
const ITEM_HEIGHT: f32 = 33.0;
const CHAR_WIDTH: f32 = 7.5;

/// Opens a popup menu of these items at a place on screen (in the layouts' pixels).
pub fn open_popup(commands: &mut Commands, ui: &mut UiAssets, images: &mut Assets<Image>, fonts: &mut Assets<Font>, id: u32, items: &[String], at: Vec2) -> Option<Entity> {
    let mut dialog = ui.layout("PopupMenu")?.clone();
    let item = ui.export("PopupMenuItem", 1)?.clone();
    let grid_w = dialog.find(GRID)?.clone();
    let g = grid_w.grid?;
    let n = items.len();
    let more = (n as f32 - g.rows as f32) * ITEM_HEIGHT;
    // (As wide as the longest words.)
    let text_w = item.find(ITEM_TEXT).map_or(143.0, |t| t.area[2] - t.area[0]);
    let longest = items.iter().map(|s| s.chars().count() as f32 * CHAR_WIDTH).fold(0.0, f32::max);
    let wider = (longest - text_w).max(0.0);
    for c in dialog.children.iter_mut() {
        if c.id != CONTAINER {
            continue;
        }
        c.area = [c.area[0] + at.x, c.area[1] + at.y - more, c.area[2] + at.x + wider, c.area[3] + at.y];
        for gc in c.children.iter_mut() {
            if gc.id == GRID {
                gc.area[2] += wider;
                gc.area[3] += more;
            }
        }
    }
    let s = ui.spawn_root(commands, images, fonts, &dialog);
    let root = s.root?;
    commands.entity(root).insert((GlobalZIndex(50), Interaction::default(), crate::hud::BlocksWorld));
    if let Some(grid) = s.id(GRID) {
        for (i, label) in items.iter().enumerate() {
            let mut w = item.clone();
            let (iw, ih) = (w.area[2] - w.area[0] + wider, w.area[3] - w.area[1]);
            let y = g.padding[1] + i as f32 * ITEM_HEIGHT;
            w.area = [g.padding[0], y, g.padding[0] + iw, y + ih];
            w.cls = "Button".to_string();
            for t in w.children.iter_mut() {
                if t.id == ITEM_TEXT {
                    t.caption = label.clone();
                    t.area[2] += wider;
                }
            }
            let c = ui.spawn_under(commands, images, fonts, &w, grid);
            if let Some(r) = c.root {
                commands.entity(r).insert(PopupItem(i));
            }
        }
    }
    commands.entity(root).insert(OpenPopup { id, s });
    Some(root)
}

/// A click on an item chooses it; on the cancel button or outside, nothing.
fn popup_clicks(
    mut commands: Commands,
    popups: Query<(Entity, &OpenPopup)>,
    items: Query<(&Interaction, &PopupItem), Changed<Interaction>>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut out: MessageWriter<PopupChoice>,
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some((root, p)) = popups.iter().next() else { return };
    let mut chosen = items.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, item)| Some(item.0));
    if chosen.is_none() && (crate::livehud::pressed(&clicks, p.s.id(CANCEL)) || crate::livehud::pressed(&clicks, Some(root))) {
        chosen = Some(None);
    }
    if let Some(index) = chosen {
        out.write(PopupChoice { id: p.id, index });
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        commands.entity(root).despawn();
    }
}
