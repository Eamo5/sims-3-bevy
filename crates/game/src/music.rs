//! Music from the game's custom-music radio station (MP3s copied into the cache): a stereo in
//! use plays a track, louder the closer the camera is.

use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::camera::SimsCamera;
use crate::interact::{GameObject, ObjectKind, UsedBy};
use crate::{AppState, PlayMode};

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicLibrary>()
            .add_systems(OnEnter(AppState::InGame), load_music)
            .add_systems(Update, (stereo_music, stereo_volume).chain().run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Resource, Default)]
struct MusicLibrary(Vec<Handle<AudioSource>>);

fn load_music(mut lib: ResMut<MusicLibrary>, mut sources: ResMut<Assets<AudioSource>>) {
    if !lib.0.is_empty() {
        return;
    }
    let dir = s3bake::default_root().dir.join("music");
    let mut files: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    files.sort();
    for f in files {
        if let Ok(bytes) = std::fs::read(&f) {
            lib.0.push(sources.add(AudioSource { bytes: bytes.into() }));
        }
    }
    info!("music: {} tracks", lib.0.len());
}

/// Music playing for a stereo.
#[derive(Component)]
struct StereoMusic(Entity);

fn stereo_music(
    mut commands: Commands,
    lib: Res<MusicLibrary>,
    stereos: Query<(Entity, &GameObject, &UsedBy, &GlobalTransform)>,
    playing: Query<(Entity, &StereoMusic)>,
    cams: Query<&SimsCamera>,
) {
    if lib.0.is_empty() {
        return;
    }
    let cam = cams.single().ok();
    // Stop music for stereos nobody is using any more.
    for (e, m) in &playing {
        if !stereos.get(m.0).is_ok_and(|(_, _, used, _)| used.0.is_some()) {
            commands.entity(e).despawn();
        }
    }
    for (stereo, obj, used, tf) in &stereos {
        if !matches!(obj.kind, ObjectKind::Stereo) || used.0.is_none() || playing.iter().any(|(_, m)| m.0 == stereo) {
            continue;
        }
        let volume = cam.map_or(0.5, |c| {
            let d = c.focus.distance(tf.translation()) + c.distance * 0.35;
            (1.2 / (1.0 + d / 12.0)).min(0.9)
        });
        let track = lib.0.choose(&mut rand::rng()).unwrap().clone();
        info!("music: the stereo starts playing (volume {volume:.2})");
        commands.spawn((
            AudioPlayer::new(track),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
            StereoMusic(stereo),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Keeps stereo volume following the camera.
pub fn stereo_volume(
    mut sinks: Query<(&StereoMusic, &mut AudioSink)>,
    stereos: Query<&GlobalTransform>,
    cams: Query<&SimsCamera>,
) {
    let Ok(cam) = cams.single() else { return };
    for (m, mut sink) in &mut sinks {
        if let Ok(tf) = stereos.get(m.0) {
            let d = cam.focus.distance(tf.translation()) + cam.distance * 0.35;
            sink.set_volume(Volume::Linear((1.2 / (1.0 + d / 12.0)).min(0.9)));
        }
    }
}
