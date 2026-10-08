//! The pie menu in the game's own look (`HUDPieMenu`'s twelve bubbles, from UI.package): the
//! first option at the top and the rest round clockwise, each in the bubble of the twelve whose
//! place is nearest (its tail towards the middle), widened to its words, lit under the pointer
//! with the bubble's own pictures and the game's highlight colour, as UI.dll's `PieMenu` lays
//! them out. The acting Sim's face is in the middle (see `hud::open_pie`).

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::layout::UiAssets;

pub struct PieMenuPlugin;

impl Plugin for PieMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (enable, build_pies, text_hover).chain());
    }
}

/// Whether menus are drawn in the game's look (its interface is baked).
pub static GAME_PIE: AtomicBool = AtomicBool::new(false);

/// Most options a menu shows round its middle (`kMaxVisibleItems`).
pub const MAX_ITEMS: usize = 12;

/// A pie menu to draw: its options' words.
#[derive(Component)]
pub struct PieRequest(pub Vec<String>);

/// An option's words (lit under the pointer).
#[derive(Component)]
struct PieText(Entity);

/// The bubbles (`0x8ED0D610 + slot`: 0 top, 1–5 down the left, 6 bottom, 7–11 up the right)
/// and their children: the words, an icon, the groove to the middle.
const ITEM: u32 = 0x8ed0_d610;
const ITEM_TEXT: u32 = 1;
const ITEM_ICON: u32 = 2;
const ITEM_GROOVE: u32 = 3;
/// The game's layout radius, the width the bubbles' words are drawn at, the icon's room
/// (taken off when there's none), and the narrowest bubble.
const RADIUS: f32 = 80.0;
const TEXT_WIDTH: f32 = 84.0;
const ICON_ROOM: f32 = 21.0;
const MIN_WIDTH: f32 = 55.0;
/// The words' colours (`HUDPieMenuReference`'s Reg and High).
const REGULAR: u32 = 0xff19_2d87;
const HIGHLIGHT: u32 = 0xff3c_31bf;
/// About how wide the words are, a character (Sims3Bold9pt, Helvetica Rounded).
const CHAR_WIDTH: f32 = 6.4;

fn enable(ui: Option<Res<UiAssets>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    if let Some(ui) = ui {
        GAME_PIE.store(ui.find("HUDPieMenu", ITEM).is_some(), Ordering::Relaxed);
        *done = true;
    }
}

/// Where the options go: the place round the middle of each in turn (the first at the top,
/// then clockwise), and which of the twelve bubbles is nearest each.
pub fn places(n: usize) -> Vec<(Vec2, u32)> {
    let at = |a: f32| Vec2::new(a.cos(), a.sin()) * RADIUS;
    let locs: Vec<Vec2> = (0..n).map(|i| at(1.5 * std::f32::consts::PI - i as f32 * std::f32::consts::TAU / n as f32)).collect();
    let ideal: Vec<Vec2> = (0..12).map(|j| at(1.5 * std::f32::consts::PI - j as f32 * std::f32::consts::TAU / 12.0)).collect();
    (0..n)
        .map(|k| {
            let li = if k == 0 { 0 } else { n - k };
            let slot = if li == 0 { 0 } else { (0..12).min_by(|a, b| locs[li].distance_squared(ideal[*a]).total_cmp(&locs[li].distance_squared(ideal[*b]))).unwrap_or(0) };
            (locs[li], slot as u32)
        })
        .collect()
}

fn build_pies(
    mut commands: Commands,
    q: Query<(Entity, &PieRequest), Added<PieRequest>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
) {
    let Some(mut ui) = ui else { return };
    for (root, req) in &q {
        for (k, (label, (loc, slot))) in req.0.iter().zip(places(req.0.len())).enumerate() {
            let Some(mut b) = ui.find("HUDPieMenu", ITEM + slot).cloned() else { continue };
            let [x1, y1, x2, y2] = b.area;
            let bw = x2 - x1;
            let words = label.chars().count() as f32 * CHAR_WIDTH;
            let width = (bw + words - TEXT_WIDTH + 2.0 - ICON_ROOM).max(MIN_WIDTH).round();
            // (Centred at the top and bottom; on the left, its tail kept where it was.)
            let x = match slot {
                0 | 6 => -width * 0.5,
                1..=5 => x1 + bw - width,
                _ => x1,
            };
            let p = (loc + Vec2::new(x, y1)).round();
            b.area = [p.x, p.y, p.x + width, p.y + (y2 - y1)];
            b.cls = "Button".to_string();
            b.flags |= s3bake::ui::WIN_VISIBLE | s3bake::ui::WIN_ENABLED;
            for c in &mut b.children {
                match c.id {
                    ITEM_TEXT => {
                        c.caption = label.clone();
                        c.colors = vec![REGULAR];
                        // (Wide enough for the words, wherever they're anchored.)
                        if words > TEXT_WIDTH {
                            let grow = words - TEXT_WIDTH;
                            if slot >= 1 && slot <= 5 {
                                c.area[0] -= grow;
                            } else {
                                c.area[2] += grow;
                            }
                        }
                    }
                    ITEM_ICON => c.flags &= !s3bake::ui::WIN_VISIBLE,
                    // (The top and bottom bubbles' groove, under or over their middle.)
                    ITEM_GROOVE if slot == 0 || slot == 6 => {
                        let gw = c.area[2] - c.area[0];
                        c.area[0] = ((width - gw) * 0.5).round();
                        c.area[2] = c.area[0] + gw;
                    }
                    _ => {}
                }
            }
            let s = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, root);
            if let Some(r) = s.root {
                commands.entity(r).insert((crate::hud::PieOption(k), crate::hud::BlocksWorld));
                if let Some(t) = s.text(ITEM_TEXT) {
                    commands.entity(r).insert(PieText(t));
                }
            }
        }
    }
}

/// The words lit under the pointer, as the game's.
fn text_hover(q: Query<(&Interaction, &PieText), Changed<Interaction>>, mut colors: Query<&mut TextColor>) {
    for (i, t) in &q {
        if let Ok(mut c) = colors.get_mut(t.0) {
            c.0 = crate::layout::color(if *i == Interaction::None { REGULAR } else { HIGHLIGHT });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_go_round_clockwise_from_the_top() {
        // Four: top, right, bottom, left.
        let p = places(4);
        assert_eq!(p.iter().map(|x| x.1).collect::<Vec<_>>(), vec![0, 9, 6, 3]);
        // Twelve fill every bubble once.
        let mut s: Vec<u32> = places(12).iter().map(|x| x.1).collect();
        s.sort();
        assert_eq!(s, (0..12).collect::<Vec<_>>());
    }
}
