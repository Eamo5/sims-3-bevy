//! The game's audio: EA "SNR" sample headers with "SNS" block data (types 0x01A527DB and
//! 0x01EEF63A), and the sound property records (0x8070223D) that turn a sound name used by the
//! game (`fridge_door_open_norm`) into a set of sample variants.
//!
//! Two codecs are used: EALayer3 (MPEG layer III granules with a compacted header), which is
//! rewrapped losslessly into standard MP3 frames, and EA-XAS (4-bit ADPCM), decoded to PCM.

pub const T_SNR: u32 = 0x01A527DB;
pub const T_SNS: u32 = 0x01EEF63A;
pub const T_SOUND_PROPS: u32 = 0x8070223D;

pub const CODEC_PCM16BE: u8 = 2;
pub const CODEC_XAS1: u8 = 4;
pub const CODEC_EALAYER3_V1: u8 = 5;
pub const CODEC_EALAYER3_V2_PCM: u8 = 6;
pub const CODEC_EALAYER3_V2_SPIKE: u8 = 7;

#[derive(Debug)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AudioError {}

fn err<T>(s: impl Into<String>) -> Result<T, AudioError> {
    Err(AudioError(s.into()))
}

fn be32(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|s| u32::from_be_bytes(s.try_into().unwrap()))
}

// ---------------------------------------------------------------------------------------------
// SNR header

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snr {
    pub version: u8,
    pub codec: u8,
    pub channels: u8,
    pub sample_rate: u32,
    /// 0 = whole sample in memory (blocks follow the header), 1 = streamed from an SNS resource.
    pub kind: u8,
    pub looping: bool,
    pub samples: u32,
    pub loop_start: u32,
    /// Bytes taken by the header; RAM samples' blocks start here.
    pub header_len: usize,
}

impl Snr {
    pub fn parse(d: &[u8]) -> Option<Snr> {
        let h0 = be32(d, 0)?;
        let h1 = be32(d, 4)?;
        let looping = (h1 >> 29) & 1 == 1;
        let kind = (h1 >> 30) as u8;
        let mut header_len = 8;
        let mut loop_start = 0;
        if looping {
            loop_start = be32(d, 8)?;
            header_len += 4;
            if kind == 1 {
                header_len += 4; // loop offset into the stream
            }
        }
        Some(Snr {
            version: (h0 >> 28) as u8,
            codec: ((h0 >> 24) & 15) as u8,
            channels: ((h0 >> 18) & 63) as u8 + 1,
            sample_rate: h0 & 0x3FFFF,
            kind,
            looping,
            samples: h1 & 0x1FFF_FFFF,
            loop_start,
            header_len,
        })
    }

    pub fn duration(&self) -> f32 {
        self.samples as f32 / self.sample_rate.max(1) as f32
    }
}

/// Payloads of the SNS-style blocks (flag byte, 24-bit size, 32-bit sample count, data) found
/// after a RAM sample's header or in a stream resource. Stops at the block flagged last.
pub fn blocks(d: &[u8]) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    let mut o = 0;
    while o + 8 <= d.len() {
        let h = be32(d, o).unwrap();
        let flag = (h >> 24) as u8;
        let size = (h & 0xFF_FFFF) as usize;
        if size < 8 || o + size > d.len() {
            break;
        }
        let samples = be32(d, o + 4).unwrap();
        out.push((samples, &d[o + 8..o + size]));
        o += size;
        if flag & 0x80 != 0 {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// EA-XAS v1

const XA_COEF: [(i32, i32); 4] = [(0, 0), (240, 0), (460, -208), (392, -220)];

/// Decodes EA-XAS v1 blocks to interleaved 16-bit PCM.
pub fn decode_xas(snr: &Snr, blocks: &[(u32, &[u8])]) -> Vec<i16> {
    let ch = snr.channels as usize;
    let mut out = Vec::with_capacity(snr.samples as usize * ch);
    for &(samples, data) in blocks {
        let frames = (samples as usize).div_ceil(128);
        let mut left = samples as usize;
        for f in 0..frames {
            let n = left.min(128);
            let mut decoded = vec![[0i16; 128]; ch];
            for (c, dec) in decoded.iter_mut().enumerate() {
                let base = (f * ch + c) * 0x4C;
                let Some(frame) = data.get(base..base + 0x4C) else { continue };
                for g in 0..4 {
                    let h = u32::from_le_bytes(frame[g * 4..g * 4 + 4].try_into().unwrap());
                    let (c1, c2) = XA_COEF[(h & 3) as usize];
                    let mut h2 = (h & 0xFFF0) as u16 as i16 as i32;
                    let mut h1 = ((h >> 16) & 0xFFF0) as u16 as i16 as i32;
                    let shift = (h >> 16) & 0x0F;
                    let mut k = g * 32;
                    dec[k] = h2 as i16;
                    dec[k + 1] = h1 as i16;
                    k += 2;
                    for row in 0..15 {
                        let byte = frame[16 + row * 4 + g];
                        for nib in [byte >> 4, byte & 15] {
                            let s = (((nib as i32) << 28) >> 28 << 12) >> shift;
                            let v = (((s << 8) + h1 * c1 + h2 * c2 + 128) >> 8).clamp(-32768, 32767);
                            dec[k] = v as i16;
                            k += 1;
                            h2 = h1;
                            h1 = v;
                        }
                    }
                }
            }
            for i in 0..n {
                for dec in &decoded {
                    out.push(dec[i]);
                }
            }
            left -= n;
        }
    }
    out
}


// ---------------------------------------------------------------------------------------------
// EALayer3 -> MP3

struct BitReader<'a> {
    d: &'a [u8],
    pos: usize,
}

impl BitReader<'_> {
    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = *self.d.get(self.pos / 8)?;
            v = (v << 1) | ((byte >> (7 - self.pos % 8)) & 1) as u32;
            self.pos += 1;
        }
        Some(v)
    }
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    bits: usize,
}

impl BitWriter {
    fn put(&mut self, n: u32, v: u32) {
        for i in (0..n).rev() {
            if self.bits % 8 == 0 {
                self.out.push(0);
            }
            if (v >> i) & 1 == 1 {
                *self.out.last_mut().unwrap() |= 0x80 >> (self.bits % 8);
            }
            self.bits += 1;
        }
    }
    fn copy_bits(&mut self, r: &mut BitReader, n: usize) -> Option<()> {
        for _ in 0..n {
            let b = r.bits(1)?;
            self.put(1, b);
        }
        Some(())
    }
}

/// One EALayer3 granule: the MPEG header fields, side info and main data of one granule.
#[derive(Clone)]
struct Granule {
    version_index: u32,
    sample_rate_index: u32,
    channel_mode: u32,
    mode_extension: u32,
    index: u32,
    scfsi: [u32; 2],
    side: Vec<(u32, u32, u32)>,
    /// Main data, bit-packed (channel after channel).
    data: Vec<u8>,
    data_bits: usize,
    /// PCM block (EALayer3 v1 0xEE frames): decoded samples it replaces, and the samples.
    pcm: Option<(u32, Vec<i16>)>,
}

impl Granule {
    fn mpeg1(&self) -> bool {
        self.version_index == 3
    }
    fn channels(&self) -> usize {
        if self.channel_mode == 3 { 1 } else { 2 }
    }
    fn sample_rate(&self) -> u32 {
        const RATES: [[u32; 3]; 4] = [[11025, 12000, 8000], [0, 0, 0], [22050, 24000, 16000], [44100, 48000, 32000]];
        RATES[self.version_index as usize].get(self.sample_rate_index as usize).copied().unwrap_or(0)
    }
}

/// Parses the EALayer3 v1 frames of one block's payload.
fn parse_granules_v1(d: &[u8], channels_hint: usize, v1b: bool, out: &mut Vec<Granule>) -> Option<()> {
    let mut r = BitReader { d, pos: 0 };
    while r.pos / 8 + 2 <= d.len() {
        let start = r.pos;
        let flag = r.bits(8)?;
        if flag != 0 && flag != 0xEE {
            return None;
        }
        let version_index = r.bits(2)?;
        let sample_rate_index = r.bits(2)?;
        let channel_mode = r.bits(2)?;
        let mode_extension = r.bits(2)?;
        if version_index == 0 && sample_rate_index == 0 && channel_mode == 0 && mode_extension == 0 {
            break; // padding at the end of a block
        }
        if version_index == 1 || sample_rate_index == 3 {
            return None;
        }
        let mpeg1 = version_index == 3;
        let channels = if channel_mode == 3 { 1 } else { 2 };
        let index = r.bits(1)?;
        let mut scfsi = [0; 2];
        if mpeg1 && index == 1 {
            for s in scfsi.iter_mut().take(channels) {
                *s = r.bits(4)?;
            }
        }
        let mut side = Vec::with_capacity(channels);
        for _ in 0..channels {
            let size = r.bits(12)?;
            let o1 = r.bits(32)?;
            let o2 = r.bits(if mpeg1 { 15 } else { 19 })?;
            side.push((size, o1, o2));
        }
        let data_bits: usize = side.iter().map(|s| s.0 as usize).sum();
        let mut w = BitWriter::default();
        w.copy_bits(&mut r, data_bits)?;
        let used = r.pos - start;
        if used % 8 != 0 {
            r.pos += 8 - used % 8;
        }
        let mut pcm = None;
        if flag == 0xEE {
            let skip = r.bits(16)?;
            let n = r.bits(16)? as usize;
            if v1b {
                r.pos += 32;
            }
            let ch = channels_hint.max(channels);
            let o = r.pos / 8;
            let bytes = d.get(o..o + n * ch * 2)?;
            let samples = bytes.chunks_exact(2).map(|b| i16::from_be_bytes([b[0], b[1]])).collect();
            r.pos += n * ch * 16;
            pcm = Some((skip, samples));
        }
        out.push(Granule { version_index, sample_rate_index, channel_mode, mode_extension, index, scfsi, side, data: w.out, data_bits, pcm });
    }
    Some(())
}

const BITRATES_V1: [u32; 15] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
const BITRATES_V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];

/// An MP3 rebuilt from EALayer3, plus the PCM the encoder stored beside the MPEG data.
pub struct Mp3 {
    pub data: Vec<u8>,
    pub sample_rate: u32,
    pub channels: u8,
    pub frames: usize,
    /// Leading PCM samples (interleaved) that EA stores uncompressed, and how many decoded
    /// samples they replace.
    pub pcm_prefix: Vec<i16>,
    pub pcm_skip: u32,
    /// Granules carrying PCM blocks, and their PCM samples (for diagnostics).
    pub pcm_blocks: usize,
    pub pcm_total: usize,
    pub granules: usize,
}

/// Rewraps EALayer3 (v1) blocks as a standard MP3 stream. The granules are re-packed into
/// frames with a bit reservoir, choosing for each frame the smallest standard bitrate.
pub fn ealayer3_to_mp3(snr: &Snr, blocks: &[(u32, &[u8])]) -> Result<Mp3, AudioError> {
    if snr.codec != CODEC_EALAYER3_V1 {
        return err(format!("codec {} is not EALayer3 v1", snr.codec));
    }
    let mut granules = Vec::new();
    for &(_, payload) in blocks {
        if parse_granules_v1(payload, snr.channels as usize, snr.version == 0, &mut granules).is_none() {
            break;
        }
    }
    let Some(first) = granules.first() else { return err("no EALayer3 frames") };
    let mpeg1 = first.mpeg1();
    let sample_rate = first.sample_rate();
    let channels = first.channels();
    let mut pcm_prefix = Vec::new();
    let mut pcm_skip = 0;
    if let Some((skip, pcm)) = &first.pcm {
        pcm_prefix = pcm.clone();
        pcm_skip = *skip;
    }
    let pcm_blocks = granules.iter().filter(|g| g.pcm.is_some()).count();
    let pcm_total = granules.iter().filter_map(|g| g.pcm.as_ref()).map(|p| p.1.len()).sum();
    let n_granules = granules.len();
    // Group granules into MPEG frames.
    let mut frames: Vec<Vec<&Granule>> = Vec::new();
    let mut i = 0;
    while i < granules.len() {
        if mpeg1 {
            if i + 1 >= granules.len() {
                break;
            }
            let (a, b) = (&granules[i], &granules[i + 1]);
            if a.index != 0 || b.index != 1 {
                i += 1;
                continue;
            }
            frames.push(vec![a, b]);
            i += 2;
        } else {
            frames.push(vec![&granules[i]]);
            i += 1;
        }
    }
    let side_len = match (mpeg1, channels) {
        (true, 1) => 17,
        (true, _) => 32,
        (false, 1) => 9,
        (false, _) => 17,
    };
    let max_back = if mpeg1 { 511 } else { 255 };
    let rates = if mpeg1 { &BITRATES_V1 } else { &BITRATES_V2 };
    let per_kbit = if mpeg1 { 144 } else { 72 };
    let frame_len = |b: usize| (per_kbit * rates[b] * 1000 / sample_rate) as usize;
    // Main data of each frame, byte-aligned.
    let mains: Vec<Vec<u8>> = frames
        .iter()
        .map(|gs| {
            let mut w = BitWriter::default();
            for g in gs {
                let mut r = BitReader { d: &g.data, pos: 0 };
                w.copy_bits(&mut r, g.data_bits);
            }
            w.out
        })
        .collect();
    // Lay out the main data across all frames' payloads back to back: each frame's data starts
    // as early as the 9-bit (8-bit for MPEG-2) back pointer allows and must end in its frame.
    let mut payload = Vec::new();
    let mut layout = Vec::with_capacity(frames.len()); // (bitrate index, main_data_begin)
    let (mut end, mut ps) = (0usize, 0usize);
    for m in &mains {
        let d = end.max(ps.saturating_sub(max_back));
        let begin = ps - d;
        let need = (d + m.len()).saturating_sub(ps);
        let b = (1..15).find(|&b| frame_len(b) - 4 - side_len >= need).unwrap_or(14);
        let cap = frame_len(b) - 4 - side_len;
        if payload.len() < ps + cap {
            payload.resize(ps + cap, 0);
        }
        let room = (ps + cap - d).min(m.len());
        payload[d..d + room].copy_from_slice(&m[..room]);
        end = d + room;
        layout.push((b, begin));
        ps += cap;
    }
    let mut out = Vec::with_capacity(ps + frames.len() * (4 + side_len));
    let mut ps = 0;
    for (gs, &(b, begin)) in frames.iter().zip(&layout) {
        let g0 = gs[0];
        let mut w = BitWriter::default();
        w.put(11, 0x7FF);
        w.put(2, g0.version_index);
        w.put(2, 1); // layer III
        w.put(1, 1); // no CRC
        w.put(4, b as u32);
        w.put(2, g0.sample_rate_index);
        w.put(1, 0); // padding
        w.put(1, 0); // private
        w.put(2, g0.channel_mode);
        w.put(2, g0.mode_extension);
        w.put(1, 0); // copyright
        w.put(1, 1); // original
        w.put(2, 0); // emphasis
        if mpeg1 {
            w.put(9, begin as u32);
            w.put(if channels == 1 { 5 } else { 3 }, 0);
            for c in 0..channels {
                w.put(4, gs[1].scfsi[c]);
            }
        } else {
            w.put(8, begin as u32);
            w.put(if channels == 1 { 1 } else { 2 }, 0);
        }
        for g in gs {
            for &(size, o1, o2) in &g.side {
                w.put(12, size);
                w.put(32, o1);
                w.put(if mpeg1 { 15 } else { 19 }, o2);
            }
        }
        debug_assert_eq!(w.out.len(), 4 + side_len);
        out.extend_from_slice(&w.out);
        let cap = frame_len(b) - 4 - side_len;
        out.extend_from_slice(&payload[ps..ps + cap]);
        ps += cap;
    }
    Ok(Mp3 { data: out, sample_rate, channels: channels as u8, frames: frames.len(), pcm_prefix, pcm_skip, pcm_blocks, pcm_total, granules: n_granules })
}

// ---------------------------------------------------------------------------------------------
// WAV

/// A 16-bit PCM WAV file.
pub fn wav(pcm: &[i16], channels: u16, sample_rate: u32) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut o = Vec::with_capacity(44 + pcm.len() * 2);
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&(36 + data_len).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    o.extend_from_slice(&16u32.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&channels.to_le_bytes());
    o.extend_from_slice(&sample_rate.to_le_bytes());
    o.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
    o.extend_from_slice(&(channels * 2).to_le_bytes());
    o.extend_from_slice(&16u16.to_le_bytes());
    o.extend_from_slice(b"data");
    o.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        o.extend_from_slice(&s.to_le_bytes());
    }
    o
}

// ---------------------------------------------------------------------------------------------
// Name maps and sound properties

/// A name map (0x0166038C): instance id -> name.
pub fn parse_name_map(d: &[u8]) -> Vec<(u64, String)> {
    let le32 = |o: usize| d.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
    let mut out = Vec::new();
    let Some(n) = le32(4) else { return out };
    let mut o = 8;
    for _ in 0..n {
        let Some(id) = d.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())) else { break };
        let Some(len) = le32(o + 8) else { break };
        let len = len as usize;
        let Some(s) = d.get(o + 12..o + 12 + len) else { break };
        out.push((id, String::from_utf8_lossy(s).into_owned()));
        o += 12 + len;
    }
    out
}

pub const P_PARENT: u32 = 0x5F6317D5;
pub const P_SAMPLES: u32 = 0x701ED91E;
/// Linear gain.
pub const P_GAIN: u32 = 0x25DF0108;
/// Distance (metres) at which the sound fades out.
pub const P_MAX_DISTANCE: u32 = 0x03593710;
/// Distance within which the sound plays at full volume.
pub const P_MIN_DISTANCE: u32 = 0x0F616B72;
/// Whether the sample loops.
pub const P_LOOPING: u32 = 0x6DD08218;

/// One value of a sound property record.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Float(f32),
    Int(u32),
    Floats(Vec<f32>),
    Keys(Vec<u64>),
    Strings(Vec<String>),
}

/// A sound property record (0x8070223D, big-endian): sample variants, a parent template and
/// tuning values, keyed by property-name hash.
#[derive(Clone, Debug, Default)]
pub struct SoundProps {
    pub props: Vec<(u32, PropValue)>,
}

impl SoundProps {
    pub fn parse(d: &[u8]) -> SoundProps {
        let mut props = Vec::new();
        let Some(n) = be32(d, 0) else { return SoundProps { props } };
        let mut o = 4;
        // One scalar of a type, returning the value and its size.
        let scalar = |kind: u32, o: usize| -> Option<(PropValue, usize)> {
            Some(match kind {
                0x01 => (PropValue::Int(*d.get(o)? as u32), 1),
                0x0A => (PropValue::Int(be32(d, o)?), 4),
                0x0D => (PropValue::Float(f32::from_bits(be32(d, o)?)), 4),
                0x13 => {
                    let len = be32(d, o)? as usize;
                    let chars = d.get(o + 4..o + 4 + len * 2)?;
                    let units: Vec<u16> = chars.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                    (PropValue::Strings(vec![String::from_utf16_lossy(&units)]), 4 + len * 2)
                }
                0x3E8 => (PropValue::Keys(vec![u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?)]), 16),
                _ => return None,
            })
        };
        for _ in 0..n {
            let (Some(hash), Some(tf)) = (be32(d, o), be32(d, o + 4)) else { break };
            let (kind, flags) = (tf >> 16, tf & 0xFFFF);
            o += 8;
            let value = if flags & 0x0C != 0 {
                let (Some(count), Some(_)) = (be32(d, o), be32(d, o + 4)) else { break };
                o += 8;
                let mut items = Vec::new();
                for _ in 0..count {
                    let Some((v, size)) = scalar(kind, o) else { break };
                    items.push(v);
                    o += size;
                }
                if items.len() < count as usize {
                    break;
                }
                // Merge the items into one value of the same kind.
                match kind {
                    0x3E8 => PropValue::Keys(items.iter().flat_map(|v| if let PropValue::Keys(k) = v { k.clone() } else { vec![] }).collect()),
                    0x13 => PropValue::Strings(items.iter().flat_map(|v| if let PropValue::Strings(k) = v { k.clone() } else { vec![] }).collect()),
                    0x0D => PropValue::Floats(items.iter().filter_map(|v| if let PropValue::Float(f) = v { Some(*f) } else { None }).collect()),
                    _ => items.into_iter().next().unwrap_or(PropValue::Int(0)),
                }
            } else {
                let Some((v, size)) = scalar(kind, o) else { break };
                o += size;
                v
            };
            props.push((hash, value));
        }
        SoundProps { props }
    }

    pub fn get(&self, hash: u32) -> Option<&PropValue> {
        self.props.iter().find(|p| p.0 == hash).map(|p| &p.1)
    }

    pub fn samples(&self) -> &[u64] {
        match self.get(P_SAMPLES) {
            Some(PropValue::Keys(k)) => k,
            _ => &[],
        }
    }

    pub fn parent(&self) -> Option<u64> {
        match self.get(P_PARENT) {
            Some(PropValue::Keys(k)) => k.first().copied(),
            _ => None,
        }
    }

    pub fn float(&self, hash: u32) -> Option<f32> {
        match self.get(hash) {
            Some(PropValue::Float(f)) => Some(*f),
            _ => None,
        }
    }
}
