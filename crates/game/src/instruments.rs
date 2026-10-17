//! The instruments' music, as the game's `BandInstrument` plays it: practised, a loop by the
//! player's skill level (`GetLoopForSkillLevel`: `guitar_playa_group` at none to
//! `guitar_playk_group` at ten), changed as they level up; "Perform..." one of the compositions
//! they know (`Perform`: learned three a level from six, `BandSkill.SkillLeveledUp`, and at ten a
//! Virtuoso's master track, `GrantUnknownMasterTrack`), played through once (`SongComplete`) for
//! more of the skill than practising.

use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::*;
use rand::seq::IndexedRandom;
use s3bake::GameDataBaked;

use crate::PlayMode;
use crate::camera::SimsCamera;
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills, Special, interactions_for};
use crate::life::Trait;
use crate::sim::Sim;

pub struct InstrumentsPlugin;

impl Plugin for InstrumentsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, instrument_music.after(crate::interact::run_actions).run_if(in_state(PlayMode::Live)));
    }
}

/// How the game names performing (`Guitar/Perform:InteractionName`, `:InteractionQueue`).
pub const PERFORM: &str = "Perform";
pub const PERFORMING: &str = "Performing";

/// An instrument's music (`GetLoopForSkillLevelPrefix`).
pub fn prefix(kind: ObjectKind) -> Option<&'static str> {
    match kind {
        ObjectKind::Guitar => Some("guitar"),
        ObjectKind::Piano | ObjectKind::Keyboard => Some("piano"),
        ObjectKind::Drums => Some("drums"),
        ObjectKind::LaserHarp => Some("theremin"),
        _ => None,
    }
}

/// The level a composition's learned at (its music's letter after `_play`: `g` six, `k` ten);
/// none for a master track.
fn learned_at(clip: &str) -> Option<u32> {
    let l = clip.split("_play").nth(1)?.chars().next()?;
    ('g'..='k').contains(&l).then(|| l as u32 - 'g' as u32 + 6)
}

/// The compositions a Sim knows (indices into the game data's `compositions`): those of their
/// level and below, and at ten a Virtuoso's master track.
pub fn known(data: &GameDataBaked, skill: &str, level: u32, traits: &[Trait], sim_id: u64) -> Vec<usize> {
    let mine = |c: &&s3bake::gamedata::CompositionInfo| c.skill == skill;
    let mut out: Vec<usize> = (0..data.compositions.len())
        .filter(|&i| {
            let c = &data.compositions[i];
            mine(&c) && !c.master && learned_at(&c.clip).is_some_and(|at| at <= level)
        })
        .collect();
    if level >= 10 && traits.contains(&Trait::Virtuoso) {
        let masters: Vec<usize> = (0..data.compositions.len()).filter(|&i| mine(&&data.compositions[i]) && data.compositions[i].master).collect();
        // (Which one, theirs for good.)
        if !masters.is_empty() {
            out.push(masters[(sim_id % masters.len() as u64) as usize]);
        }
    }
    out
}

/// Percent of an instrument's compositions known (its journal's "Compositions Known": master
/// tracks known counted over and above the rest, to the hundredth).
pub fn known_percent(data: &GameDataBaked, skill: &str, level: u32, traits: &[Trait], sim_id: u64) -> f64 {
    let all = data.compositions.iter().filter(|c| c.skill == skill && !c.master).count();
    let known = known(data, skill, level, traits, sim_id);
    let masters = known.iter().filter(|&&i| data.compositions[i].master).count();
    if all + masters == 0 {
        return 0.0;
    }
    (known.len() as f64 / (all + masters) as f64 * 100.0).round()
}

/// The music played for someone at an instrument: what, and how it's going.
#[derive(Component)]
pub struct InstrumentMusic {
    instrument: Entity,
    sound: String,
    audio: Option<Entity>,
    /// A composition performed: when it ends (game minutes).
    ends: Option<f64>,
}

/// The audio of an instrument's music, sounding from it.
#[derive(Component)]
struct MusicAudio(Entity);

/// What a Sim's playing: the instrument, and whether performing (the song's index).
fn playing(queue: &ActionQueue, objects: &Query<(&GameObject, &GlobalTransform)>) -> Option<(Entity, &'static str, &'static str, Option<Option<u16>>)> {
    let a = queue.0.front()?;
    let ActionKind::Object { target, def } = a.kind else { return None };
    if a.cancel || !matches!(a.phase, Phase::Running(_)) {
        return None;
    }
    let (o, _) = objects.get(target).ok()?;
    let d = interactions_for(o.kind).get(def)?;
    let skill = d.skill?;
    let prefix = prefix(o.kind)?;
    match d.special {
        Special::Perform => Some((target, prefix, skill, Some(a.song_choice))),
        Special::None | Special::PlayForTips => Some((target, prefix, skill, None)),
        _ => None,
    }
}

/// The instruments' music: started as someone plays, the next of the level's loops when one's
/// done, a new one when they level up, and the song performed till it's over.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn instrument_music(
    mut commands: Commands,
    clock: Res<crate::clock::GameClock>,
    sounds: Option<Res<crate::sound::Sounds>>,
    mut cache: ResMut<crate::sound::SampleCache>,
    mut sources: ResMut<Assets<AudioSource>>,
    settings: Res<crate::options::Settings>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut players: Query<(Entity, &Sim, &mut ActionQueue, &Skills, Option<&mut InstrumentMusic>)>,
    objects: Query<(&GameObject, &GlobalTransform)>,
    mut audio: Query<(&MusicAudio, Option<&mut AudioSink>)>,
    cams: Query<&SimsCamera>,
    mut did: MessageWriter<crate::journal::Did>,
) {
    let cam = cams.single().ok();
    let level_gain = settings.gain(crate::options::Channel::Effects);
    let mut rng = rand::rng();
    for (me, sim, mut queue, skills, music) in &mut players {
        let now_playing = playing(&queue, &objects);
        let Some((instrument, prefix, skill, perform)) = now_playing else {
            if let Some(m) = music {
                if let Some(a) = m.audio {
                    commands.entity(a).try_despawn();
                }
                commands.entity(me).remove::<InstrumentMusic>();
            }
            continue;
        };
        // What's to be heard: the song performed (chosen, else one they know), or the loop for
        // their level.
        let level = skills.level(skill).min(10);
        let sound = match perform {
            Some(choice) => {
                let Some(data) = ui.as_ref().map(|u| &u.data) else { continue };
                let song = choice.map(|s| s as usize).or_else(|| known(data, skill, level, &sim.traits, sim.id).choose(&mut rng).copied());
                let Some(song) = song.filter(|&s| s < data.compositions.len()) else {
                    // (Nothing known to perform.)
                    if let Some(a) = queue.0.front_mut() {
                        a.completed = true;
                    }
                    continue;
                };
                if let Some(a) = queue.0.front_mut() {
                    a.song_choice = Some(song as u16);
                }
                data.compositions[song].clip.clone()
            }
            None => format!("{prefix}_play{}_group", (b'a' + level as u8) as char),
        };
        let at = objects.get(instrument).map(|o| o.1.translation()).unwrap_or_default();
        let def = sounds.as_ref().and_then(|s| s.def(&sound));
        let gain = def.map_or(0.0, |d| d.gain * crate::sound::SFX_VOLUME * crate::sound::spatial_gain(d, at, cam) * level_gain);
        match music {
            Some(mut m) if m.instrument == instrument && m.sound == sound => {
                // (The song performed: over when it's played through.)
                if let Some(ends) = m.ends {
                    if clock.minutes >= ends {
                        if let Some(a) = queue.0.front_mut() {
                            a.completed = true;
                        }
                        did.write(crate::journal::Did::count(me, crate::journal::Stat::CompositionsPerformed, 1.0));
                        m.ends = None;
                    }
                    if let Some(Ok((_, Some(mut sink)))) = m.audio.map(|a| audio.get_mut(a)) {
                        sink.set_volume(Volume::Linear(gain));
                    }
                    continue;
                }
                match m.audio.map(|a| audio.get_mut(a)) {
                    Some(Ok((_, Some(mut sink)))) => sink.set_volume(Volume::Linear(gain)),
                    Some(Ok(_)) => {}
                    // (One of the level's loops over: another.)
                    Some(Err(_)) | None if perform.is_none() => {
                        debug!("{} plays on: {sound}", sim.first);
                        m.audio = start(&mut commands, sounds.as_deref(), &sound, &mut cache, &mut sources, gain, me);
                    }
                    _ => {}
                }
            }
            old => {
                if let Some(a) = old.as_ref().and_then(|m| m.audio) {
                    commands.entity(a).try_despawn();
                }
                let audio_e = start(&mut commands, sounds.as_deref(), &sound, &mut cache, &mut sources, gain, me);
                // (A song's length: its music's, a minute of the game's to a second of it.)
                let ends = perform.is_some().then(|| {
                    let secs = def.and_then(|d| d.samples.first()).zip(sounds.as_ref()).map_or(45.0, |(id, s)| s.duration(*id) as f64);
                    clock.minutes + secs
                });
                info!("{} {} {sound}", sim.first, if perform.is_some() { "performs" } else { "plays" });
                commands.entity(me).insert(InstrumentMusic { instrument, sound, audio: audio_e, ends });
            }
        }
    }
}

/// One of the sound's variants, playing once.
fn start(
    commands: &mut Commands,
    sounds: Option<&crate::sound::Sounds>,
    name: &str,
    cache: &mut crate::sound::SampleCache,
    sources: &mut Assets<AudioSource>,
    gain: f32,
    player: Entity,
) -> Option<Entity> {
    let s = sounds?;
    let def = s.def(name)?;
    let id = *def.samples.choose(&mut rand::rng())?;
    let h = s.sample(id, cache, sources)?;
    Some(
        commands
            .spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)), MusicAudio(player), DespawnOnExit(crate::AppState::InGame)))
            .id(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameDataBaked {
        let c = |clip: &str, master: bool| s3bake::gamedata::CompositionInfo { skill: "Guitar".into(), name: clip.into(), clip: clip.into(), master };
        GameDataBaked {
            compositions: vec![c("guitar_playga", false), c("guitar_playgb", false), c("guitar_playha", false), c("guitar_playka", false), c("guitar_mastera", true), c("guitar_masterb", true)],
            ..Default::default()
        }
    }

    #[test]
    fn compositions_learned_by_level_and_a_virtuosos_master_track() {
        let d = data();
        assert!(known(&d, "Guitar", 5, &[], 1).is_empty());
        assert_eq!(known(&d, "Guitar", 6, &[], 1), vec![0, 1]);
        assert_eq!(known(&d, "Guitar", 7, &[], 1), vec![0, 1, 2]);
        assert_eq!(known(&d, "Guitar", 10, &[], 1), vec![0, 1, 2, 3]);
        assert_eq!(known(&d, "Guitar", 10, &[Trait::Virtuoso], 1), vec![0, 1, 2, 3, 5]);
        assert!(known(&d, "Piano", 10, &[], 1).is_empty());
        assert_eq!(known_percent(&d, "Guitar", 6, &[], 1), 50.0);
        assert_eq!(known_percent(&d, "Guitar", 10, &[Trait::Virtuoso], 1), 100.0);
    }
}
