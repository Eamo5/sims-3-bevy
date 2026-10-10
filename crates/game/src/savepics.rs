//! A saved game's own pictures, as the game keeps with its saves (`mLotThumbnail`,
//! `mHouseholdThumbnail`): the household's lot, seen from the way the player was looking, and
//! the family, their portraits side by side. They're written beside the save (`<save>.png`,
//! `<save>.family.png`) when it's saved, for the main menu's save strip and the loading screen.

use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};

use crate::sim::HouseholdMember;

pub struct SavePicsPlugin;

impl Plugin for SavePicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (on_save, lot_picture).chain().run_if(in_state(crate::AppState::InGame)));
    }
}

/// The lot picture's size, and how long its camera looks before the picture is taken.
const SIZE: u32 = 256;
const FRAMES: u32 = 4;

/// The lot being pictured: its camera, the picture, where it goes, frames waited.
#[derive(Component)]
struct LotShot {
    image: Handle<Image>,
    path: PathBuf,
    frames: u32,
}

/// When a game's been saved: pictures of its lot and family.
#[allow(clippy::too_many_arguments)]
fn on_save(
    mut commands: Commands,
    last: Option<Res<crate::save::LastSave>>,
    slot: Res<crate::save::SaveSlot>,
    (household, world): (Option<Res<crate::interact::Household>>, Res<crate::loading::CurrentWorld>),
    main_cam: Query<(&GlobalTransform, Option<&bevy::camera::visibility::RenderLayers>, Option<&AmbientLight>, Option<&DistanceFog>, Option<&Projection>), With<crate::camera::SimsCamera>>,
    members: Query<Entity, With<HouseholdMember>>,
    (mut portraits, mut images): (ResMut<crate::portraits::Portraits>, ResMut<Assets<Image>>),
) {
    let Some(last) = last else { return };
    if !last.is_changed() || last.is_added() && slot.0.is_none() {
        return;
    }
    let Some(path) = slot.0.clone() else { return };
    // The lot, from the way the camera's looking, its whole width in view.
    if let (Some(h), Ok((cam, layers, ambient, fog, projection))) = (household.as_ref(), main_cam.single())
        && let Some(lot) = world.data.lots.get(h.lot_index)
    {
        let corners = crate::home::lot_corners(lot);
        let mid = corners.iter().fold(Vec3::ZERO, |a, b| a + *b) / 4.0;
        let ground = world.data.heightmap.sample(mid.x, mid.z);
        let size = corners.iter().map(|c| c.distance(mid)).fold(0.0, f32::max);
        let forward = cam.forward().as_vec3();
        let look = Vec3::new(mid.x, ground + 1.0, mid.z);
        let eye = look - forward * (size * 1.25).max(12.0);
        let handle = images.add(target_image());
        let mut c = commands.spawn((
            Camera3d::default(),
            Camera { order: -5, clear_color: ClearColorConfig::Custom(Color::srgb(0.55, 0.7, 0.9)), ..default() },
            RenderTarget::Image(handle.clone().into()),
            Transform::from_translation(eye).looking_at(look, Vec3::Y),
            LotShot { image: handle, path: path.with_extension("png"), frames: 0 },
            DespawnOnExit(crate::AppState::InGame),
        ));
        // (Seen as the player sees it: the same light, haze and lens.)
        if let Some(l) = layers {
            c.insert(l.clone());
        }
        if let Some(a) = ambient {
            c.insert(a.clone());
        }
        if let Some(f) = fog {
            c.insert(f.clone());
        }
        if let Some(p) = projection {
            c.insert(p.clone());
        }
    }
    // The family: their portraits laid side by side by a camera of their own (two rows for
    // more than four), on the portraits' blue.
    let faces: Vec<Handle<Image>> = members.iter().take(8).map(|e| portraits.portrait(&mut images, e)).collect();
    if faces.is_empty() {
        return;
    }
    let handle = images.add(target_image());
    let cam = commands
        .spawn((
            Camera2d,
            Camera { order: -6, clear_color: ClearColorConfig::Custom(Color::srgb_u8(143, 184, 224)), ..default() },
            RenderTarget::Image(handle.clone().into()),
            LotShot { image: handle, path: path.with_extension("family.png"), frames: 0 },
            DespawnOnExit(crate::AppState::InGame),
        ))
        .id();
    let n = faces.len();
    let cols = if n <= 4 { n } else { n.div_ceil(2) };
    let cell = SIZE as f32 / cols.max(n.div_ceil(cols)) as f32;
    commands
        .spawn((
            Node { width: Val::Px(SIZE as f32), height: Val::Px(SIZE as f32), flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, align_content: AlignContent::Center, ..default() },
            UiTargetCamera(cam),
            FamilyCard(cam),
            DespawnOnExit(crate::AppState::InGame),
        ))
        .with_children(|p| {
            for f in faces {
                p.spawn((ImageNode::new(f), Node { width: Val::Px(cell), height: Val::Px(cell), ..default() }));
            }
        });
}

/// The family's card (put away with its camera).
#[derive(Component)]
struct FamilyCard(Entity);

/// A picture a camera draws into, which can be copied out.
fn target_image() -> Image {
    let mut image = Image::new_fill(Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT;
    image
}

/// The lot's camera looks for a few frames (for everything to be drawn), then its picture is
/// taken and it's put away.
fn lot_picture(mut commands: Commands, mut shots: Query<(Entity, &mut LotShot)>, cards: Query<(Entity, &FamilyCard)>) {
    for (e, mut s) in &mut shots {
        s.frames += 1;
        if s.frames == FRAMES {
            let path = s.path.clone();
            commands.spawn(Screenshot::image(s.image.clone())).observe(move |shot: On<ScreenshotCaptured>| {
                if let Ok(d) = shot.image.clone().try_into_dynamic()
                    && let Err(e) = d.to_rgb8().save(&path)
                {
                    warn!("lot picture: {e}");
                }
            });
        }
        if s.frames > FRAMES + 2 {
            for (card, c) in &cards {
                if c.0 == e {
                    commands.entity(card).despawn();
                }
            }
            commands.entity(e).despawn();
        }
    }
}
