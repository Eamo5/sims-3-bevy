//! Sims: skeleton rigs (new EA format), GEOM body meshes, CAS parts and animation clips.

use std::collections::HashMap;

use crate::rcol::Rcol;
use crate::util::{Eof, R, Reader};
use s3pkg::ResourceKey;

// ---------------------------------------------------------------------------------------------
// RIG 0x8EAF13DE (new format, major 3/4)

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Bone {
    pub name: String,
    pub hash: u32,
    pub parent: i32,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Rig {
    pub name: String,
    pub bones: Vec<Bone>,
}

impl Rig {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let major = r.u32()?;
        let minor = r.u32()?;
        if !(3..=4).contains(&major) || !(1..=2).contains(&minor) {
            return Err(Eof);
        }
        let n = r.i32()?.max(0) as usize;
        if n > 2048 {
            return Err(Eof);
        }
        let mut bones = Vec::with_capacity(n);
        for _ in 0..n {
            let position = r.vec3()?;
            let rotation = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
            let scale = r.vec3()?;
            let nl = r.i32()?.max(0) as usize;
            let name = String::from_utf8_lossy(r.bytes(nl)?).into_owned();
            let _opposite = r.i32()?;
            let parent = r.i32()?;
            let hash = r.u32()?;
            let _flags = r.u32()?;
            bones.push(Bone { name, hash, parent, position, rotation, scale });
        }
        let name = if let Ok(nl) = r.i32() { String::from_utf8_lossy(r.bytes(nl.max(0) as usize).unwrap_or(&[])).into_owned() } else { String::new() };
        Ok(Self { name, bones })
    }

    pub fn index_of(&self, hash: u32) -> Option<usize> {
        self.bones.iter().position(|b| b.hash == hash)
    }
}

// ---------------------------------------------------------------------------------------------
// GEOM 0x015A1849

#[derive(Clone, Debug, Default)]
pub struct Geom {
    pub shader: u32,
    pub params: HashMap<u32, crate::model::ParamValue>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub bone_indices: Vec<[u8; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub bone_hashes: Vec<u32>,
    pub keys: Vec<ResourceKey>,
    /// Vertex ids (shared by a mesh and its morphs), when the mesh has them.
    pub ids: Vec<u32>,
}

impl Geom {
    pub fn parse(resource: &[u8]) -> R<Self> {
        let rcol = Rcol::parse(resource)?;
        let ci = rcol.find_tag(b"GEOM").ok_or(Eof)?;
        let d = rcol.chunk_data(ci).ok_or(Eof)?;
        let mut r = Reader::new(d);
        r.skip(4)?;
        let version = r.u32()?;
        let tgi_rel = r.u32()? as usize;
        let tgi_pos = r.pos + tgi_rel;
        let _tgi_size = r.u32()?;
        let shader = r.u32()?;
        // TGI table first, so texture params can resolve.
        let mut keys = Vec::new();
        {
            let mut t = Reader::at(d, tgi_pos);
            let n = t.u32()? as usize;
            for _ in 0..n.min(256) {
                let ty = t.u32()?;
                let g = t.u32()?;
                let i = t.u64()?;
                keys.push(ResourceKey::new(ty, g, i));
            }
        }
        let mut params = HashMap::new();
        if shader != 0 {
            let size = r.u32()? as usize;
            let start = r.pos;
            let keys_ref = &keys;
            params = crate::model::parse_mtnf(d, start, &|raw| keys_ref.get(raw as usize).copied()).unwrap_or_default();
            r.pos = start + size;
        }
        let _merge = r.u32()?;
        let _sort = r.u32()?;
        let vcount = r.i32()?.max(0) as usize;
        let ecount = r.i32()?.max(0) as usize;
        let mut elems = Vec::with_capacity(ecount);
        for _ in 0..ecount {
            let usage = r.u32()?;
            let dtype = r.u32()?;
            let size = r.u8()?;
            elems.push((usage, dtype, size));
        }
        let mut g = Geom { shader, params, keys, ..Default::default() };
        g.positions.reserve(vcount);
        let mut uv_seen;
        for _ in 0..vcount {
            let mut pos = [0.0; 3];
            let mut nrm = [0.0, 1.0, 0.0];
            let mut uv = [0.0; 2];
            let mut bi = [0u8; 4];
            let mut bw = [0.0f32; 4];
            uv_seen = false;
            for &(usage, _dtype, size) in &elems {
                let start = r.pos;
                match usage {
                    1 => pos = r.vec3()?,
                    2 => nrm = r.vec3()?,
                    // As stored (top-left origin, like the textures).
                    3 if !uv_seen => {
                        uv = [r.f32()?, r.f32()?];
                        uv_seen = true;
                    }
                    4 => bi = r.bytes(4)?.try_into().unwrap(),
                    10 if size == 4 => g.ids.push(r.u32()?),
                    5 => {
                        if size == 16 {
                            bw = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
                        } else {
                            let b = r.bytes(4)?;
                            bw = [b[0] as f32 / 255.0, b[1] as f32 / 255.0, b[2] as f32 / 255.0, b[3] as f32 / 255.0];
                        }
                    }
                    _ => {}
                }
                r.pos = start + size as usize;
            }
            g.positions.push(pos);
            g.normals.push(nrm);
            g.uvs.push(uv);
            g.bone_indices.push(bi);
            g.weights.push(bw);
        }
        let _item_count = r.u32()?;
        let fp_size = r.u8()? as usize;
        let n_idx = r.u32()? as usize;
        g.indices.reserve(n_idx);
        for _ in 0..n_idx {
            let v = if fp_size == 4 { r.u32()? } else { r.u16()? as u32 };
            g.indices.push(v);
        }
        if version == 5 {
            let _skcon = r.i32()?;
        } else {
            // Later versions add UV stitch / seam data before the bone list; give up on bones.
            return Ok(g);
        }
        let nb = r.u32()? as usize;
        for _ in 0..nb.min(512) {
            g.bone_hashes.push(r.u32()?);
        }
        Ok(g)
    }
}

// ---------------------------------------------------------------------------------------------
// CASP 0x034AEECB

pub const AGE_BABY: u32 = 0x01;
pub const AGE_TODDLER: u32 = 0x02;
pub const AGE_CHILD: u32 = 0x04;
pub const AGE_TEEN: u32 = 0x08;
pub const AGE_YOUNG_ADULT: u32 = 0x10;
pub const AGE_ADULT: u32 = 0x20;
pub const AGE_ELDER: u32 = 0x40;
pub const GENDER_MALE: u32 = 0x1000;
pub const GENDER_FEMALE: u32 = 0x2000;

pub const CT_HAIR: u32 = 1;
pub const CT_SCALP: u32 = 2;
pub const CT_FACE: u32 = 3;
pub const CT_BODY: u32 = 4;
pub const CT_TOP: u32 = 5;
pub const CT_BOTTOM: u32 = 6;
pub const CT_SHOES: u32 = 7;
pub const CT_EYEBROW: u32 = 0x16;
pub const CT_GLASSES: u32 = 12;
pub const CT_BEARD: u32 = 16;
pub const CT_LIPSTICK: u32 = 17;
pub const CT_EYESHADOW: u32 = 18;
/// Masks (a burglar's), gloves and stockings: worn as layers over the face, hands and legs.
pub const CT_MASK: u32 = 21;
pub const CT_GLOVES: u32 = 24;
pub const CT_STOCKINGS: u32 = 25;

pub const CAT_NAKED: u32 = 0x1;
pub const CAT_EVERYDAY: u32 = 0x2;
pub const CAT_FORMAL: u32 = 0x4;
pub const CAT_SLEEP: u32 = 0x8;
pub const CAT_SWIM: u32 = 0x10;
pub const CAT_ATHLETIC: u32 = 0x20;
pub const CAT_VALID_RANDOM: u32 = 0x200000;
pub const CAT_HIDDEN: u32 = 0x1000000;

#[derive(Clone, Debug, Default)]
pub struct CasPart {
    pub name: String,
    pub clothing_type: u32,
    pub data_type: u32,
    pub age_gender: u32,
    pub category: u32,
    pub vpxy: Vec<ResourceKey>,
    pub diffuse: Vec<ResourceKey>,
    pub presets: Vec<String>,
    pub keys: Vec<ResourceKey>,
    /// Body-shape blends (BBLN): fat, fit, thin, special.
    pub blends: [Option<ResourceKey>; 4],
}

fn str7_be(r: &mut Reader) -> R<String> {
    let mut n = 0usize;
    let mut shift = 0;
    loop {
        let b = r.u8()?;
        n |= ((b & 0x7F) as usize) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    let b = r.bytes(n)?;
    Ok(String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)])).collect::<Vec<_>>()))
}

impl CasPart {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let _version = r.u32()?;
        let tgi_rel = r.u32()? as usize;
        let tgi_pos = 8 + tgi_rel;
        // IGT key table.
        let mut keys = Vec::new();
        {
            let mut t = Reader::at(d, tgi_pos);
            let n = t.u8()? as usize;
            for _ in 0..n {
                let i = t.u64()?;
                let g = t.u32()?;
                let ty = t.u32()?;
                keys.push(ResourceKey::new(ty, g, i));
            }
        }
        let npresets = r.u32()? as usize;
        let mut presets = Vec::new();
        for _ in 0..npresets.min(64) {
            let chars = r.i32()?.max(0) as usize;
            let b = r.bytes(chars * 2)?;
            presets.push(String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()));
            r.u32()?;
        }
        let name = str7_be(&mut r)?;
        let _sort = r.f32()?;
        let _unique = r.u8()?;
        let clothing_type = r.u32()?;
        let data_type = r.u32()?;
        let age_gender = r.u32()?;
        let category = r.u32()?;
        let _naked = r.u8()?;
        let _parent = r.u8()?;
        // Body-shape blends: fat, fit, thin, special.
        let mut blends = [None; 4];
        for b in blends.iter_mut() {
            *b = keys.get(r.u8()? as usize).copied().filter(|k: &ResourceKey| k.t == T_BLEND);
        }
        let _overlay = r.u32()?;
        let nv = r.u8()? as usize;
        let mut vpxy = Vec::new();
        for _ in 0..nv {
            let i = r.u8()? as usize;
            if let Some(k) = keys.get(i) {
                vpxy.push(*k);
            }
        }
        let nl = r.u8()? as usize;
        for _ in 0..nl {
            r.u8()?;
            r.u32()?;
            let m = r.u8()? as usize;
            r.skip(m * 12)?;
        }
        let nd = r.u8()? as usize;
        let mut diffuse = Vec::new();
        for _ in 0..nd {
            let i = r.u8()? as usize;
            if let Some(k) = keys.get(i) {
                diffuse.push(*k);
            }
        }
        Ok(Self { name, clothing_type, data_type, age_gender, category, vpxy, diffuse, presets, keys, blends })
    }

    /// GEOM keys for LOD 0 from the part's first VPXY.
    pub fn lod0_geoms(&self, pkgs: &s3pkg::PackageSet) -> Vec<ResourceKey> {
        let Some(vk) = self.vpxy.first() else { return Vec::new() };
        let Some(d) = pkgs.read(vk).or_else(|| pkgs.read_ti(vk.t, vk.i)) else { return Vec::new() };
        vpxy_lod_geoms(&d, 0)
    }
}

/// Reads a VPXY resource's GEOM keys for one LOD (entry kind 0).
pub fn vpxy_lod_geoms(d: &[u8], want_lod: u8) -> Vec<ResourceKey> {
    let Ok(rcol) = Rcol::parse(d) else { return Vec::new() };
    let Some(ci) = rcol.find_tag(b"VPXY") else { return Vec::new() };
    let Some(c) = rcol.chunk_data(ci) else { return Vec::new() };
    let keys = crate::model::tgi_table_at(c, 8).unwrap_or_default();
    let mut r = Reader::at(c, 16);
    let Ok(n) = r.u8() else { return Vec::new() };
    // Meshes per LOD; parts without the wanted LOD (baby bodies start at 1) use the most
    // detailed one they have.
    let mut lods: std::collections::BTreeMap<u8, Vec<ResourceKey>> = Default::default();
    for _ in 0..n {
        let Ok(kind) = r.u8() else { break };
        if kind == 0 {
            let (Ok(lod), Ok(m)) = (r.u8(), r.u8()) else { break };
            for _ in 0..m {
                let Ok(i) = r.u32() else { break };
                if let Some(k) = keys.get(i as usize) {
                    lods.entry(lod).or_default().push(*k);
                }
            }
        } else if r.u32().is_err() {
            break;
        }
    }
    match lods.remove(&want_lod) {
        Some(v) => v,
        None => lods.into_values().next().unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------------------------
// CLIP 0x6B20C4F3

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub translation: Vec<(f32, [f32; 3])>,
    pub rotation: Vec<(f32, [f32; 4])>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Clip {
    pub name: String,
    pub duration: f32,
    pub tracks: HashMap<u32, Track>,
    /// Sound cues from the clip's event table, in time order.
    pub sounds: Vec<ClipSound>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SoundAction {
    #[default]
    Play,
    StartLoop,
    StopLoop,
}

/// A sound the clip asks for at a point in time. `name` is the game's sound name, e.g.
/// `fridge_door_open_norm`, or a footstep stem (`foot_step`) completed by surface and shoe.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClipSound {
    pub time: f32,
    pub name: String,
    pub action: SoundAction,
}

/// Splits a script event like `play_sound_a_chp__cheap___a_norm__normal___a_exp__expensive`
/// into the action and the sound for the normal-quality object.
pub fn script_sound(script: &str) -> Option<(SoundAction, String)> {
    let lower = script.to_ascii_lowercase();
    let (action, rest) = if let Some(r) = lower.strip_prefix("play_sound_") {
        (SoundAction::Play, r)
    } else if let Some(r) = lower.strip_prefix("start_looping_sound_") {
        (SoundAction::StartLoop, r)
    } else if let Some(r) = lower.strip_prefix("stop_looping_sound_") {
        (SoundAction::StopLoop, r)
    } else {
        return None;
    };
    let tiers: Vec<&str> = rest.split("___").collect();
    let pick = tiers.iter().find(|t| t.ends_with("__normal")).or(tiers.first())?;
    let name = pick.split("__").next()?.trim_matches('_');
    (!name.is_empty()).then(|| (action, name.to_string()))
}

/// The sound cues of a CLIP resource's event table ("=CE="). Events are found by their
/// header pattern (time followed by two -1.0 floats), so unknown event kinds are skipped.
pub fn clip_sounds(res: &[u8]) -> Vec<ClipSound> {
    let mut out = Vec::new();
    let Ok(rel) = u32_at(res, 0x18) else { return out };
    let start = 0x18 + rel as usize;
    if res.get(start..start + 4) != Some(b"=CE=") {
        return out;
    }
    let count = u32_at(res, start + 8).unwrap_or(0) as usize;
    let end = (start + 20 + u32_at(res, start + 12).unwrap_or(0) as usize + 64).min(res.len());
    let mut o = start + 20;
    let mut found = 0;
    while o + 28 <= end && found < count {
        let kind = u16_at(res, o).unwrap_or(0);
        let a = u32_at(res, o + 12).unwrap_or(0);
        let b = u32_at(res, o + 16).unwrap_or(0);
        if !(1..=40).contains(&kind) || a != 0xBF80_0000 || b != 0xBF80_0000 {
            o += 4;
            continue;
        }
        found += 1;
        let time = f32_at(res, o + 8).unwrap_or(0.0);
        let len = u32_at(res, o + 24).unwrap_or(0) as usize;
        let name = cstr(res, o + 28);
        let payload = o + 28 + ((len + 1 + 3) & !3);
        match kind {
            3 => {
                let sound = cstr(res, payload);
                let sound = if sound.is_empty() { name } else { sound };
                out.push(ClipSound { time, name: sound.to_ascii_lowercase(), action: SoundAction::Play });
            }
            4 => {
                if let Some((action, sound)) = script_sound(&name) {
                    out.push(ClipSound { time, name: sound, action });
                }
            }
            _ => {}
        }
        o = payload;
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

fn u16_at(b: &[u8], o: usize) -> R<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(Eof)
}
fn u32_at(b: &[u8], o: usize) -> R<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(Eof)
}
fn f32_at(b: &[u8], o: usize) -> R<f32> {
    Ok(f32::from_bits(u32_at(b, o)?))
}
fn cstr(b: &[u8], o: usize) -> String {
    let end = b[o.min(b.len())..].iter().position(|&c| c == 0).map(|p| o + p).unwrap_or(b.len());
    String::from_utf8_lossy(&b[o.min(b.len())..end]).into_owned()
}

/// Just the clip's name (cheap), for building a name index.
pub fn clip_name(res: &[u8]) -> Option<String> {
    let off = u32_at(res, 0x0C).ok()? as usize;
    let blob = res.get(0x0C + off..)?;
    if blob.get(0..8)? != b"_pilC3S_" {
        return None;
    }
    let name_off = u32_at(blob, 0x28).ok()? as usize;
    Some(cstr(blob, name_off))
}

impl Clip {
    pub fn parse(res: &[u8]) -> R<Self> {
        if u32_at(res, 0)? != 0x6B20C4F3 {
            return Err(Eof);
        }
        let size = u32_at(res, 0x08)? as usize;
        let off = u32_at(res, 0x0C)? as usize;
        let blob = res.get(0x0C + off..0x0C + off + size).ok_or(Eof)?;
        if &blob[0..8] != b"_pilC3S_" {
            return Err(Eof);
        }
        let dt = f32_at(blob, 0x10)?;
        let ticks = u16_at(blob, 0x14)?;
        let n_curves = u32_at(blob, 0x18)? as usize;
        let n_pal = u32_at(blob, 0x1C)? as usize;
        let info_off = u32_at(blob, 0x20)? as usize;
        let pal_off = u32_at(blob, 0x24)? as usize;
        let name = cstr(blob, u32_at(blob, 0x28)? as usize);
        let mut palette = Vec::with_capacity(n_pal);
        for i in 0..n_pal {
            palette.push(f32_at(blob, pal_off + 4 * i)?);
        }
        let mut infos = Vec::with_capacity(n_curves);
        for c in 0..n_curves {
            let ci = info_off + 20 * c;
            infos.push((
                u32_at(blob, ci)? as usize,
                u32_at(blob, ci + 4)?,
                f32_at(blob, ci + 8)?,
                f32_at(blob, ci + 12)?,
                u16_at(blob, ci + 16)? as usize,
                blob[ci + 18],
                blob[ci + 19],
            ));
        }
        let mut tracks: HashMap<u32, Track> = HashMap::new();
        for (ci, &(data_off, key, offset, scale, n, chan, sub)) in infos.iter().enumerate() {
            if n == 0 {
                continue;
            }
            // Indexed-vector keys may be padded: derive the stride from the next curve.
            let next_off = infos.iter().skip(ci + 1).find(|x| x.4 > 0).map(|x| x.0);
            let mut p = data_off;
            for _ in 0..n {
                let tick = u16_at(blob, p)? as f32;
                let kf = u16_at(blob, p + 2)?;
                p += 4;
                let sgn = |c: usize| (kf >> c) & 1 == 1;
                let deq = |raw: u32, bits: u32, c: usize| {
                    let mut v = raw as f32 / ((1u32 << bits) - 1) as f32;
                    if sgn(c) {
                        v = -v;
                    }
                    v * scale + offset
                };
                let t = tick * dt;
                match chan {
                    0x12 => {
                        let w = u32_at(blob, p)?;
                        p += 4;
                        let v = [deq(w & 0x3FF, 10, 0), deq((w >> 10) & 0x3FF, 10, 1), deq((w >> 20) & 0x3FF, 10, 2)];
                        if sub == 1 {
                            tracks.entry(key).or_default().translation.push((t, v));
                        }
                    }
                    0x14 => {
                        let mut q = [0f32; 4];
                        for (c, qc) in q.iter_mut().enumerate() {
                            *qc = deq(u16_at(blob, p + 2 * c)? as u32 & 0xFFF, 12, c);
                        }
                        p += 8;
                        if sub == 2 {
                            let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-6);
                            tracks.entry(key).or_default().rotation.push((t, [q[0] / l, q[1] / l, q[2] / l, q[3] / l]));
                        }
                    }
                    0x03 => {
                        let mut v = [0f32; 3];
                        for (c, vc) in v.iter_mut().enumerate() {
                            let mut x = palette.get(u16_at(blob, p + 2 * c)? as usize).copied().unwrap_or(0.0);
                            if sgn(c) {
                                x = -x;
                            }
                            *vc = x * scale + offset;
                        }
                        let stride = next_off.map(|no| (no.saturating_sub(data_off)) / n).unwrap_or(10);
                        p += if stride >= 12 { 8 } else { 6 };
                        if sub == 1 {
                            tracks.entry(key).or_default().translation.push((t, v));
                        }
                    }
                    0x04 => {
                        let mut q = [0f32; 4];
                        for (c, qc) in q.iter_mut().enumerate() {
                            let mut x = palette.get(u16_at(blob, p + 2 * c)? as usize).copied().unwrap_or(0.0);
                            if sgn(c) {
                                x = -x;
                            }
                            *qc = x * scale + offset;
                        }
                        p += 8;
                        if sub == 2 {
                            let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-6);
                            tracks.entry(key).or_default().rotation.push((t, [q[0] / l, q[1] / l, q[2] / l, q[3] / l]));
                        }
                    }
                    0x05 => {
                        p += 2;
                        let stride = next_off.map(|no| (no.saturating_sub(data_off)) / n).unwrap_or(6);
                        if stride >= 8 {
                            p += 2;
                        }
                    }
                    _ => break,
                }
            }
        }
        Ok(Self { name, duration: ticks as f32 * dt, tracks, sounds: clip_sounds(res) })
    }
}

// ---------------------------------------------------------------------------------------------
// TONE 0x0354796A (skin tone)

pub const T_TONE: u32 = 0x0354796A;

#[derive(Clone, Debug)]
pub struct ToneTexture {
    pub age_gender: u32,
    pub type_flags: u32,
    pub specular: Option<ResourceKey>,
    pub detail_dark: Option<ResourceKey>,
    pub detail_light: Option<ResourceKey>,
    pub normal: Option<ResourceKey>,
    pub overlay: Option<ResourceKey>,
}

#[derive(Clone, Debug, Default)]
pub struct SkinTone {
    pub ramp: Option<ResourceKey>,
    pub textures: Vec<ToneTexture>,
}

impl SkinTone {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let version = r.u32()?;
        let keys = crate::model::tgi_table_at(d, 4)?;
        r.u32()?;
        r.u32()?;
        let ns = r.u32()? as usize;
        r.skip(ns * 17)?;
        let ramp_idx = r.u32()? as usize;
        let _sub = r.u32()?;
        let nt = r.u32()? as usize;
        let mut textures = Vec::new();
        let key = |i: u32| keys.get(i as usize).copied();
        for _ in 0..nt.min(64) {
            let age_gender = r.u32()?;
            let type_flags = r.u32()?;
            let (s, dd, dl, n, o) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?);
            if version >= 6 {
                r.u32()?;
                r.u32()?;
            }
            textures.push(ToneTexture {
                age_gender,
                type_flags,
                specular: key(s),
                detail_dark: key(dd),
                detail_light: key(dl),
                normal: key(n),
                overlay: key(o),
            });
        }
        Ok(Self { ramp: keys.get(ramp_idx).copied(), textures })
    }

    /// Texture set for an age/gender and part type (2 scalp, 4 face, 8 body).
    pub fn find(&self, age: u32, gender: u32, kind: u32) -> Option<&ToneTexture> {
        self.textures
            .iter()
            .find(|t| t.age_gender & age != 0 && t.age_gender & gender != 0 && t.type_flags & kind != 0)
    }
}

// ---------------------------------------------------------------------------------------------
// BOND 0x0355E0A6 (bone adjustments: face sliders move the face's bones)

pub const T_BOND: u32 = 0x0355E0A6;

/// One bone's adjustment: its name's FNV-32, and what's added to its offset and scale, and
/// its rotation (a quaternion x, y, z, w).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoneAdjust {
    pub bone: u32,
    pub offset: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: [f32; 4],
}

/// A BOND: an RCOL with one untagged chunk of `u32 version, u32 count, count × {u32 bone hash,
/// f32 offset[3], f32 scale[3], f32 quat[4]}`.
pub fn parse_bond(d: &[u8]) -> R<Vec<BoneAdjust>> {
    let rcol = crate::rcol::Rcol::parse(d)?;
    let ch = rcol.chunks.first().ok_or(Eof)?;
    let data = d.get(ch.offset..ch.offset + ch.size).ok_or(Eof)?;
    let mut r = Reader::new(data);
    let _version = r.u32()?;
    let n = r.u32()? as usize;
    if n > 512 {
        return Err(Eof);
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let bone = r.u32()?;
        let mut f = [0f32; 10];
        for v in f.iter_mut() {
            *v = r.f32()?;
        }
        out.push(BoneAdjust { bone, offset: [f[0], f[1], f[2]], scale: [f[3], f[4], f[5]], rotation: [f[6], f[7], f[8], f[9]] });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// BBLN 0x062C8204 (body blends) / FACE 0x0358B08A (face sliders)

pub const T_BLEND: u32 = 0x062C8204;

/// One region of a blend: morph meshes per age/gender (age_gender, amount, key index) and bone
/// morphs.
#[derive(Clone, Debug, Default)]
pub struct BlendEntry {
    pub region: u32,
    pub geoms: Vec<(u32, f32, u32)>,
    pub bones: Vec<(u32, f32, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct BlendInfo {
    pub name: String,
    pub entries: Vec<BlendEntry>,
    pub keys: Vec<ResourceKey>,
    /// Version 8: the BGEO holding the vertex deltas.
    pub bgeo: Option<ResourceKey>,
}

impl BlendInfo {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let version = r.u32()?;
        let tgi_rel = r.u32()? as usize;
        let tgi_pos = 8 + tgi_rel;
        let _tgi_size = r.u32()?;
        let name = str7_be(&mut r)?;
        let mut bgeo = None;
        if version >= 8 {
            let _n = r.u32()?;
            let (t, g, i) = (r.u32()?, r.u32()?, r.u64()?);
            bgeo = Some(ResourceKey::new(t, g, i));
        }
        let n = r.u32()? as usize;
        if n > 256 {
            return Err(Eof);
        }
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let region = r.u32()?;
            let mut list = || -> R<Vec<(u32, f32, u32)>> {
                let m = r.u32()? as usize;
                if m > 256 {
                    return Err(Eof);
                }
                (0..m).map(|_| Ok((r.u32()?, r.f32()?, r.u32()?))).collect()
            };
            let geoms = list()?;
            let bones = list()?;
            entries.push(BlendEntry { region, geoms, bones });
        }
        let mut t = Reader::at(d, tgi_pos);
        let nk = t.u32()? as usize;
        let mut keys = Vec::new();
        for _ in 0..nk.min(512) {
            let ty = t.u32()?;
            let g = t.u32()?;
            let i = t.u64()?;
            keys.push(ResourceKey::new(ty, g, i));
        }
        Ok(Self { name, entries, keys, bgeo })
    }
}

// ---------------------------------------------------------------------------------------------
// BGEO 0x067CAA11: blend geometry (vertex deltas of a body or face morph)

pub const T_BGEO: u32 = 0x067CAA11;

/// The deltas of one blend: per vertex id, a position and a normal delta.
#[derive(Clone, Debug, Default)]
pub struct BgeoBlend {
    pub age_gender: u32,
    pub region: u32,
    pub deltas: HashMap<u32, ([f32; 3], [f32; 3])>,
}

/// Layout: "BGEO", version 0x300, blend count, LOD count, total vertices, total vectors, blend
/// header size (8), LOD entry size (12), then offsets of the blends, the per-vertex u16s and the
/// vectors. Each blend: age/gender, region, then per LOD (first vertex id, vertex count, vector
/// count). Each vertex's u16: bit 0 = has a position delta, bit 1 = has a normal delta, bits
/// 2..15 = a signed step of a running index (never reset) into the LOD's vectors (shared). A
/// vector is 3 × u16, each a signed value with its sign bit flipped, / 2000.
pub fn parse_bgeo(d: &[u8]) -> R<Vec<BgeoBlend>> {
    let mut r = Reader::new(d);
    if r.fourcc()? != *b"BGEO" {
        return Err(Eof);
    }
    let _version = r.u32()?;
    let nblend = r.u32()? as usize;
    let nlod = r.u32()? as usize;
    let _total_verts = r.u32()?;
    let _total_vecs = r.u32()?;
    let bsize = r.u32()? as usize;
    let lsize = r.u32()? as usize;
    let boff = r.u32()? as usize;
    let voff = r.u32()? as usize;
    let vecoff = r.u32()? as usize;
    if nblend > 64 || nlod > 16 || lsize < 12 {
        return Err(Eof);
    }
    let u16_at = |o: usize| -> R<u16> { d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or(Eof) };
    let comp = |u: u16| ((u ^ 0x8000) as i16) as f32 / 2000.0;
    let vec_at = |i: usize| -> R<[f32; 3]> {
        let o = vecoff + i * 6;
        Ok([comp(u16_at(o)?), comp(u16_at(o + 2)?), comp(u16_at(o + 4)?)])
    };
    let mut out = Vec::with_capacity(nblend);
    // Vertex and vector runs follow each other blend by blend, LOD by LOD.
    let (mut vbase, mut vecbase) = (0usize, 0usize);
    for b in 0..nblend {
        let mut h = Reader::at(d, boff + b * (bsize + nlod * lsize));
        let age_gender = h.u32()?;
        let region = h.u32()?;
        h.pos = boff + b * (bsize + nlod * lsize) + bsize;
        let mut deltas = HashMap::new();
        // (The running index carries on from one LOD to the next.)
        let mut idx: i32 = 0;
        for _ in 0..nlod {
            let start = h.u32()?;
            let nverts = h.u32()? as usize;
            let nvecs = h.u32()? as usize;
            h.skip(lsize - 12)?;
            for k in 0..nverts {
                let v = u16_at(voff + (vbase + k) * 2)?;
                let step = ((v as i16) >> 2) as i32;
                idx += step;
                let flags = v & 3;
                let at = |o: i32| -> R<[f32; 3]> {
                    if o < 0 || o as usize >= nvecs {
                        return Err(Eof);
                    }
                    vec_at(vecbase + o as usize)
                };
                let pos = if flags & 1 != 0 { at(idx)? } else { [0.0; 3] };
                let nrm = if flags & 2 != 0 { at(idx + (flags & 1) as i32)? } else { [0.0; 3] };
                if flags != 0 {
                    deltas.insert(start + k as u32, (pos, nrm));
                }
            }
            vbase += nverts;
            vecbase += nvecs;
        }
        out.push(BgeoBlend { age_gender, region, deltas });
    }
    Ok(out)
}

/// Sim outfits (SIMO): a premade set of clothes, like a career's uniform.
pub const T_OUTFIT: u32 = 0x025ED6F4;

/// One part of an outfit: the CAS part and the design (colours and patterns) it's worn in.
#[derive(Clone, Debug)]
pub struct OutfitPart {
    pub casp: ResourceKey,
    pub body_type: u32,
    /// The part's preset XML (a `CasRgbMask` complate and its patterns).
    pub preset: String,
}

/// A Sim outfit: its age and gender, and its parts.
#[derive(Clone, Debug, Default)]
pub struct SimOutfit {
    pub age: u32,
    pub gender: u32,
    pub parts: Vec<OutfitPart>,
}

impl SimOutfit {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let version = r.u32()?;
        let tgi_pos = 8 + r.u32()? as usize;
        let mut keys = Vec::new();
        {
            // (Counted with a short from version 0x15.)
            let mut t = Reader::at(d, tgi_pos);
            let n = if version >= 0x15 { t.u16()? as usize } else { t.u8()? as usize };
            for _ in 0..n {
                let i = t.u64()?;
                let g = t.u32()?;
                let ty = t.u32()?;
                keys.push(ResourceKey::new(ty, g, i));
            }
        }
        // The parts' presets, in the order of the parts.
        let n = r.u32()? as usize;
        let mut presets = Vec::new();
        for _ in 0..n.min(32) {
            r.u8()?;
            let chars = r.u32()? as usize;
            let b = r.bytes(chars * 2)?;
            presets.push(String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()));
        }
        let start = r.pos;
        let mid = d.get(start..tgi_pos).ok_or(Eof)?;
        let u = |o: usize| mid.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
        let (age, gender) = (u(24), u(28));
        // Then the body sliders, skin and hair, and the part list (each part: its CASP's index,
        // its body type, and its textures' indices), then a zero and the face sliders (each an
        // index and an amount). The header before the list differs by version, so the list is
        // found by where it fits: one part per preset, each naming a CAS part, ending just so.
        // Base-game outfits index with bytes, later ones with shorts.
        let parse_at = |at: usize, wide: bool| -> Option<Vec<OutfitPart>> {
            let ix = |o: usize| -> Option<usize> { if wide { mid.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) } else { mid.get(o).map(|b| *b as usize) } };
            let w = if wide { 2 } else { 1 };
            if *mid.get(at)? as usize != presets.len() || presets.is_empty() {
                return None;
            }
            let mut o = at + 1;
            let mut parts = Vec::new();
            for p in &presets {
                let casp = *keys.get(ix(o)?)?;
                if casp.t != s3pkg::types::CASP {
                    return None;
                }
                let body_type = u32::from_le_bytes(mid.get(o + w..o + w + 4)?.try_into().ok()?);
                let m = *mid.get(o + w + 4)? as usize;
                o += w + 5 + m * 2 * w;
                parts.push(OutfitPart { casp, body_type, preset: p.clone() });
            }
            let faces = *mid.get(o + 1)? as usize;
            (mid[o] == 0 && o + 2 + faces * (w + 4) == mid.len()).then_some(parts)
        };
        let parts = (32..mid.len()).rev().find_map(|at| parse_at(at, false).or_else(|| parse_at(at, true))).unwrap_or_default();
        Ok(Self { age, gender, parts })
    }
}
