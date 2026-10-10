//! The loading screen in the game's own look (`GameLoadingScreen`, UI.package), as UI.dll's
//! `LoadingScreenController` shows it: the town's own loading picture behind (covering the
//! screen), "Welcome to <town>!" for a new game or the household's name for a saved one, a
//! game tip changed every ten seconds, and the load bar filling. On a first run what's being
//! converted is written under the bar.

use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::AppState;
use crate::layout::{Spawned, UiAssets, UiFillBar};

pub struct LoadScreenPlugin;

impl Plugin for LoadScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Loading), spawn_screen.after(crate::loading::start_loading))
            .add_systems(Update, (update_screen, cover).run_if(in_state(AppState::Loading)).run_if(resource_exists::<LoadScreen>))
            .add_systems(OnExit(AppState::Loading), |mut commands: Commands| commands.remove_resource::<LoadScreen>());
    }
}

const TIP: u32 = 0x06eb_5201;
const TITLE: u32 = 0x06eb_5211;
const BAR: u32 = 0x06eb_5200;
const LOT_IMAGE: u32 = 0x06eb_5210;
const FAMILY_IMAGE: u32 = 0x06eb_5212;
/// Seconds a tip stays (`kSecondsBetweenGameTips`), and about how long a load takes (the bar's
/// pace: it fills most of the way in this, then creeps).
const TIP_SECONDS: f32 = 10.0;
const LOAD_SECONDS: f32 = 12.0;

#[derive(Resource)]
pub struct LoadScreen {
    s: Spawned,
    started: f32,
    tip_at: f32,
    unseen: Vec<String>,
    status: Option<Entity>,
}

/// The town's loading picture (by its world file's name), as the controller picks it.
fn loading_picture(world_file: &str) -> Option<&'static str> {
    Some(match world_file.to_ascii_lowercase().as_str() {
        "twinbrook" => "world_loading_twinbrook",
        "bridgeport" => "world_loading_bridgeport",
        "appaloosaplains" => "ep5_world_loading_screen",
        "starlight shores" => "world_loading_EP6World",
        "moonlight falls" => "world_loading_EP7World",
        "islaparadiso" => "ep10_world_loading_screen",
        "sims university" => "world_loading_university",
        "oasis landing" => "world_loading_future",
        "china" => "world_loading_beijing",
        "france" => "world_loading_paris",
        "egypt" => "world_loading_cairo",
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn spawn_screen(
    mut commands: Commands,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    selected: Option<Res<crate::data::SelectedWorld>>,
    (save, slot): (Option<Res<crate::save::PendingLoad>>, Option<Res<crate::save::SaveSlot>>),
    old: Query<Entity, With<crate::loading::OldLoading>>,
    time: Res<Time>,
) {
    let Some(mut ui) = ui else { return };
    let Some(s) = ui.spawn(&mut commands, &mut images, &mut fonts, "GameLoadingScreen") else { return };
    let Some(root) = s.root else { return };
    commands.entity(root).insert((DespawnOnExit(AppState::Loading), GlobalZIndex(20), CoverPicture));
    for e in &old {
        commands.entity(e).insert(Visibility::Hidden);
    }
    // The town's picture and the welcome (or the household's name).
    let file = selected.as_ref().and_then(|w| w.0.path.file_stem().map(|s| s.to_string_lossy().into_owned())).unwrap_or_default();
    if let Some((h, _)) = loading_picture(&file).and_then(|n| ui.image(&mut images, s3pkg::fnv64(n))) {
        commands.entity(root).insert(SetRootPicture(h));
    }
    let title = match &save {
        Some(g) => g.0.household.clone(),
        None => {
            let town = selected.as_ref().map(|w| w.0.name.clone()).unwrap_or_default();
            ui.localize("Ui/Caption/GameEntry/LoadingScreen:WelcomeText").map_or_else(|| format!("Welcome to {town}!"), |t| t.replace("{0.String}", &town))
        }
    };
    if let Some(t) = s.text(TITLE) {
        commands.entity(t).insert(Text::new(title));
    }
    // A saved game's own pictures of its lot and family, if it has them (else the layout's).
    if let Some(slot) = save.as_ref().and(slot.as_ref()).and_then(|p| p.0.clone()) {
        for (id, suffix) in [(LOT_IMAGE, "png"), (FAMILY_IMAGE, "family.png")] {
            if let (Some(e), Some(h)) = (s.id(id), crate::mainmenu::picture_file(&mut images, &slot.with_extension(suffix))) {
                crate::hudpanels::picture(&mut commands, e, h, Color::WHITE);
            }
        }
    }
    // What's being converted, under the bar.
    let status = s.id(BAR).map(|bar| {
        let (font, line) = ui.text_font(&mut fonts, 0);
        commands
            .spawn((Text::new(""), font, line, TextColor(Color::WHITE), TextLayout::new(Justify::Center, LineBreak::WordBoundary), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(48.0), ..default() }, ChildOf(bar)))
            .id()
    });
    let mut unseen = ui.baked().tips.clone();
    unseen.reverse();
    let t = time.elapsed_secs();
    commands.insert_resource(LoadScreen { s, started: t, tip_at: t - TIP_SECONDS, unseen, status });
}

/// A new tip every ten seconds (each seen once before any again), the bar filling.
fn update_screen(
    mut p: ResMut<LoadScreen>,
    time: Res<Time>,
    ui: Option<Res<UiAssets>>,
    mut texts: Query<&mut Text, Without<crate::loading::ProgressText>>,
    mut bars: Query<&mut UiFillBar>,
    progress: Query<&Text, With<crate::loading::ProgressText>>,
) {
    let t = time.elapsed_secs();
    if t - p.tip_at >= TIP_SECONDS {
        p.tip_at = t;
        if p.unseen.is_empty()
            && let Some(ui) = ui.as_ref()
        {
            p.unseen = ui.baked().tips.clone();
        }
        let pick = p.unseen.choose(&mut rand::rng()).cloned();
        if let Some(tip) = pick {
            p.unseen.retain(|x| *x != tip);
            set(&mut texts, p.s.text(TIP), &tip);
        }
    }
    let elapsed = t - p.started;
    let v = 0.9 * (1.0 - (-elapsed / LOAD_SECONDS * 2.0).exp()) + 0.1 * (elapsed / (LOAD_SECONDS * 20.0)).min(1.0);
    if let Some(mut b) = p.s.id(BAR).and_then(|e| bars.get_mut(e).ok())
        && (b.value - v).abs() > 0.002
    {
        b.value = v.min(0.99);
    }
    // (The first run's conversion, word for word.)
    let status = progress.iter().next().map(|s| s.0.clone()).unwrap_or_default();
    let show = if status.starts_with("First run") || status.starts_with("Converting") { status } else { String::new() };
    set(&mut texts, p.status, &show);
}

fn set(texts: &mut Query<&mut Text, Without<crate::loading::ProgressText>>, e: Option<Entity>, s: &str) {
    if let Some(mut t) = e.and_then(|e| texts.get_mut(e).ok())
        && t.0 != s
    {
        t.0 = s.to_string();
    }
}

/// The root's picture: the town's own.
#[derive(Component)]
struct SetRootPicture(Handle<Image>);

#[derive(Component)]
struct CoverPicture;

/// The picture behind covers the screen, its shape kept.
fn cover(
    mut commands: Commands,
    q: Query<(Entity, &crate::layout::UiPicture, Option<&SetRootPicture>), With<CoverPicture>>,
    mut pics: Query<(&mut ImageNode, &mut Node)>,
    images: Res<Assets<Image>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(w) = windows.single() else { return };
    for (e, p, set) in &q {
        let Ok((mut img, mut node)) = pics.get_mut(p.0) else { continue };
        if let Some(s) = set {
            img.image = s.0.clone();
            commands.entity(e).remove::<SetRootPicture>();
        }
        let Some(size) = images.get(&img.image).map(|i| i.size_f32()) else { continue };
        let (sw, sh) = (w.width(), w.height());
        let scale = (sw / size.x).max(sh / size.y);
        let (iw, ih) = (size.x * scale, size.y * scale);
        let want = (Val::Px(((sw - iw) * 0.5).round()), Val::Px(((sh - ih) * 0.5).round()), Val::Px(iw.round()), Val::Px(ih.round()));
        if (node.left, node.top, node.width, node.height) != want {
            (node.left, node.top, node.width, node.height) = want;
            node.position_type = PositionType::Absolute;
            node.right = Val::Auto;
            node.bottom = Val::Auto;
        }
    }
}
