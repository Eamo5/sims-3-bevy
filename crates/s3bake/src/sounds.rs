//! The game's sounds: the clips' sound cues (object sounds, Simlish voices, footsteps), UI
//! clicks, stings, ambience, mode music and the radio stations. Samples are converted once:
//! EALayer3 is rewrapped losslessly as MP3 and EA-XAS decoded to 16-bit WAV, both formats the
//! game's audio decoder plays directly. `sounds.bin` maps each sound name to its variants.

use std::collections::{HashMap, HashSet};

use s3formats::audio::{self, SoundProps};
use s3pkg::PackageSet;
use serde::{Deserialize, Serialize};

use crate::bake::{BakeRoot, Progress, par_map};
use crate::pack::{PackReader, PackWriter, write_value};

pub const SOUNDS_VERSION: u32 = 3;

/// Sample file formats in `sounds.pack`.
pub const FORMAT_MP3: u8 = 0;
pub const FORMAT_WAV: u8 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SoundDef {
    /// Variants (sample instance ids in `sounds.pack`); one is picked at random.
    pub samples: Vec<u64>,
    /// Linear gain.
    pub gain: f32,
    /// Full volume within `min_distance`, silent beyond `max_distance` (metres; 0 = not 3D).
    pub min_distance: f32,
    pub max_distance: f32,
    pub looping: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SampleInfo {
    pub format: u8,
    pub channels: u8,
    pub sample_rate: u32,
    pub duration: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SoundBank {
    pub version: u32,
    /// By FNV-64 of the lowercased sound name.
    pub sounds: HashMap<u64, SoundDef>,
    pub samples: HashMap<u64, SampleInfo>,
}

impl SoundBank {
    pub fn get(&self, name: &str) -> Option<&SoundDef> {
        self.sounds.get(&s3pkg::fnv64(name)).filter(|d| !d.samples.is_empty())
    }
}

/// Adult voices (female/male, three pitches) and children's, appended to `vo_` cues.
pub const ADULT_VOICES: [&str; 6] = ["fa", "fb", "fc", "ma", "mb", "mc"];
pub const CHILD_VOICES: [&str; 3] = ["ca", "cb", "cc"];
/// Footstep surfaces and shoes (`foot_step_<surface>_<shoe>`).
pub const SURFACES: [&str; 8] = ["wood", "cpet", "lino", "marble", "cment", "grass", "gravel", "dirt"];
pub const SHOES: [&str; 5] = ["rub", "bare", "leath", "heel", "sand"];
/// Seat materials (`sit_sofa_scoot_<material>`).
pub const SEATS: [&str; 4] = ["cloth", "leath", "wood", "cment"];

/// Sounds the game plays outside animations.
pub const EXTRA_SOUNDS: &[&str] = &[
    // Interface.
    "ui_piemenu_primary",
    "ui_primary_button",
    "ui_button_fw",
    "ui_button_ffw",
    "ui_button_zoom_in",
    "ui_button_zoom_out",
    "ui_object_plop",
    "ui_build_flooring_plop",
    "ui_build_wallcovering_plop",
    "ui_build_design_tool_open",
    "ui_object_sell",
    "ui_cart_add_to",
    "ui_text_notification_open",
    "ui_text_notification_close",
    "ui_hud_panel_open",
    "ui_hud_panel_close",
    "ui_hardwindow_open",
    "ui_hardwindow_close",
    "ui_cas_trait_add",
    "ui_cas_exit",
    "ui_cas_save_success",
    "ui_cas_randomizesub_mup",
    "ui_dream_open",
    "ui_dream_deny",
    "ui_page_turn",
    "ui_style_plop",
    // Stings.
    "sting_career_positive",
    "sting_career_grade_hi",
    "sting_career_grade_lo",
    "sting_career_fail",
    "sting_career_max",
    "sting_job_completion",
    "sting_promise_satisfy",
    "sting_make_wish",
    "sting_lifetime_wish_success",
    "sting_lifetime_achievement",
    "sting_life_great",
    "sting_life_sucks",
    "sting_death",
    "sting_agetrans_b_p",
    "sting_agetrans_p_c",
    "sting_agetrans_c_t",
    "sting_agetrans_t_h",
    "sting_agetrans_h_a",
    "sting_agetrans_a_e",
    "sting_baby_conception",
    "sting_propose",
    "sting_wedding_cake",
    "sting_wedding_anniversary",
    "sting_good_event",
    "sting_bad_event",
    "sting_neutral_event",
    "sting_sm_good_event",
    "sting_sm_bad_event",
    "sting_generic_great",
    "sting_generic_tragic",
    "sting_motive_failure",
    "sting_opp_presented",
    "sting_school_level_up",
    "sting_new_profession",
    "sting_retirement",
    "sting_give_gift_accept",
    "sting_loveletter_accept",
    "sting_masterpiece",
    // Ambience.
    "amb_birds_allday",
    "amb_birds_midday",
    "amb_birds_dusk",
    "amb_world_bird_day",
    "amb_insects_nite",
    "amb_owl_nite",
    "amb_dogs_day",
    "amb_dogs_nite",
    "amb_birds_ocean",
    "amb_ocean_oneshot",
    // Phone.
    "phone_ring_cela",
    "phone_ring_cela_ui",
    "phone_dial_onehand",
    "phone_make_call",
    "phone_talk_chat_hia",
    "phone_talk_chat_tota",
    // Music for game modes.
    "music_theme",
    "music_load",
    "music_mode_buy",
    "music_mode_build",
    "music_mode_cas",
    "music_mode_map",
    "music_mode_edit_town",
];

/// Radio stations (each a playlist of songs).
pub const STATIONS: &[&str] = &[
    "stereo_pop",
    "stereo_rock",
    "stereo_classical",
    "stereo_country",
    "stereo_kids",
    "stereo_latin",
    "stereo_electronica",
    "stereo_hiphop",
    "stereo_indie",
    "stereo_rnb",
    "stereo_rap",
    "stereo_soul",
    "stereo_disco",
    "stereo_darkwave",
    "stereo_geekrock",
    "stereo_songwriter",
    "stereo_roots",
    "stereo_rockabilly",
    "stereo_islandlife",
    "stereo_beachparty",
    "stereo_future",
    "stereo_horror",
    "stereo_superhero",
    "stereo_china",
    "stereo_egypt",
    "stereo_france",
];

/// The concrete sound names a clip cue may stand for.
pub fn expand_cue(name: &str) -> Vec<String> {
    let mut out = vec![name.to_string(), format!("{name}_lp")];
    if name.starts_with("vo_") {
        let voices: &[&str] = if name.ends_with('c') { &CHILD_VOICES } else { &ADULT_VOICES };
        out.extend(voices.iter().map(|v| format!("{name}_{v}")));
    }
    if name.starts_with("foot_") {
        for s in SURFACES {
            for h in SHOES {
                out.push(format!("{name}_{s}_{h}"));
            }
        }
    }
    if name.starts_with("sit_") {
        out.extend(SEATS.iter().map(|m| format!("{name}_{m}")));
    }
    out
}

pub fn sounds_ready(root: &BakeRoot) -> bool {
    let g = root.global_dir();
    g.join("sounds.pack").exists()
        && crate::pack::read_value::<SoundBank>(&g.join("sounds.bin")).is_ok_and(|b| b.version == SOUNDS_VERSION)
}

/// One sample converted to a playable file.
fn convert_sample(pkgs: &PackageSet, id: u64) -> Option<(SampleInfo, Vec<u8>)> {
    let head = pkgs.read_ti(audio::T_SNR, id)?;
    let snr = audio::Snr::parse(&head)?;
    let stream;
    let body: &[u8] = if snr.kind == 0 {
        head.get(snr.header_len..)?
    } else {
        stream = pkgs.read_ti(audio::T_SNS, id)?;
        &stream
    };
    let blocks = audio::blocks(body);
    let info = |format| SampleInfo { format, channels: snr.channels, sample_rate: snr.sample_rate, duration: snr.duration() };
    match snr.codec {
        audio::CODEC_EALAYER3_V1 => {
            let mp3 = audio::ealayer3_to_mp3(&snr, &blocks).ok()?;
            Some((info(FORMAT_MP3), mp3.data))
        }
        audio::CODEC_XAS1 => {
            let pcm = audio::decode_xas(&snr, &blocks);
            Some((info(FORMAT_WAV), audio::wav(&pcm, snr.channels as u16, snr.sample_rate)))
        }
        audio::CODEC_PCM16BE => {
            let pcm: Vec<i16> = blocks.iter().flat_map(|b| b.1.chunks_exact(2).map(|c| i16::from_be_bytes([c[0], c[1]]))).collect();
            Some((info(FORMAT_WAV), audio::wav(&pcm, snr.channels as u16, snr.sample_rate)))
        }
        _ => None,
    }
}

/// Bakes the sound bank: every sound the clips in `clips.pack` ask for plus [`EXTRA_SOUNDS`]
/// and [`STATIONS`].
pub fn bake_sounds(root: &BakeRoot, pkgs: &PackageSet, progress: Progress) -> Result<usize, String> {
    progress("Converting: sounds…");
    let gdir = root.global_dir();
    // Sound property records, merged across packs (expansions add songs to stations).
    let mut records: HashMap<u64, Vec<SoundProps>> = HashMap::new();
    for k in pkgs.keys_of_type(audio::T_SOUND_PROPS) {
        if let Some(d) = pkgs.read(k) {
            records.entry(k.i).or_default().push(SoundProps::parse(&d));
        }
    }
    // The cues of the baked clips.
    let mut names: Vec<String> = Vec::new();
    if let (Ok(reader), Ok(clip_names)) = (PackReader::open(&gdir.join("clips.pack")), crate::pack::read_value::<Vec<String>>(&gdir.join("clip_names.bin"))) {
        let mut cues = HashSet::new();
        for n in &clip_names {
            let Some(bytes) = reader.get::<Vec<u8>>(&crate::clips::clip_key(n)) else { continue };
            let Ok(raw) = lz4_flex::decompress_size_prepended(&bytes) else { continue };
            let Ok(clip) = postcard::from_bytes::<s3formats::sim::Clip>(&raw) else { continue };
            cues.extend(clip.sounds.into_iter().map(|s| s.name));
        }
        for c in cues {
            names.extend(expand_cue(&c));
        }
    }
    names.extend(EXTRA_SOUNDS.iter().map(|s| s.to_string()));
    names.extend(STATIONS.iter().map(|s| s.to_string()));
    names.sort();
    names.dedup();

    // Resolve each name through its parent chain.
    let first = |id: u64, f: &dyn Fn(&SoundProps) -> Option<f32>| -> Option<f32> {
        let mut cur = Some(id);
        for _ in 0..8 {
            let recs = records.get(&cur?)?;
            if let Some(v) = recs.iter().find_map(f) {
                return Some(v);
            }
            cur = recs.iter().find_map(|r| r.parent());
        }
        None
    };
    let mut sounds = HashMap::new();
    for name in &names {
        let id = s3pkg::fnv64(name);
        let Some(recs) = records.get(&id) else { continue };
        let mut samples: Vec<u64> = recs.iter().flat_map(|r| r.samples().iter().copied()).collect();
        let mut cur = recs.iter().find_map(|r| r.parent());
        for _ in 0..8 {
            if !samples.is_empty() {
                break;
            }
            let Some(p) = cur.and_then(|p| records.get(&p)) else { break };
            samples = p.iter().flat_map(|r| r.samples().iter().copied()).collect();
            cur = p.iter().find_map(|r| r.parent());
        }
        samples.dedup();
        if samples.is_empty() {
            continue;
        }
        let int = |h: u32| move |r: &SoundProps| match r.get(h) {
            Some(audio::PropValue::Int(v)) => Some(*v as f32),
            _ => None,
        };
        sounds.insert(
            id,
            SoundDef {
                samples,
                gain: first(id, &|r| r.float(audio::P_GAIN)).unwrap_or(1.0),
                min_distance: first(id, &|r| r.float(audio::P_MIN_DISTANCE)).unwrap_or(0.0),
                max_distance: first(id, &|r| r.float(audio::P_MAX_DISTANCE)).unwrap_or(0.0),
                looping: first(id, &int(audio::P_LOOPING)).is_some_and(|v| v != 0.0),
            },
        );
    }
    // Convert the samples.
    let mut ids: Vec<u64> = sounds.values().flat_map(|d| d.samples.iter().copied()).collect();
    ids.sort();
    ids.dedup();
    progress(&format!("Converting: {} sounds with {} samples…", sounds.len(), ids.len()));
    let converted = par_map(&ids, |&id| convert_sample(pkgs, id));
    let converted: Vec<_> = converted;
    let tmp = gdir.join("sounds.pack.tmp");
    let mut w = PackWriter::create(&tmp).map_err(|e| e.to_string())?;
    if std::env::var_os("SOUND_STATS").is_some() {
        let size: HashMap<u64, (usize, u8, f32)> =
            ids.iter().zip(&converted).filter_map(|(id, c)| c.as_ref().map(|(i, d)| (*id, (d.len(), i.format, i.duration)))).collect();
        let mut by_cat: std::collections::BTreeMap<String, (usize, usize, usize, f32)> = Default::default();
        for name in &names {
            let Some(d) = sounds.get(&s3pkg::fnv64(name)) else { continue };
            let cat = name.split('_').take(2).collect::<Vec<_>>().join("_");
            let e = by_cat.entry(cat).or_default();
            for s in &d.samples {
                if let Some((n, f, dur)) = size.get(s) {
                    if *f == FORMAT_MP3 { e.0 += n } else { e.1 += n }
                    e.2 += 1;
                    e.3 += dur;
                }
            }
        }
        let mut v: Vec<_> = by_cat.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1.0 + x.1.1));
        for (c, (m, wv, n, dur)) in v.iter().take(40) {
            println!("{c:30} mp3 {:6} KB  wav {:6} KB  {n} samples {dur:.0}s", m / 1000, wv / 1000);
        }
    }
    let mut samples = HashMap::new();
    let (mut mp3, mut wav, mut failed, mut bytes) = (0, 0, 0, 0usize);
    for (id, c) in ids.iter().zip(converted) {
        let c: Option<(SampleInfo, Vec<u8>)> = c;
        match c {
            Some((info, data)) => {
                if info.format == FORMAT_MP3 { mp3 += 1 } else { wav += 1 }
                bytes += data.len();
                w.add((audio::T_SNR, 0, *id), &data).map_err(|e| e.to_string())?;
                samples.insert(*id, info);
            }
            None => failed += 1,
        }
    }
    w.finish().map_err(|e| e.to_string())?;
    for d in sounds.values_mut() {
        d.samples.retain(|s| samples.contains_key(s));
    }
    sounds.retain(|_, d| !d.samples.is_empty());
    std::fs::rename(&tmp, gdir.join("sounds.pack")).map_err(|e| e.to_string())?;
    let n = sounds.len();
    write_value(&gdir.join("sounds.bin"), &SoundBank { version: SOUNDS_VERSION, sounds, samples }).map_err(|e| e.to_string())?;
    progress(&format!("Converting: {n} sounds ({mp3} MP3, {wav} WAV, {failed} unsupported samples, {} MB)", bytes / 1_000_000));
    Ok(n)
}
