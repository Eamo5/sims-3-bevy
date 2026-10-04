//! Stereos: a stereo in use plays a song from one of the game's radio stations (or the
//! player's custom-music folder), louder the closer the camera is.

use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::*;
use rand::Rng;
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

#[allow(clippy::too_many_arguments)]
fn stereo_music(
    mut commands: Commands,
    lib: Res<MusicLibrary>,
    stereos: Query<(Entity, &GameObject, &UsedBy, &GlobalTransform)>,
    playing: Query<(Entity, &StereoMusic)>,
    cams: Query<&SimsCamera>,
    sounds: Option<Res<crate::sound::Sounds>>,
    mut cache: ResMut<crate::sound::SampleCache>,
    mut sources: ResMut<Assets<AudioSource>>,
) {
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
        // A song from a random radio station; the custom-music folder counts as one more.
        let mut rng = rand::rng();
        let stations: Vec<&str> = sounds.as_ref().map_or(vec![], |s| s3bake::sounds::STATIONS.iter().copied().filter(|n| s.def(n).is_some()).collect());
        let custom = !lib.0.is_empty() && rng.random_range(0..=stations.len()) == 0;
        let track = match (custom, stations.choose(&mut rng), sounds.as_ref()) {
            (false, Some(station), Some(s)) => {
                let def = s.def(station).unwrap();
                let id = *def.samples.choose(&mut rng).unwrap();
                info!("music: the stereo plays {station}");
                s.sample(id, &mut cache, &mut sources)
            }
            _ => lib.0.choose(&mut rng).cloned(),
        };
        let Some(track) = track else { continue };
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
