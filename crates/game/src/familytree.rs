//! A Sim's family tree in the game's own dialog (UI.dll's `FamilyTreeDialog` over
//! `HUDSimologyFamilyTreeDialog`), from the Simology panel's Family Tree button: the Sim's name,
//! their parents and grandparents above (each couple over their child), their brothers and
//! sisters beside them and their spouse at their side, their children and grandchildren below,
//! each in the game's thumb (`HUDSimologyFamilyTreeThumbItem`: their picture, the ellipsis when
//! more of their family is out of the picture) and joined by the game's lines; a click on anyone
//! puts them at the middle of the tree.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::AppState;
use crate::layout::{Spawned, UiAssets, UiButton};
use crate::sim::Sim;

pub struct FamilyTreePlugin;

impl Plugin for FamilyTreePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (scripted, family_tree).chain().run_if(in_state(crate::PlayMode::Live))).add_systems(OnExit(AppState::InGame), |mut commands: Commands| {
            commands.remove_resource::<TreeDialog>();
            commands.remove_resource::<OpenFamilyTree>();
        });
    }
}

/// FAMILY_TREE=1 (tests): the selected Sim's tree put up (12 seconds in), then a relative's (two
/// seconds later), with screenshots into FAMILY_TREE_SHOTS.
fn scripted(
    mut commands: Commands,
    time: Res<Time<crate::autotest::InputTimeline>>,
    sel: Query<&Sim, With<crate::sim::Selected>>,
    dialog: Option<Res<TreeDialog>>,
    family: Res<crate::family::Genealogy>,
    mut step: Local<u8>,
) {
    if std::env::var("FAMILY_TREE").is_err() {
        return;
    }
    let shot = |commands: &mut Commands, name: &str| {
        let dir = std::env::var("FAMILY_TREE_SHOTS").unwrap_or_else(|_| ".".into());
        let _ = std::fs::create_dir_all(&dir);
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(format!("{dir}/{name}.png")));
    };
    let t = time.elapsed_secs();
    match *step {
        0 if t > 12.0 => {
            let Ok(s) = sel.single() else { return };
            commands.insert_resource(OpenFamilyTree(s.id));
        }
        1 if t > 14.0 => shot(&mut commands, "ft1_tree"),
        2 if t > 15.0 => {
            // (A parent's tree, or a child's.)
            let Some(d) = dialog else { return };
            let other = family.parents(d.current).first().copied().or_else(|| family.children(d.current).first().copied());
            info!("family tree test: {} relatives, next {other:?}", family.0.len());
            if let Some(o) = other {
                commands.insert_resource(OpenFamilyTree(o));
            }
        }
        3 if t > 17.0 => shot(&mut commands, "ft2_relative"),
        _ => return,
    }
    *step += 1;
}

/// Asked for (by the Simology panel's button): the family tree of this Sim (by id).
#[derive(Resource, Clone, Copy)]
pub struct OpenFamilyTree(pub u64);

/// Whether the game's dialog can be put up.
pub fn available(ui: &UiAssets) -> bool {
    ui.layout("HUDSimologyFamilyTreeDialog").is_some() && ui.layout("HUDSimologyFamilyTreeThumbItem").is_some()
}

// `HUDSimologyFamilyTreeDialog`'s windows.
const NAME: u32 = 0x0693_3700;
const OKAY: u32 = 0x0693_3702;
const TITLE: u32 = 0x0693_3704;
const SCROLL: u32 = 0x0693_3705;
const THUMB_PARENT: u32 = 0x0693_3703;
// `HUDSimologyFamilyTreeThumbItem`'s.
const THUMB_PICTURE: u32 = 1;
const ELLIPSIS: u32 = 2;
/// The thumbs' size and spacing (`kRegularArea`, `X_DIST_BETWEEN_THUMBS`, `Y_DIST_...`).
const THUMB: f32 = 56.0;
const X_GAP: f32 = 15.0;
const Y_GAP: f32 = 37.0;
/// The dialog's least size, its margin round the tree, and where the tree starts under the name.
const MIN_SIZE: Vec2 = Vec2::new(339.0, 269.0);
const BUFFER: f32 = 55.0;
const TREE_TOP: f32 = 72.0;
/// The lines' colour (`DEFAULT_RELATIONSHIP_COLOR`).
const LINE: u32 = 0xff36_7cdd;
const LINE_WIDTH: f32 = 2.0;

/// The dialog up, for whom.
#[derive(Resource)]
struct TreeDialog {
    s: Spawned,
    current: u64,
}

/// A thumb in the tree: whose.
#[derive(Component)]
struct TreeThumb(u64);

/// Someone in the tree: where (generation, x), and whether more of their family is off the
/// tree.
struct Placed {
    row: i32,
    x: f32,
    more: bool,
}

/// The tree around `me`: two generations up (each couple over their child), brothers and sisters
/// to the left, the spouse to the right, two generations down (each under their parents).
fn lay_out(me: u64, family: &crate::family::Genealogy, spouse_of: &dyn Fn(u64) -> Option<u64>) -> (HashMap<u64, Placed>, Vec<(Vec2, Vec2)>) {
    let mut at: HashMap<u64, Placed> = HashMap::new();
    let step = THUMB + X_GAP;
    // Ancestors, placed left to right (a couple's child centred under them).
    fn up(id: u64, row: i32, level: u32, cursor: &mut f32, family: &crate::family::Genealogy, at: &mut HashMap<u64, Placed>) -> f32 {
        let parents: Vec<u64> = family.parents(id).iter().copied().filter(|p| !at.contains_key(p)).collect();
        let x = if level == 0 || parents.is_empty() {
            let x = *cursor;
            *cursor += THUMB + X_GAP;
            x
        } else {
            let xs: Vec<f32> = parents.iter().take(2).map(|p| up(*p, row - 1, level - 1, cursor, family, at)).collect();
            (xs[0] + xs[xs.len() - 1]) * 0.5
        };
        let more = level == 0 && !family.parents(id).is_empty();
        at.insert(id, Placed { row, x, more });
        x
    }
    let mut cursor = 0.0;
    let me_x = up(me, 0, 2, &mut cursor, family, &mut at);
    // Brothers and sisters to the left, the spouse to the right.
    let siblings: Vec<u64> = family.siblings_of(me).into_iter().filter(|s| !at.contains_key(s)).collect();
    for (k, s) in siblings.into_iter().enumerate() {
        at.insert(s, Placed { row: 0, x: me_x - step * (k as f32 + 1.0), more: !family.children(s).is_empty() });
    }
    let spouse = spouse_of(me).filter(|s| !at.contains_key(s));
    if let Some(s) = spouse {
        at.insert(s, Placed { row: 0, x: me_x + step, more: !family.parents(s).is_empty() });
    }
    // Descendants: each family's width, then placed under its parents.
    fn width(id: u64, level: u32, family: &crate::family::Genealogy) -> f32 {
        let kids = family.children(id);
        if level == 0 || kids.is_empty() {
            return THUMB;
        }
        (kids.iter().map(|k| width(*k, level - 1, family)).sum::<f32>() + X_GAP * (kids.len() as f32 - 1.0)).max(THUMB)
    }
    fn down(id: u64, row: i32, level: u32, centre: f32, family: &crate::family::Genealogy, at: &mut HashMap<u64, Placed>) {
        let kids: Vec<u64> = family.children(id).into_iter().filter(|k| !at.contains_key(k)).collect();
        if level == 0 || kids.is_empty() {
            return;
        }
        let total = kids.iter().map(|k| width(*k, level - 1, family)).sum::<f32>() + X_GAP * (kids.len() as f32 - 1.0);
        let mut x = centre - total * 0.5;
        for k in kids {
            let w = width(k, level - 1, family);
            let kx = x + (w - THUMB) * 0.5;
            at.insert(k, Placed { row, x: kx, more: level == 1 && !family.children(k).is_empty() });
            down(k, row + 1, level - 1, kx + THUMB * 0.5, family, at);
            x += w + X_GAP;
        }
    }
    let centre = me_x + THUMB * 0.5 + if spouse.is_some() { step * 0.5 } else { 0.0 };
    down(me, 1, 2, centre, family, &mut at);
    // The lines: a couple joined side by side; parents down to their children (over a bar when
    // there are several).
    let mut lines = Vec::new();
    let mid = |p: &Placed| Vec2::new(p.x + THUMB * 0.5, row_y(p.row) + THUMB * 0.5);
    let mut families: HashMap<Vec<u64>, Vec<u64>> = HashMap::new();
    for (id, _) in at.iter() {
        let parents: Vec<u64> = family.parents(*id).iter().copied().filter(|p| at.contains_key(p)).collect();
        if !parents.is_empty() {
            let mut key = parents;
            key.sort();
            families.entry(key).or_default().push(*id);
        }
    }
    // (Brothers and sisters known without their parents hang off the same bar as the Sim.)
    let sibs: Vec<u64> = family.siblings_of(me).into_iter().filter(|s| at.contains_key(s) && family.parents(*s).iter().all(|p| !at.contains_key(p))).collect();
    if !sibs.is_empty() {
        let mut key: Vec<u64> = family.parents(me).iter().copied().filter(|p| at.contains_key(p)).collect();
        key.sort();
        let e = families.entry(key).or_default();
        e.extend(sibs);
        if !e.contains(&me) {
            e.push(me);
        }
    }
    for (parents, kids) in &families {
        let tops: Vec<Vec2> = kids.iter().map(|k| mid(&at[k]) - Vec2::Y * THUMB * 0.5).collect();
        let bar_y = tops[0].y - Y_GAP * 0.5;
        let from = match parents.as_slice() {
            [] => None,
            [p] => Some(mid(&at[p]) + Vec2::Y * THUMB * 0.5),
            [a, b, ..] => {
                let (a, b) = (mid(&at[a]), mid(&at[b]));
                lines.push((a, b));
                Some((a + b) * 0.5)
            }
        };
        let xs: Vec<f32> = tops.iter().map(|t| t.x).chain(from.map(|f| f.x)).collect();
        let (lo, hi) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
        if let Some(f) = from {
            lines.push((f, Vec2::new(f.x, bar_y)));
        }
        if hi > lo {
            lines.push((Vec2::new(lo, bar_y), Vec2::new(hi, bar_y)));
        }
        for t in tops {
            lines.push((Vec2::new(t.x, bar_y), t));
        }
    }
    if let Some(s) = spouse {
        lines.push((mid(&at[&me]), mid(&at[&s])));
    }
    (at, lines)
}

/// The top of a generation's row (the Sim's own is row 0).
fn row_y(row: i32) -> f32 {
    (row + 2) as f32 * (THUMB + Y_GAP)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn family_tree(
    mut commands: Commands,
    dialog: Option<Res<TreeDialog>>,
    asked: Option<Res<OpenFamilyTree>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    family: Res<crate::family::Genealogy>,
    people: Query<(Entity, &Sim, &crate::social::Relationships)>,
    mut portraits: ResMut<crate::portraits::Portraits>,
    (clicks, thumbs): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &TreeThumb), Changed<Interaction>>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let close = |commands: &mut Commands, d: &TreeDialog| {
        if let Some(r) = d.s.root {
            commands.entity(r).despawn();
        }
        commands.remove_resource::<TreeDialog>();
    };
    // OK closes it; a click on someone puts them in the middle.
    let mut want = asked.as_deref().map(|a| a.0);
    if let Some(d) = dialog.as_deref() {
        if crate::livehud::pressed(&clicks, d.s.id(OKAY)) {
            close(&mut commands, d);
            return;
        }
        if let Some((_, t)) = thumbs.iter().find(|(i, _)| **i == Interaction::Pressed)
            && t.0 != d.current
        {
            want = Some(t.0);
            play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
        }
    }
    let Some(me) = want else { return };
    commands.remove_resource::<OpenFamilyTree>();
    if let Some(d) = dialog.as_deref() {
        close(&mut commands, d);
    }
    if !available(&ui) {
        return;
    }
    let entity_of = |id: u64| people.iter().find(|(_, s, _)| s.id == id);
    let spouse_of = |id: u64| -> Option<u64> {
        let (_, _, rels) = entity_of(id)?;
        rels.0.iter().find(|(_, r)| r.status == crate::social::RelStatus::Married).and_then(|(e, _)| people.get(*e).ok()).map(|(_, s, _)| s.id)
    };
    let name_of = |id: u64| entity_of(id).map(|(_, s, _)| s.full_name()).or_else(|| family.0.get(&id).map(|p| p.name.clone())).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Unknown".into());
    let (at, lines) = lay_out(me, &family, &spouse_of);
    // The tree's extent, and the dialog sized round it (as `RefreshTree` sizes it).
    let lo_x = at.values().map(|p| p.x).fold(f32::MAX, f32::min);
    let hi_x = at.values().map(|p| p.x + THUMB).fold(f32::MIN, f32::max);
    let lo_row = at.values().map(|p| p.row).min().unwrap_or(0);
    let hi_row = at.values().map(|p| p.row).max().unwrap_or(0);
    let (tree_w, tree_h) = (hi_x - lo_x, row_y(hi_row) + THUMB - row_y(lo_row));
    let size = Vec2::new((tree_w + 2.0 * BUFFER).max(MIN_SIZE.x), (TREE_TOP + tree_h + 2.0 * BUFFER).max(MIN_SIZE.y));
    let Some(mut w) = ui.layout("HUDSimologyFamilyTreeDialog").cloned() else { return };
    w.area = [0.0, 0.0, size.x, size.y];
    w.place = s3bake::ui::UiPlace::Center(0.5);
    w.flags |= s3bake::ui::WIN_VISIBLE;
    let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
    let Some(root) = s.root else { return };
    commands.entity(root).insert((GlobalZIndex(22), crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(AppState::InGame)));
    if let Some(t) = s.text(NAME) {
        commands.entity(t).insert(Text::new(name_of(me)));
    }
    if let Some(t) = s.text(TITLE) {
        commands.entity(t).insert(Text::new(ui.localize("Ui/Caption/HUD/FamilyTree:Title").unwrap_or_else(|| "Family Tree".into())));
    }
    if let Some(e) = s.id(OKAY) {
        commands.entity(e).insert(crate::icons::Tooltip("OK".into()));
    }
    for id in [SCROLL, THUMB_PARENT] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    // The tree, centred under the name.
    let origin = Vec2::new((size.x - tree_w) * 0.5 - lo_x, TREE_TOP + ((size.y - TREE_TOP) - tree_h) * 0.5 - row_y(lo_row));
    let holder = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Pickable::IGNORE, ChildOf(root))).id();
    let colour = crate::layout::color(LINE);
    for (a, b) in lines {
        let (a, b) = (a + origin, b + origin);
        let (l, t) = (a.x.min(b.x) - LINE_WIDTH * 0.5, a.y.min(b.y) - LINE_WIDTH * 0.5);
        commands.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(l), top: Val::Px(t), width: Val::Px((a.x - b.x).abs() + LINE_WIDTH), height: Val::Px((a.y - b.y).abs() + LINE_WIDTH), ..default() },
            BackgroundColor(colour),
            Pickable::IGNORE,
            ChildOf(holder),
        ));
    }
    let Some(template) = ui.layout("HUDSimologyFamilyTreeThumbItem").cloned() else { return };
    let fallback = ui.image(&mut images, s3pkg::fnv64(crate::caslook::AGE_ICONS[5])).map(|(h, _)| h);
    for (id, p) in &at {
        let mut tw = template.clone();
        let (x, y) = (origin.x + p.x, origin.y + row_y(p.row));
        tw.area = [x, y, x + THUMB, y + THUMB];
        tw.place = s3bake::ui::UiPlace::Fixed;
        tw.cls = "Button".into();
        let t = ui.spawn_under(&mut commands, &mut images, &mut fonts, &tw, holder);
        let Some(te) = t.root else { continue };
        let word = if *id == me { String::new() } else { family.word(me, *id).map(|w| format!("\n{w}")).unwrap_or_default() };
        commands.entity(te).insert((TreeThumb(*id), crate::icons::Tooltip(format!("{}{word}", name_of(*id)))));
        if *id == me {
            commands.entity(te).queue(|mut e: EntityWorldMut| {
                if let Some(mut b) = e.get_mut::<UiButton>() {
                    b.selected = true;
                }
            });
        }
        if let Some(win) = t.id(THUMB_PICTURE) {
            match entity_of(*id) {
                Some((e, _, _)) => {
                    let h = portraits.portrait(&mut images, e);
                    commands.entity(win).despawn_children();
                    commands.entity(win).with_children(|c| {
                        c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(e), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
                    });
                }
                None => {
                    if let Some(h) = fallback.clone() {
                        crate::hudpanels::picture(&mut commands, win, h, Color::WHITE);
                    }
                }
            }
        }
        if let Some(e) = t.id(ELLIPSIS) {
            commands.entity(e).insert(if p.more { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    commands.insert_resource(TreeDialog { s, current: me });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_generations_laid_out() {
        let mut g = crate::family::Genealogy::default();
        // Grandparents 1, 2 (of father 3) and 4, 5 (of mother 6); me 7, a sister 8; children 9, 10.
        for (c, p) in [(3, 1), (3, 2), (6, 4), (6, 5), (7, 3), (7, 6), (8, 3), (8, 6), (9, 7), (10, 7)] {
            g.add_parent(c, p);
        }
        let (at, lines) = lay_out(7, &g, &|id| (id == 7).then_some(11));
        for id in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11] {
            assert!(at.contains_key(&id), "{id} on the tree");
        }
        assert_eq!(at[&1].row, -2);
        assert_eq!(at[&3].row, -1);
        assert_eq!(at[&9].row, 1);
        // (The father between his parents, me under my parents' middle, the sister to my left,
        // the spouse to my right.)
        assert!((at[&3].x - (at[&1].x + at[&2].x) * 0.5).abs() < 0.1);
        assert!((at[&7].x - (at[&3].x + at[&6].x) * 0.5).abs() < 0.1);
        assert!(at[&8].x < at[&7].x && at[&11].x > at[&7].x);
        // (No two thumbs on a row overlap.)
        for (a, pa) in &at {
            for (b, pb) in &at {
                if a != b && pa.row == pb.row {
                    assert!((pa.x - pb.x).abs() >= THUMB, "{a} and {b} overlap");
                }
            }
        }
        assert!(!lines.is_empty());
    }
}
