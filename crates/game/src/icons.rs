//! The game's interface icons and the words that go with its moodlets, traits and skills
//! (baked from the game's own tuning tables by `s3bake::gamedata`), and hover tooltips.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use s3bake::GameDataBaked;

use crate::menu::text;

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (open_game_ui, tooltips));
    }
}

/// Moodlet, trait and skill data and their icons.
#[derive(Resource)]
pub struct GameUi {
    pub data: Arc<GameDataBaked>,
    icons: Option<Arc<s3bake::Icons>>,
    cache: HashMap<String, Option<Handle<Image>>>,
}

impl GameUi {
    /// An icon by name, decoded once.
    pub fn icon(&mut self, images: &mut Assets<Image>, name: &str) -> Option<Handle<Image>> {
        if name.is_empty() {
            return None;
        }
        if let Some(h) = self.cache.get(name) {
            return h.clone();
        }
        let img = self.icons.as_ref().and_then(|i| i.png(name)).and_then(|png| s3bake::gamedata::decode_icon(&png)).map(|(w, h, px)| {
            Image::new(
                Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                TextureDimension::D2,
                px,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            )
        });
        let h = img.map(|i| images.add(i));
        self.cache.insert(name.to_string(), h.clone());
        h
    }

    /// An icon's PNG bytes (for images that need their own sampler).
    pub fn png(&self, name: &str) -> Option<Vec<u8>> {
        self.icons.as_ref()?.png(name)
    }

    /// The game's name, description and icon of a trait (the pets' version lends only its icon).
    pub fn trait_info(&self, t: crate::life::Trait) -> Option<s3bake::gamedata::TraitInfo> {
        let mut info = self.data.trait_info(&t.game_id())?.clone();
        if info.hex.ends_with("Pet") {
            info.name = t.name().to_string();
            info.desc.clear();
        }
        Some(info)
    }

    /// The game's name, description and icon for a moodlet; only the icon when the game's
    /// buff is a near match rather than the same thing.
    pub fn moodlet_info(&self, k: crate::life::MoodletKind) -> (String, String, String) {
        let d = k.def();
        let (hex, same) = k.buff();
        match self.data.buff(hex) {
            Some(b) if same && !b.name.is_empty() => (b.name.clone(), b.desc.clone(), b.icon.clone()),
            Some(b) => (d.name.to_string(), d.desc.to_string(), b.icon.clone()),
            None => (d.name.to_string(), d.desc.to_string(), String::new()),
        }
    }
}

fn open_game_ui(mut commands: Commands, existing: Option<Res<GameUi>>, mut tried: Local<f32>, time: Res<Time>) {
    if existing.is_some() || time.elapsed_secs() < *tried {
        return;
    }
    // Retried every few seconds until the first-run conversion has made the data.
    *tried = time.elapsed_secs() + 3.0;
    let root = s3bake::default_root();
    let Some(data) = s3bake::load_gamedata(&root) else { return };
    commands.insert_resource(GameUi { data: Arc::new(data), icons: s3bake::Icons::open(&root).map(Arc::new), cache: HashMap::new() });
}

/// Text shown when the pointer rests on this UI element.
#[derive(Component, Clone)]
pub struct Tooltip(pub String);

#[derive(Component)]
struct TooltipBox;

/// A UI image of an icon at a size.
pub fn icon_bundle(h: Handle<Image>, size: f32) -> impl Bundle {
    (ImageNode::new(h), Node { width: Val::Px(size), height: Val::Px(size), ..default() })
}

fn tooltips(
    mut commands: Commands,
    hovered: Query<(&Interaction, &Tooltip, Option<&InheritedVisibility>, Option<&ComputedNode>)>,
    boxes: Query<(Entity, &ComputedNode), With<TooltipBox>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut shown: Local<Option<String>>,
) {
    let cursor = windows.single().ok().filter(|w| w.focused).and_then(|w| w.cursor_position().map(|c| (c, w.width(), w.height())));
    let want = cursor.and_then(|_| hovered.iter()
        .find(|(i, _, visible, layout)| **i == Interaction::Hovered && visible.is_none_or(|v| v.get())
            && layout.is_none_or(|n| n.size().x > 0.0 && n.size().y > 0.0))
        .map(|(_, t, _, _)| t.0.clone()));
    if want == *shown {
        // Follow the pointer.
        if let (Some((c, w, h)), Ok((b, node))) = (cursor, boxes.single()) {
            commands.entity(b).insert(place(c, w, h, node.size() * node.inverse_scale_factor()));
        }
        return;
    }
    for (b, _) in &boxes {
        commands.entity(b).despawn();
    }
    *shown = want.clone();
    let (Some(t), Some((c, w, h))) = (want, cursor) else { return };
    commands
        .spawn((
            TooltipBox,
            GlobalZIndex(100),
            Pickable::IGNORE,
            place(c, w, h, Vec2::new(320.0, 120.0)),
            BackgroundColor(Color::srgba(0.04, 0.10, 0.20, 0.96)),
            BorderColor::all(Color::srgba(0.5, 0.75, 1.0, 0.8)),
        ))
        .with_children(|b| {
            b.spawn((text(t, 14.0, Color::WHITE), Pickable::IGNORE));
        });
}

#[cfg(test)]
mod tooltip_tests {
    use super::*;

    fn count(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<TooltipBox>>().iter(world).count()
    }

    #[test]
    fn measured_tooltips_fit_near_every_window_edge() {
        for window in [Vec2::new(800.0, 600.0), Vec2::new(360.0, 640.0)] {
            for size in [Vec2::new(120.0, 40.0), Vec2::new(320.0, 280.0)] {
                for cursor in [Vec2::ZERO, window, window * 0.5, Vec2::new(window.x - 1.0, 10.0), Vec2::new(10.0, window.y - 1.0)] {
                    let p = tooltip_position(cursor, window, size);
                    assert!(p.x >= 8.0 && p.y >= 8.0);
                    assert!(p.x + size.x <= window.x - 8.0);
                    assert!(p.y + size.y <= window.y - 8.0);
                }
            }
        }
        let p = tooltip_position(Vec2::new(400.0, 400.0), Vec2::new(800.0, 600.0), Vec2::new(320.0, 280.0));
        assert!(p.y + 280.0 < 400.0, "tall tooltip should flip above the pointer");
        assert_eq!(place(Vec2::ZERO, 240.0, 400.0, Vec2::ZERO).max_width, Val::Px(224.0));
    }

    #[test]
    fn stale_hover_hides_on_pointer_exit_and_reappears_on_return() {
        let mut app = App::new();
        app.add_systems(Update, tooltips);
        let mut window = Window::default();
        window.focused = true;
        window.set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        let win = app.world_mut().spawn((window, PrimaryWindow)).id();
        let control = app.world_mut().spawn((Interaction::Hovered, Tooltip("Test".into()), InheritedVisibility::VISIBLE)).id();
        app.update();
        assert_eq!(count(&mut app), 1);
        app.world_mut().get_mut::<Window>(win).unwrap().set_cursor_position(None);
        app.update();
        assert_eq!(count(&mut app), 0, "stale hovered state must not leave a tooltip behind");
        app.world_mut().get_mut::<Window>(win).unwrap().set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        app.update();
        assert_eq!(count(&mut app), 1);
        app.world_mut().entity_mut(control).insert(InheritedVisibility::HIDDEN);
        app.update();
        assert_eq!(count(&mut app), 0);
        app.world_mut().entity_mut(control).insert(InheritedVisibility::VISIBLE);
        app.update();
        assert_eq!(count(&mut app), 1);
        app.world_mut().entity_mut(control).insert(ComputedNode::default());
        app.update();
        assert_eq!(count(&mut app), 0, "collapsed layout must not show stale tooltips");
        app.world_mut().entity_mut(control).remove::<ComputedNode>();
        app.update();
        assert_eq!(count(&mut app), 1);
        app.world_mut().get_mut::<Window>(win).unwrap().focused = false;
        app.update();
        assert_eq!(count(&mut app), 0);
    }
}

/// A tooltip's box near the pointer, kept on screen.
fn place(c: Vec2, w: f32, h: f32, size: Vec2) -> Node {
    let position = tooltip_position(c, Vec2::new(w, h), size);
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(position.x),
        top: Val::Px(position.y),
        max_width: Val::Px(320.0_f32.min((w - 16.0).max(0.0))),
        padding: UiRect::all(Val::Px(8.0)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    }
}

fn tooltip_position(cursor: Vec2, window: Vec2, size: Vec2) -> Vec2 {
    let after = cursor + Vec2::new(16.0, 18.0);
    let before = cursor - size - Vec2::splat(12.0);
    let axis = |after: f32, before: f32, extent: f32, bound: f32| {
        let max = (bound - extent - 8.0).max(0.0);
        let position = if after + extent + 8.0 <= bound { after } else { before };
        position.clamp(8.0_f32.min(max), max)
    };
    Vec2::new(axis(after.x, before.x, size.x, window.x), axis(after.y, before.y, size.y, window.y))
}
