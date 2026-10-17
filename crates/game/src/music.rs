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
            .add_systems(Update, (workout_station, stereo_music, stereo_volume, favorite_music).chain().run_if(in_state(PlayMode::Live)));
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

/// Music playing for a stereo, and its kind (a favourite music: `classical`, `custom`).
#[derive(Component)]
struct StereoMusic(Entity, String);

/// A song played: a station's (its sample), or one of the custom-music folder's.
#[derive(Clone, PartialEq, Debug)]
enum Track {
    Station(String, u64),
    Custom(usize),
}

/// The songs a stereo's played, the last few (for "Skip... Backward").
#[derive(Component, Default)]
struct Played(Vec<Track>);

/// A stereo's volume, as the game's `ChangeVolume` sets it (its `SoundLevel`: Medium till it's
/// changed), and each level's gain (`kSoundGainLowLevel`...).
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct StereoVolume(pub usize);
pub const VOLUMES: [(&str, f32); 3] = [("Low", 0.25), ("Medium", 0.6), ("High", 1.0)];
const MEDIUM: usize = 1;

/// How the game names them (`Stereo:ChangeVolume`, `:Skip`, `:Next`, `:Back`).
pub const CHANGE_VOLUME: &str = "Change Volume...";
pub const SKIP: &str = "Skip...";
pub const FORWARD: &str = "Forward";
pub const BACKWARD: &str = "Backward";

/// A song skipped, on to the next (`true`) or back to the last (the game's `SkipSong`).
#[derive(Component)]
pub struct SkipSong(pub bool);

/// How loud a stereo's turned, against the Medium its music's heard at otherwise.
fn turned(v: Option<&StereoVolume>) -> f32 {
    VOLUMES[v.map_or(MEDIUM, |v| v.0.min(2))].1 / VOLUMES[MEDIUM].1
}

/// A stereo turned on: it plays until it's turned off, on a station chosen (`stereo_pop`) or
/// one of its own choosing.
#[derive(Component, Clone, PartialEq)]
pub struct StereoOn(pub Option<String>);

/// The game's workout station (its `Stereo` table's `IsWorkoutStation`).
const WORKOUT_STATION: &str = "stereo_electronica";

/// Working out to a stereo: it's put on its workout station, as the game's `SwitchToWorkOutChannel`.
fn workout_station(mut commands: Commands, sims: Query<&crate::interact::ActionQueue>, objects: Query<(&crate::interact::GameObject, Option<&StereoOn>)>) {
    for q in &sims {
        let Some(a) = q.current() else { continue };
        let crate::interact::ActionKind::Object { target, def } = a.kind else { continue };
        if !matches!(a.phase, crate::interact::Phase::Running(_)) {
            continue;
        }
        let Ok((o, on)) = objects.get(target) else { continue };
        if crate::interact::interactions_for(o.kind).get(def).is_some_and(|d| matches!(d.special, crate::interact::Special::CardioWorkOut | crate::interact::Special::StrengthWorkOut))
            && on.is_none_or(|s| s.0.as_deref() != Some(WORKOUT_STATION))
        {
            commands.entity(target).insert(StereoOn(Some(WORKOUT_STATION.to_string())));
        }
    }
}

/// The stations a stereo can be changed to: the game's that are installed, with their names.
pub fn stations(sounds: Option<&crate::sound::Sounds>) -> Vec<(&'static str, String)> {
    s3bake::sounds::STATIONS
        .iter()
        .copied()
        .filter(|n| sounds.is_some_and(|s| s.def(n).is_some()))
        .map(|n| (n, station_name(n)))
        .collect()
}

/// A station's name for show: `stereo_geekrock` "Geek Rock".
pub fn station_name(key: &str) -> String {
    match key.trim_start_matches("stereo_") {
        "hiphop" => "Hip Hop".into(),
        "rnb" => "R&B".into(),
        "geekrock" => "Geek Rock".into(),
        "songwriter" => "Singer-Songwriter".into(),
        "islandlife" => "Island Life".into(),
        "beachparty" => "Beach Party".into(),
        "darkwave" => "Darkwave".into(),
        k => {
            let mut c = k.chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stereo_music(
    mut commands: Commands,
    lib: Res<MusicLibrary>,
    mut stereos: Query<(Entity, &GameObject, &UsedBy, &GlobalTransform, Option<&StereoOn>, (Option<&StereoVolume>, Option<&SkipSong>, Option<&mut Played>))>,
    playing: Query<(Entity, &StereoMusic)>,
    cams: Query<&SimsCamera>,
    sounds: Option<Res<crate::sound::Sounds>>,
    mut cache: ResMut<crate::sound::SampleCache>,
    mut sources: ResMut<Assets<AudioSource>>,
    settings: Res<crate::options::Settings>,
) {
    let cam = cams.single().ok();
    let level = settings.gain(crate::options::Channel::Music);
    // Stop music for stereos turned off (and nobody dancing to), changed to another station, or
    // skipped on from.
    let mut stopped = Vec::new();
    for (e, m) in &playing {
        let keep = stereos.get(m.0).is_ok_and(|(_, _, used, _, on, (_, skip, _))| {
            skip.is_none()
                && match on {
                    Some(StereoOn(Some(station))) => station.trim_start_matches("stereo_") == m.1,
                    Some(_) => true,
                    None => used.0.is_some(),
                }
        });
        if !keep {
            commands.entity(e).try_despawn();
            stopped.push(m.0);
        }
    }
    for (stereo, obj, used, tf, on, (vol, skip, mut played)) in &mut stereos {
        if !matches!(obj.kind, ObjectKind::Stereo) || (used.0.is_none() && on.is_none()) || (playing.iter().any(|(_, m)| m.0 == stereo) && !stopped.contains(&stereo)) {
            continue;
        }
        // (Back to the song before, skipped back to.)
        let back = skip.is_some_and(|s| !s.0);
        commands.entity(stereo).remove::<SkipSong>();
        // (Danced to, it's on: it plays on after, as the game's does.)
        if on.is_none() {
            commands.entity(stereo).insert(StereoOn(None));
        }
        let volume = cam.map_or(0.5, |c| {
            let d = c.focus.distance(tf.translation()) + c.distance * 0.35;
            (1.2 / (1.0 + d / 12.0)).min(0.9)
        }) * turned(vol);
        // A song from a random radio station; the custom-music folder counts as one more.
        let mut rng = rand::rng();
        let mut stations: Vec<&str> = sounds.as_ref().map_or(vec![], |s| s3bake::sounds::STATIONS.iter().copied().filter(|n| s.def(n).is_some()).collect());
        // (The station it's on, if one was chosen.)
        if let Some(StereoOn(Some(chosen))) = on {
            stations.retain(|n| *n == chosen.as_str());
        }
        let custom = !lib.0.is_empty() && on.is_none_or(|o| o.0.is_none()) && rng.random_range(0..=stations.len()) == 0;
        // (The song before, skipped back to; else one of the station's.)
        let history = played.as_deref().map_or(&[][..], |p| &p.0[..]);
        let before = back.then(|| history.len().checked_sub(2).map(|i| history[i].clone())).flatten();
        let went_back = before.is_some();
        let pick = match before {
            Some(t) => Some(t),
            None => match (custom, stations.choose(&mut rng), sounds.as_ref()) {
                (false, Some(station), Some(s)) => s.def(station).and_then(|d| d.samples.choose(&mut rng)).map(|id| Track::Station(station.to_string(), *id)),
                _ => (!lib.0.is_empty()).then(|| Track::Custom(rng.random_range(0..lib.0.len()))),
            },
        };
        let Some(pick) = pick else { continue };
        let (genre, track) = match &pick {
            Track::Station(station, id) => {
                info!("music: the stereo plays {station}");
                (station.trim_start_matches("stereo_").to_string(), sounds.as_ref().and_then(|s| s.sample(*id, &mut cache, &mut sources)))
            }
            Track::Custom(i) => ("custom".to_string(), lib.0.get(*i).cloned()),
        };
        let Some(track) = track else { continue };
        // (Kept in the stereo's history: the one gone back from let go.)
        match played.as_deref_mut() {
            Some(p) => {
                if went_back {
                    p.0.pop();
                } else {
                    p.0.push(pick.clone());
                    let over = p.0.len().saturating_sub(10);
                    p.0.drain(..over);
                }
            }
            None => {
                commands.entity(stereo).insert(Played(vec![pick.clone()]));
            }
        }
        commands.spawn((
            AudioPlayer::new(track),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume * level)),
            StereoMusic(stereo, genre),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Sims near a stereo playing their favourite music enjoy it.
fn favorite_music(
    clock: Res<crate::clock::GameClock>,
    playing: Query<&StereoMusic>,
    stereos: Query<&GlobalTransform>,
    mut sims: Query<(&crate::sim::Sim, &GlobalTransform, &mut crate::life::Moodlets)>,
    mut last: Local<f64>,
) {
    if clock.minutes - *last < 10.0 {
        return;
    }
    *last = clock.minutes;
    for m in &playing {
        let Ok(at) = stereos.get(m.0) else { continue };
        for (sim, tf, mut moods) in &mut sims {
            if sim.favorites.music == m.1 && tf.translation().distance(at.translation()) < 8.0 && !moods.has(crate::life::MoodletKind::EnjoyingMusic) {
                moods.add(crate::life::MoodletKind::EnjoyingMusic, clock.minutes);
            }
        }
    }
}

/// Keeps stereo volume following the camera.
pub fn stereo_volume(
    mut sinks: Query<(&StereoMusic, &mut AudioSink)>,
    stereos: Query<(&GlobalTransform, Option<&StereoVolume>)>,
    cams: Query<&SimsCamera>,
    settings: Res<crate::options::Settings>,
) {
    let Ok(cam) = cams.single() else { return };
    let level = settings.gain(crate::options::Channel::Music);
    for (m, mut sink) in &mut sinks {
        if let Ok((tf, vol)) = stereos.get(m.0) {
            let d = cam.focus.distance(tf.translation()) + cam.distance * 0.35;
            sink.set_volume(Volume::Linear((1.2 / (1.0 + d / 12.0)).min(0.9) * turned(vol) * level));
        }
    }
}
