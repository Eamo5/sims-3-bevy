//! The game's own sounds, converted at bake time (`s3bake::sounds`): the cues in animation
//! clips (object sounds, Simlish voices, footsteps), interface clicks, stings for life events,
//! ambience by time of day, and music for the menus, Create-a-Sim and buy mode.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;
use s3bake::{PackReader, SoundBank, SoundDef};
use s3formats::sim::SoundAction;

use crate::buy::BuyMode;
use crate::camera::SimsCamera;
use crate::clock::GameClock;
use crate::nav::Floor;
use crate::sim::{Age, Sim};
use crate::{AppState, PlayMode};

/// `SOUND_LOG=1` logs every sound played.
static LOG: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var_os("SOUND_LOG").is_some());

/// Overall loudness of each kind of sound.
const SFX_VOLUME: f32 = 0.9;
const UI_VOLUME: f32 = 0.6;
const MUSIC_VOLUME: f32 = 0.45;
const AMBIENCE_VOLUME: f32 = 0.35;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySound>()
            .add_message::<ClipCue>()
            .init_resource::<SampleCache>()
            .init_resource::<ModeMusic>()
            .init_resource::<Ambience>()
            .add_systems(Startup, open_sounds)
            .add_systems(OnEnter(AppState::InGame), open_sounds)
            .add_systems(
                Update,
                (
                    clip_cues,
                    play_sounds,
                    loop_volumes,
                    ui_clicks,
                    stings,
                    mode_music,
                    ambience.run_if(in_state(PlayMode::Live)),
                )
                    .chain()
                    .run_if(resource_exists::<Sounds>),
            );
    }
}

/// The baked sound bank and its samples.
#[derive(Resource, Clone)]
pub struct Sounds(Arc<SoundData>);

pub struct SoundData {
    pub bank: SoundBank,
    pack: PackReader,
}

impl Sounds {
    pub fn open() -> Option<Self> {
        let g = s3bake::default_root().global_dir();
        if !s3bake::sounds_ready(&s3bake::default_root()) {
            return None;
        }
        let bank: SoundBank = s3bake::read_value(&g.join("sounds.bin")).ok()?;
        let pack = PackReader::open(&g.join("sounds.pack")).ok()?;
        Some(Self(Arc::new(SoundData { bank, pack })))
    }

    pub fn def(&self, name: &str) -> Option<&SoundDef> {
        self.0.bank.get(name)
    }

    /// The playable file of a sample, cached as an audio asset.
    pub fn sample(&self, id: u64, cache: &mut SampleCache, sources: &mut Assets<AudioSource>) -> Option<Handle<AudioSource>> {
        if let Some(h) = cache.0.get(&id) {
            return Some(h.clone());
        }
        let bytes: Vec<u8> = self.0.pack.get(&(s3formats::audio::T_SNR, 0, id))?;
        let h = sources.add(AudioSource { bytes: bytes.into() });
        cache.0.insert(id, h.clone());
        Some(h)
    }

    /// Duration of a sample in seconds.
    pub fn duration(&self, id: u64) -> f32 {
        self.0.bank.samples.get(&id).map_or(1.0, |s| s.duration)
    }
}

fn open_sounds(mut commands: Commands, existing: Option<Res<Sounds>>) {
    if existing.is_none()
        && let Some(s) = Sounds::open()
    {
        info!("sounds: {} sounds, {} samples", s.0.bank.sounds.len(), s.0.bank.samples.len());
        commands.insert_resource(s);
    }
}

/// Decoded-sample handles, kept so repeated sounds don't re-read the pack.
#[derive(Resource, Default)]
pub struct SampleCache(HashMap<u64, Handle<AudioSource>>);

/// Plays one of the game's sounds by name.
#[derive(Message, Clone, Debug)]
pub struct PlaySound {
    pub name: String,
    /// Where the sound comes from (louder the closer the camera); `None` for interface sounds.
    pub at: Option<Vec3>,
    pub volume: f32,
}

impl PlaySound {
    pub fn ui(name: &str) -> Self {
        Self { name: name.to_string(), at: None, volume: UI_VOLUME }
    }
}

/// A Create a Sim line in a Sim's voice: `stem` (`vo_cas_trait_friendlyA`) for their sex and
/// voice (`_fa`...), when the game has it (children's lines end in C, theirs `_ca`/`_cb`).
pub fn cas_line(sounds: &Sounds, stem: &str, sim: &Sim) -> Option<String> {
    let pitch = ['a', 'b', 'c'][(sim.voice as usize).min(2)];
    let child = matches!(sim.age, Age::Child);
    let who = if child { 'c' } else if sim.female { 'f' } else { 'm' };
    let stem = if child { stem.trim_end_matches('A').to_string() + "C" } else { stem.to_string() };
    [pitch, 'a', 'b', 'c'].into_iter().map(|p| format!("{stem}_{who}{p}")).find(|n| sounds.def(n).is_some())
}

/// A sound cue reached in a Sim's animation clip. An empty name with `StopLoop` stops every
/// loop the Sim started (the animation changed).
#[derive(Message, Clone, Debug)]
pub struct ClipCue {
    pub sim: Entity,
    pub name: String,
    pub action: SoundAction,
}

/// A looping sound started by a Sim's animation.
#[derive(Component)]
struct CueLoop {
    owner: Entity,
    name: String,
    def: SoundDef,
}

/// How loud a sound at `at` is for the camera.
fn spatial_gain(def: &SoundDef, at: Vec3, cam: Option<&SimsCamera>) -> f32 {
    let Some(cam) = cam else { return 1.0 };
    let max = if def.max_distance > 0.0 { def.max_distance } else { 30.0 };
    let min = def.min_distance.clamp(1.0, max * 0.5);
    let d = cam.focus.distance(at) + cam.distance * 0.3;
    if d <= min {
        1.0
    } else {
        let f = ((d - min) / (max * 1.6 - min)).clamp(0.0, 1.0);
        (1.0 - f) * (1.0 - f)
    }
}

/// Picks the concrete sound a clip cue stands for, given who plays it and where.
fn resolve_cue(sounds: &Sounds, name: &str, sim: &Sim, surface: &str) -> Option<String> {
    let has = |n: &str| sounds.def(n).is_some();
    let child = sim.age == Age::Child;
    if name.starts_with("vo_") {
        let pitch = ['a', 'b', 'c'][(sim.voice as usize).min(2)];
        let who = if child { 'c' } else if sim.female { 'f' } else { 'm' };
        for p in [pitch, 'a', 'b', 'c'] {
            let n = format!("{name}_{who}{p}");
            if has(&n) {
                return Some(n);
            }
        }
    }
    if name.starts_with("foot_") {
        let shoe = if child {
            "rub"
        } else if sim.female {
            if sim.look % 2 == 0 { "heel" } else { "rub" }
        } else if sim.look % 2 == 0 {
            "leath"
        } else {
            "rub"
        };
        for n in [format!("{name}_{surface}_{shoe}"), format!("{name}_{surface}_rub"), format!("{name}_wood_rub")] {
            if has(&n) {
                return Some(n);
            }
        }
        return None;
    }
    if name.starts_with("sit_") {
        for m in ["cloth", "leath", "wood"] {
            let n = format!("{name}_{m}");
            if has(&n) {
                return Some(n);
            }
        }
    }
    if has(name) {
        return Some(name.to_string());
    }
    let lp = format!("{name}_lp");
    has(&lp).then_some(lp)
}

/// What a Sim is walking on, for footsteps.
fn surface_at(building: Option<&crate::building::ActiveBuilding>, sidewalk: Option<&crate::town::Sidewalk>, level: u8, p: Vec3) -> &'static str {
    use s3bake::{ROOM_BATH, ROOM_BED, ROOM_KITCHEN, ROOM_OUTSIDE, ROOM_PORCH};
    if let Some(kind) = building.and_then(|b| b.room_at(level, p)) {
        return match kind {
            ROOM_BATH | ROOM_KITCHEN => "lino",
            ROOM_BED => "cpet",
            ROOM_OUTSIDE => "cment",
            ROOM_PORCH => "wood",
            _ => "wood",
        };
    }
    if let Some(s) = sidewalk {
        let d = Vec2::new(p.x, p.z) - s.center;
        let along = d.dot(s.along);
        let across = (d - s.along * along).length();
        if along.abs() <= s.half_length + 2.0 && across < 2.5 {
            return "cment";
        }
    }
    "grass"
}

#[allow(clippy::too_many_arguments)]
fn clip_cues(
    mut commands: Commands,
    mut cues: MessageReader<ClipCue>,
    mut play: MessageWriter<PlaySound>,
    sounds: Res<Sounds>,
    mut cache: ResMut<SampleCache>,
    mut sources: ResMut<Assets<AudioSource>>,
    sims: Query<(&Sim, &GlobalTransform, Option<&Floor>, &Visibility)>,
    loops: Query<(Entity, &CueLoop)>,
    (building, sidewalk, cams): (Option<Res<crate::building::ActiveBuilding>>, Option<Res<crate::town::Sidewalk>>, Query<&SimsCamera>),
) {
    let cam = cams.single().ok();
    for cue in cues.read() {
        if cue.action == SoundAction::StopLoop && cue.name.is_empty() {
            for (e, l) in &loops {
                if l.owner == cue.sim {
                    commands.entity(e).despawn();
                }
            }
            continue;
        }
        let Ok((sim, tf, floor, vis)) = sims.get(cue.sim) else { continue };
        if *vis == Visibility::Hidden {
            continue;
        }
        let p = tf.translation();
        let level = floor.map_or(1, |f| f.0);
        let surface = surface_at(building.as_deref(), sidewalk.as_deref(), level, p);
        let Some(name) = resolve_cue(&sounds, &cue.name, sim, surface) else { continue };
        match cue.action {
            SoundAction::Play => {
                play.write(PlaySound { name, at: Some(p + Vec3::Y), volume: SFX_VOLUME });
            }
            SoundAction::StartLoop => {
                if loops.iter().any(|(_, l)| l.owner == cue.sim && l.name == name) {
                    continue;
                }
                let Some(def) = sounds.def(&name).cloned() else { continue };
                let Some(&id) = def.samples.choose(&mut rand::rng()) else { continue };
                let Some(h) = sounds.sample(id, &mut cache, &mut sources) else { continue };
                let v = def.gain * SFX_VOLUME * spatial_gain(&def, p, cam);
                if *LOG {
                    info!("sound loop: {name} ({id:016X}) gain {v:.2}");
                }
                commands.spawn((
                    AudioPlayer::new(h),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(v)),
                    CueLoop { owner: cue.sim, name, def },
                    DespawnOnExit(AppState::InGame),
                ));
            }
            SoundAction::StopLoop => {
                let stem = name.trim_end_matches("_lp");
                for (e, l) in &loops {
                    if l.owner == cue.sim && l.name.trim_end_matches("_lp") == stem {
                        commands.entity(e).despawn();
                    }
                }
            }
        }
    }
}

fn play_sounds(
    mut commands: Commands,
    settings: Res<crate::options::Settings>,
    mut reqs: MessageReader<PlaySound>,
    sounds: Res<Sounds>,
    mut cache: ResMut<SampleCache>,
    mut sources: ResMut<Assets<AudioSource>>,
    cams: Query<&SimsCamera>,
) {
    let cam = cams.single().ok();
    for r in reqs.read() {
        let Some(def) = sounds.def(&r.name) else { continue };
        let gain = def.gain * r.volume * r.at.map_or(1.0, |p| spatial_gain(def, p, cam)) * settings.gain(crate::options::Channel::of(&r.name));
        if gain < 0.01 {
            continue;
        }
        let Some(&id) = def.samples.choose(&mut rand::rng()) else { continue };
        let Some(h) = sounds.sample(id, &mut cache, &mut sources) else { continue };
        if *LOG {
            info!("sound: {} ({id:016X}) gain {gain:.2}", r.name);
        }
        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain))));
    }
}

/// Loops follow their Sim and the camera; they stop when the Sim is gone.
fn loop_volumes(
    mut commands: Commands,
    settings: Res<crate::options::Settings>,
    mut loops: Query<(Entity, &CueLoop, Option<&mut AudioSink>)>,
    owners: Query<&GlobalTransform>,
    cams: Query<&SimsCamera>,
) {
    let cam = cams.single().ok();
    for (e, l, sink) in &mut loops {
        let Ok(tf) = owners.get(l.owner) else {
            commands.entity(e).despawn();
            continue;
        };
        if let Some(mut sink) = sink {
            let level = settings.gain(crate::options::Channel::of(&l.name));
            sink.set_volume(Volume::Linear(l.def.gain * SFX_VOLUME * spatial_gain(&l.def, tf.translation(), cam) * level));
        }
    }
}

/// Interface sounds: button presses, pie menus, game speed, notifications, buy mode.
#[allow(clippy::too_many_arguments)]
fn ui_clicks(
    mut play: MessageWriter<PlaySound>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
    pie: Option<Res<crate::hud::PieMenu>>,
    clock: Option<Res<GameClock>>,
    notes: Option<Res<crate::interact::Notifications>>,
    buy: Option<Res<BuyMode>>,
    mut last: Local<(usize, usize, bool, bool, Option<Entity>)>,
) {
    if buttons.iter().any(|i| *i == Interaction::Pressed) {
        play.write(PlaySound::ui("ui_primary_button"));
    }
    let pie_root = pie.and_then(|p| p.root);
    if pie_root.is_some() && pie_root != last.4 {
        play.write(PlaySound::ui("ui_piemenu_primary"));
    }
    last.4 = pie_root;
    if let Some(clock) = clock
        && clock.speed != last.0
    {
        if last.2 {
            match clock.speed {
                2 => {
                    play.write(PlaySound::ui("ui_button_fw"));
                }
                3 => {
                    play.write(PlaySound::ui("ui_button_ffw"));
                }
                _ => {}
            }
        }
        last.0 = clock.speed;
        last.2 = true;
    }
    if let Some(notes) = notes {
        if notes.0.len() > last.1 {
            play.write(PlaySound::ui("ui_text_notification_open"));
        }
        last.1 = notes.0.len();
    }
    if let Some(buy) = buy
        && buy.active != last.3
    {
        play.write(PlaySound::ui(if buy.active { "ui_hud_panel_open" } else { "ui_hud_panel_close" }));
        last.3 = buy.active;
    }
}

/// Stings for life's big moments.
fn stings(
    mut play: MessageWriter<PlaySound>,
    mut events: MessageReader<crate::life::LifeEvent>,
    household: Query<(), With<crate::sim::HouseholdMember>>,
    time: Res<Time>,
    mut last: Local<(Option<&'static str>, f32)>,
) {
    use crate::life::LifeEventKind as K;
    for ev in events.read() {
        if household.get(ev.sim).is_err() {
            continue;
        }
        let name = match &ev.kind {
            K::Promoted => "sting_career_positive",
            K::Demoted => "sting_career_grade_lo",
            K::Fired => "sting_career_fail",
            K::NewJob => "sting_new_profession",
            K::SkillUp { .. } => "sting_school_level_up",
            K::FirstKiss => "sting_sm_good_event",
            K::StartedDating => "sting_good_event",
            K::Engaged => "sting_propose",
            K::Married => "sting_wedding_cake",
            K::BrokeUp => "sting_sm_bad_event",
            K::MovedIn => "sting_good_event",
            K::Bought { .. } => "ui_object_plop",
            _ => continue,
        };
        // The same moment for several Sims (moving in together) plays once.
        let now = time.elapsed_secs();
        if last.0 == Some(name) && now - last.1 < 2.0 {
            continue;
        }
        *last = (Some(name), now);
        play.write(PlaySound::ui(name).with_volume(MUSIC_VOLUME * 1.6));
    }
}

impl PlaySound {
    pub fn with_volume(mut self, v: f32) -> Self {
        self.volume = v;
        self
    }
}

/// Background music for the menus, Create-a-Sim and buy mode.
#[derive(Resource, Default)]
struct ModeMusic {
    playlist: Option<&'static str>,
    playing: Option<Entity>,
}

#[derive(Component)]
struct ModeTrack;

#[allow(clippy::too_many_arguments)]
fn mode_music(
    mut commands: Commands,
    mut music: ResMut<ModeMusic>,
    sounds: Res<Sounds>,
    mut cache: ResMut<SampleCache>,
    mut sources: ResMut<Assets<AudioSource>>,
    state: Res<State<AppState>>,
    mode: Option<Res<State<PlayMode>>>,
    buy: Option<Res<BuyMode>>,
    (tracks, mut sinks, settings): (Query<(), With<ModeTrack>>, Query<&mut AudioSink, With<ModeTrack>>, Res<crate::options::Settings>),
) {
    // The music level follows the options.
    if settings.is_changed() {
        for mut s in &mut sinks {
            s.set_volume(Volume::Linear(MUSIC_VOLUME * settings.gain(crate::options::Channel::Music)));
        }
    }
    let wanted = match state.get() {
        AppState::MainMenu => Some("music_theme"),
        AppState::CreateHousehold => Some("music_mode_cas"),
        AppState::Loading => Some("music_load"),
        AppState::InGame => match mode.as_deref().map(|m| *m.get()) {
            Some(PlayMode::ChooseLot) => Some("music_mode_map"),
            _ if buy.is_some_and(|b| b.active) => Some("music_mode_buy"),
            _ => None,
        },
    };
    let finished = music.playing.is_some_and(|e| tracks.get(e).is_err());
    if wanted != music.playlist || finished {
        if let Some(e) = music.playing.take()
            && wanted != music.playlist
        {
            commands.entity(e).try_despawn();
        }
        music.playlist = wanted;
        let Some(list) = wanted else { return };
        let Some(def) = sounds.def(list) else { return };
        let Some(&id) = def.samples.choose(&mut rand::rng()) else { return };
        let Some(h) = sounds.sample(id, &mut cache, &mut sources) else { return };
        if *LOG {
            info!("music: {list} ({id:016X})");
        }
        let e = commands
            .spawn((
                AudioPlayer::new(h),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(MUSIC_VOLUME * def.gain.max(0.5) * settings.gain(crate::options::Channel::Music))),
                ModeTrack,
            ))
            .id();
        music.playing = Some(e);
    }
}

/// Birds by day, insects and owls at night: occasional one-shots near the camera.
#[derive(Resource, Default)]
struct Ambience {
    next: f32,
}

fn ambience(
    time: Res<Time>,
    clock: Res<GameClock>,
    mut amb: ResMut<Ambience>,
    mut play: MessageWriter<PlaySound>,
    sounds: Res<Sounds>,
    cams: Query<&SimsCamera>,
    weather: Option<Res<crate::weather::Weather>>,
) {
    amb.next -= time.delta_secs();
    if amb.next > 0.0 {
        return;
    }
    let mut rng = rand::rng();
    amb.next = rng.random_range(4.0..10.0);
    let Ok(cam) = cams.single() else { return };
    // Quieter when zoomed far out.
    let zoom = (1.0 - (cam.distance - 20.0) / 120.0).clamp(0.2, 1.0);
    let h = clock.hour_f();
    // (The birds keep quiet in rain and snow.)
    if weather.as_ref().is_some_and(|w| w.falling(clock.minutes) > 0.2) {
        return;
    }
    use crate::weather::Season as S;
    let choices: &[&str] = match (weather.as_ref().map_or(S::Summer, |w| w.season(clock.day())), h) {
        (S::Fall, h) if (6.0..9.0).contains(&h) => &["amb_fall_birds_morn", "amb_birds_allday"],
        (S::Fall, h) if (9.0..17.0).contains(&h) => &["amb_fall_birds_midday", "amb_dogs_day"],
        (S::Fall, h) if (17.0..20.5).contains(&h) => &["amb_fall_birds_dusk"],
        (S::Fall, _) => &["amb_fall_owl_night", "amb_insects_nite", "amb_dogs_nite"],
        (S::Winter, h) if (6.0..9.0).contains(&h) => &["amb_winter_birds_morn"],
        (S::Winter, h) if (9.0..17.0).contains(&h) => &["amb_winter_birds_midday", "amb_dogs_day"],
        (S::Winter, h) if (17.0..20.5).contains(&h) => &["amb_winter_birds_dusk"],
        (S::Winter, _) => &["amb_winter_owl_night", "amb_dogs_nite"],
        (S::Spring, h) if (6.0..9.0).contains(&h) => &["amb_spring_birds_morn", "amb_world_bird_day"],
        (S::Spring, h) if (9.0..17.0).contains(&h) => &["amb_spring_birds_midday", "amb_birds_allday", "amb_dogs_day"],
        (S::Spring, h) if (17.0..20.5).contains(&h) => &["amb_spring_birds_dusk", "amb_birds_allday"],
        (S::Spring, _) => &["amb_spring_insects_night", "amb_owl_nite", "amb_dogs_nite"],
        (_, h) if (6.0..9.0).contains(&h) => &["amb_birds_allday", "amb_world_bird_day", "amb_summer_birds_morn"],
        (_, h) if (9.0..17.0).contains(&h) => &["amb_birds_midday", "amb_birds_allday", "amb_dogs_day", "amb_summer_birds_midday"],
        (_, h) if (17.0..20.5).contains(&h) => &["amb_birds_dusk", "amb_birds_allday", "amb_summer_birds_dusk"],
        _ => &["amb_insects_nite", "amb_insects_nite", "amb_owl_nite", "amb_dogs_nite", "amb_summer_insects_night"],
    };
    let Some(name) = choices.iter().copied().filter(|n| sounds.def(n).is_some()).collect::<Vec<_>>().choose(&mut rng).copied() else { return };
    if let Some(&id) = sounds.def(name).and_then(|d| d.samples.first()) {
        amb.next = amb.next.max(sounds.duration(id) * 0.8);
    }
    play.write(PlaySound::ui(name).with_volume(AMBIENCE_VOLUME * zoom));
}
