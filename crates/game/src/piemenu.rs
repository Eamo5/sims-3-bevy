//! The pie menu in the game's own look (`HUDPieMenu`'s twelve bubbles, from UI.package): the
//! first option at the top and the rest round clockwise, each in the bubble of the twelve whose
//! place is nearest (its tail towards the middle), widened to its words, lit under the pointer
//! with the bubble's own pictures and the game's highlight colour, as UI.dll's `PieMenu` lays
//! them out. The acting Sim's face is in the middle (see `hud::open_pie`); a submenu's middle is
//! the game's return button, named for it. A menu that would go off the screen is moved onto it,
//! the pointer with it, as the game's `SetupPieMenuButtons` does.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::layout::UiAssets;

pub struct PieMenuPlugin;

impl Plugin for PieMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (enable, build_pies, warp_pointer, text_hover, mask_hover).chain());
    }
}

/// Whether menus are drawn in the game's look (its interface is baked).
pub static GAME_PIE: AtomicBool = AtomicBool::new(false);

/// Most options a menu shows round its middle (`kMaxVisibleItems`).
pub const MAX_ITEMS: usize = 12;

/// A pie menu to draw: its options' words, and a submenu's name (its middle's way back).
#[derive(Component)]
pub struct PieRequest(pub Vec<String>, pub Option<String>);

/// A submenu's middle: back to the menu it was opened from.
#[derive(Component)]
pub struct PieBack;

/// An option's words (lit under the pointer).
#[derive(Component)]
struct PieText(Entity);

/// The menu's hit mask, under its bubbles: the option the pointer points towards from the
/// middle is lit and taken (the game's `mPieMenuHitMask` and `GetButtonIndex`), so a bubble's
/// whole slice of the menu is its, its groove too.
#[derive(Component)]
pub struct PieMask;

/// No option within this of the middle, either way (the game's 30).
const DEAD_ZONE: f32 = 30.0;

/// A hit mask over a menu's area (about its middle).
pub fn mask(area: Rect) -> impl Bundle {
    (
        Button,
        PieMask,
        crate::hud::BlocksWorld,
        ZIndex(-1),
        Node { position_type: PositionType::Absolute, left: Val::Px(area.min.x), top: Val::Px(area.min.y), width: Val::Px(area.width()), height: Val::Px(area.height()), ..default() },
    )
}

/// The option a pointer this far from the middle points towards (the one whose place lies most
/// its way), none near the middle.
pub fn option_toward(dirs: &[Vec2], d: Vec2) -> Option<usize> {
    if d.x.abs() < DEAD_ZONE && d.y.abs() < DEAD_ZONE {
        return None;
    }
    dirs.iter().enumerate().max_by(|a, b| a.1.dot(d).total_cmp(&b.1.dot(d))).map(|(i, _)| i)
}

/// The bubbles (`0x8ED0D610 + slot`: 0 top, 1–5 down the left, 6 bottom, 7–11 up the right)
/// and their children: the words, an icon, the groove to the middle.
const ITEM: u32 = 0x8ed0_d610;
const ITEM_TEXT: u32 = 1;
const ITEM_ICON: u32 = 2;
const ITEM_GROOVE: u32 = 3;
/// The middle's return button.
const RETURN: u32 = 0x8ed0_d61c;
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

#[allow(clippy::type_complexity)]
fn build_pies(
    mut commands: Commands,
    mut q: Query<(Entity, &PieRequest, &mut Node), Added<PieRequest>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut pie: ResMut<crate::hud::PieMenu>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(mut ui) = ui else { return };
    for (root, req, mut node) in &mut q {
        // (All its bubbles' room, to keep it on the screen.)
        let mut room = Rect::EMPTY;
        // The middle of a submenu: back to the menu before, named for this one.
        if let Some(name) = &req.1
            && let Some(mut b) = ui.find("HUDPieMenu", RETURN).cloned()
        {
            let words = name.chars().count() as f32 * CHAR_WIDTH;
            let [x1, y1, x2, y2] = b.area;
            let grow = (words - TEXT_WIDTH).max(0.0).round();
            b.area = [x1 - (grow * 0.5).round(), y1, x2 + grow - (grow * 0.5).round(), y2];
            b.cls = "Button".to_string();
            b.flags |= s3bake::ui::WIN_VISIBLE | s3bake::ui::WIN_ENABLED;
            for c in &mut b.children {
                match c.id {
                    ITEM_TEXT => {
                        c.caption = format!("{name}...");
                        c.colors = vec![REGULAR];
                        c.area[2] += grow;
                    }
                    ITEM_ICON => c.flags &= !s3bake::ui::WIN_VISIBLE,
                    _ => {}
                }
            }
            room = room.union(Rect::new(b.area[0], b.area[1], b.area[2], b.area[3]));
            let s = ui.spawn_under(&mut commands, &mut images, &mut fonts, &b, root);
            if let Some(r) = s.root {
                commands.entity(r).insert((PieBack, crate::hud::BlocksWorld));
                if let Some(t) = s.text(ITEM_TEXT) {
                    commands.entity(r).insert(PieText(t));
                }
            }
        }
        for (k, (label, (loc, slot))) in req.0.iter().zip(places(req.0.len())).enumerate() {
            // (What opens more, as the game marks it: "Have Breakfast...".)
            let label = &label.strip_suffix(" ›").map_or_else(|| label.clone(), |l| format!("{l}..."));
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
            room = room.union(Rect::new(b.area[0], b.area[1], b.area[2], b.area[3]));
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
            // (The groove's ball, out past the top and bottom bubbles: a click on it goes
            // through to the menu's mask, and so to its bubble.)
            if let Some(g) = s.id(ITEM_GROOVE) {
                commands.entity(g).insert((Pickable::IGNORE, bevy::ui::FocusPolicy::Pass));
            }
        }
        // (Its whole area, picked by the pointer's direction.)
        if !room.is_empty() {
            let pad = 8.0;
            let m = commands.spawn(mask(Rect::new(room.min.x - pad, room.min.y - pad, room.max.x + pad, room.max.y + pad))).id();
            commands.entity(root).add_child(m);
        }
        // Moved onto the screen when it would go off it, the pointer to its middle again.
        if let (Ok(w), Val::Px(x), Val::Px(y)) = (windows.single(), node.left, node.top)
            && !room.is_empty()
        {
            let at = Vec2::new(x, y);
            let (lo, hi) = (at + room.min, at + room.max);
            let screen = Vec2::new(w.width(), w.height());
            let shift = |lo: f32, hi: f32, size: f32| if lo < 0.0 { -lo } else if hi > size { size - hi } else { 0.0 };
            let d = Vec2::new(shift(lo.x, hi.x, screen.x), shift(lo.y, hi.y, screen.y));
            if d != Vec2::ZERO {
                node.left = Val::Px(at.x + d.x);
                node.top = Val::Px(at.y + d.y);
                pie.at = at + d;
                pie.warp = Some(at + d);
            }
        }
    }
}

/// The pointer moved to a menu's middle, when the menu was moved onto the screen.
fn warp_pointer(mut pie: ResMut<crate::hud::PieMenu>, mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>) {
    if let Some(p) = pie.warp.take()
        && let Ok(mut w) = windows.single_mut()
    {
        w.set_cursor_position(Some(p));
    }
}

/// Over the menu between its bubbles: the one pointed towards lit, as if under the pointer.
#[allow(clippy::type_complexity)]
fn mask_hover(
    mask: Query<&Interaction, With<PieMask>>,
    pie: Res<crate::hud::PieMenu>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut options: Query<(&crate::hud::PieOption, &Interaction, Option<&PieText>, Option<&crate::layout::UiButton>, Option<&mut BackgroundColor>), Without<PieMask>>,
    mut colors: Query<&mut TextColor>,
    mut pics: Query<&mut ImageNode>,
    mut last: Local<Option<usize>>,
) {
    let over = mask.iter().any(|i| *i != Interaction::None);
    let toward = over.then(|| windows.single().ok().and_then(|w| w.cursor_position())).flatten().and_then(|c| option_toward(&pie.dirs, c - pie.at));
    if toward == *last {
        return;
    }
    *last = toward;
    for (o, i, text, button, bg) in &mut options {
        if *i != Interaction::None {
            continue;
        }
        let lit = toward == Some(o.0);
        if let Some(t) = text
            && let Ok(mut c) = colors.get_mut(t.0)
        {
            c.0 = crate::layout::color(if lit { HIGHLIGHT } else { REGULAR });
        }
        if let Some((p, h)) = button.and_then(|b| b.picture_for(if lit { 2 } else { 0 }))
            && let Ok(mut img) = pics.get_mut(p)
        {
            img.image = h;
        }
        if button.is_none()
            && let Some(mut bg) = bg
        {
            bg.0 = if lit { crate::hud::BUBBLE_HOVER } else { crate::hud::BUBBLE };
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
    fn the_pointer_picks_the_option_it_points_towards() {
        let dirs: Vec<Vec2> = places(4).into_iter().map(|p| p.0).collect();
        // (Above the middle, even off to the side a little: the top one.)
        assert_eq!(option_toward(&dirs, Vec2::new(20.0, -100.0)), Some(0));
        assert_eq!(option_toward(&dirs, Vec2::new(120.0, 10.0)), Some(1));
        assert_eq!(option_toward(&dirs, Vec2::new(0.0, 60.0)), Some(2));
        assert_eq!(option_toward(&dirs, Vec2::new(-90.0, -20.0)), Some(3));
        // (On the face in the middle: none.)
        assert_eq!(option_toward(&dirs, Vec2::new(10.0, -12.0)), None);
    }

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
