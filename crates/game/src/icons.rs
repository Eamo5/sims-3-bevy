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
    hovered: Query<(&Interaction, &Tooltip)>,
    boxes: Query<Entity, With<TooltipBox>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut shown: Local<Option<String>>,
) {
    let want = hovered.iter().find(|(i, _)| **i == Interaction::Hovered).map(|(_, t)| t.0.clone());
    let cursor = windows.single().ok().and_then(|w| w.cursor_position().map(|c| (c, w.width(), w.height())));
    if want == *shown {
        // Follow the pointer.
        if let (Some((c, w, h)), Ok(b)) = (cursor, boxes.single()) {
            commands.entity(b).insert(place(c, w, h));
        }
        return;
    }
    for b in &boxes {
        commands.entity(b).despawn();
    }
    *shown = want.clone();
    let (Some(t), Some((c, w, h))) = (want, cursor) else { return };
    commands
        .spawn((
            TooltipBox,
            GlobalZIndex(100),
            Pickable::IGNORE,
            place(c, w, h),
            BackgroundColor(Color::srgba(0.04, 0.10, 0.20, 0.96)),
            BorderColor::all(Color::srgba(0.5, 0.75, 1.0, 0.8)),
        ))
        .with_children(|b| {
            b.spawn((text(t, 14.0, Color::WHITE), Pickable::IGNORE));
        });
}

/// A tooltip's box near the pointer, kept on screen.
fn place(c: Vec2, w: f32, h: f32) -> Node {
    let (x, y) = (c.x + 16.0, c.y + 18.0);
    let mut n = Node {
        position_type: PositionType::Absolute,
        max_width: Val::Px(320.0),
        padding: UiRect::all(Val::Px(8.0)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    };
    if x > w - 330.0 {
        n.right = Val::Px((w - c.x + 12.0).max(0.0));
    } else {
        n.left = Val::Px(x);
    }
    if y > h - 120.0 {
        n.bottom = Val::Px((h - c.y + 12.0).max(0.0));
    } else {
        n.top = Val::Px(y);
    }
    n
}
