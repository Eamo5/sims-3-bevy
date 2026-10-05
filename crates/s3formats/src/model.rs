//! MODL / MLOD meshes with VRTF, VBUF, IBUF and MATD materials.

use std::collections::HashMap;

use crate::rcol::{ChunkRef, Rcol};
use crate::util::{Eof, R, Reader};
use s3pkg::{PackageSet, ResourceKey, types};

pub const P_DIFFUSE_MAP: u32 = 0x6CC0FD85;
pub const P_NORMAL_MAP: u32 = 0x6E56548A;
pub const P_SPECULAR_MAP: u32 = 0xAD528A60;
pub const P_ALPHA_MAP: u32 = 0xC3FAAC4F;
pub const P_MULTIPLY_MAP: u32 = 0xCD869A45;
pub const P_UV_SCALES: u32 = 0x420520E9;
pub const P_DIFFUSE: u32 = 0x637DAA05;
pub const P_ALPHA_THRESHOLD: u32 = 0xE77A2B60;
pub const P_TRANSPARENCY: u32 = 0x05D22FD3;

pub const SHADER_DROP_SHADOW: u32 = 0xC09C7582;
pub const SHADER_SHADOW_MAP: u32 = 0x21FE207D;
pub const SHADER_PHONG_ALPHA: u32 = 0xFC5FC212;
pub const SHADER_FOLIAGE: u32 = 0x4549E22E;
pub const SHADER_GLASS_OBJECTS: u32 = 0x492ECA7C;
pub const SHADER_GLASS_TRANSLUCENT: u32 = 0x849CF021;
pub const SHADER_GLASS_FENCES: u32 = 0x52986C62;
pub const SHADER_GLASS_PORTALS: u32 = 0x81DD204D;
pub const SHADER_GLASS_RABBIT: u32 = 0x265FFAA1;
pub const SHADER_SIM_GLASS: u32 = 0x5EDA9CDE;
pub const SHADER_SIM_HAIR: u32 = 0x84FD7152;
pub const SHADER_SIM_EYELASHES: u32 = 0x9D9DA161;
pub const SHADER_LOT_IMPOSTER: u32 = 0x68601DE3;
pub const SHADER_ADDITIVE: u32 = 0x5AF16731;

#[derive(Clone, Debug)]
pub enum ParamValue {
    Float(Vec<f32>),
    Int(i32),
    /// Texture as a resolved resource key.
    Texture(ResourceKey),
    Raw,
}

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub shader: u32,
    pub params: HashMap<u32, ParamValue>,
}

impl Material {
    pub fn texture(&self, name: u32) -> Option<ResourceKey> {
        match self.params.get(&name) {
            Some(ParamValue::Texture(k)) => Some(*k),
            _ => None,
        }
    }

    pub fn floats(&self, name: u32) -> Option<&[f32]> {
        match self.params.get(&name) {
            Some(ParamValue::Float(v)) => Some(v),
            _ => None,
        }
    }

    pub fn is_shadow(&self) -> bool {
        matches!(self.shader, SHADER_DROP_SHADOW | SHADER_SHADOW_MAP)
    }

    pub fn is_alpha_blended(&self) -> bool {
        matches!(
            self.shader,
            SHADER_PHONG_ALPHA
                | SHADER_GLASS_OBJECTS
                | SHADER_GLASS_TRANSLUCENT
                | SHADER_GLASS_FENCES
                | SHADER_GLASS_PORTALS
                | SHADER_GLASS_RABBIT
                | SHADER_SIM_GLASS
                | SHADER_ADDITIVE
        )
    }

    pub fn is_alpha_tested(&self) -> bool {
        matches!(self.shader, SHADER_FOLIAGE | SHADER_SIM_HAIR | SHADER_SIM_EYELASHES)
            || self.params.contains_key(&P_ALPHA_THRESHOLD)
    }
}

/// Parses an MTNF/MTRL block. `tex` maps a raw texture reference word to a key.
pub fn parse_mtnf(d: &[u8], start: usize, tex: &dyn Fn(u32) -> Option<ResourceKey>) -> R<HashMap<u32, ParamValue>> {
    let mut r = Reader::at(d, start);
    let tag = r.fourcc()?;
    let _unk = r.u32()?;
    if &tag == b"MTNF" {
        let _size = r.u32()?;
    } else {
        let _a = r.u16()?;
        let _b = r.u16()?;
    }
    let n = r.i32()?.max(0) as usize;
    if n > 1024 {
        return Err(Eof);
    }
    let mut params = HashMap::new();
    for _ in 0..n {
        let name = r.u32()?;
        let dtype = r.u32()?;
        let words = r.i32()?.max(0) as usize;
        let offset = r.u32()? as usize;
        let mut v = Reader::at(d, start + offset);
        let value = match (dtype, words) {
            (1, 1..=16) => {
                let mut f = Vec::with_capacity(words);
                for _ in 0..words {
                    f.push(v.f32()?);
                }
                ParamValue::Float(f)
            }
            (2, _) => ParamValue::Int(v.i32()?),
            (4, 4) => match tex(v.u32()?) {
                Some(k) => ParamValue::Texture(k),
                None => ParamValue::Raw,
            },
            (4, 5) => {
                let i = v.u64()?;
                let t = v.u32()?;
                let g = v.u32()?;
                ParamValue::Texture(ResourceKey::new(t, g, i))
            }
            _ => ParamValue::Raw,
        };
        params.insert(name, value);
    }
    Ok(params)
}

fn parse_matd(rcol: &Rcol, d: &[u8]) -> R<Material> {
    let mut r = Reader::new(d);
    let tag = r.fourcc()?;
    if &tag != b"MATD" {
        return Err(Eof);
    }
    let version = r.u32()?;
    let _name = r.u32()?;
    let shader = r.u32()?;
    let _len = r.u32()?;
    if version >= 0x103 {
        let _video = r.u32()?;
        let _painting = r.u32()?;
    }
    let params = parse_mtnf(d, r.pos, &|raw| rcol.external_key(raw))?;
    Ok(Material { shader, params })
}

/// A material stored as a resource of its own (an RCOL holding one MATD).
pub fn load_matd_resource(pkgs: &PackageSet, key: &ResourceKey) -> Option<Material> {
    let data = pkgs.read(key).or_else(|| pkgs.read_ti(key.t, key.i))?;
    let rcol = Rcol::parse(&data).ok()?;
    let i = rcol.find_tag(b"MATD")?;
    parse_matd(&rcol, rcol.chunk_data(i)?).ok()
}

/// Resolves a material reference (MATD, or MTST -> default MATD).
fn resolve_material(rcol: &Rcol, raw: u32, depth: u32) -> Option<Material> {
    if depth > 4 {
        return None;
    }
    let (_, d) = rcol.resolve(raw)?;
    match &d[0..4] {
        b"MATD" => parse_matd(rcol, d).ok(),
        b"MTST" => {
            let mut r = Reader::new(d);
            r.skip(12).ok()?;
            let default = r.u32().ok()?;
            resolve_material(rcol, default, depth + 1)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct VElem {
    usage: u8,
    usage_index: u8,
    format: u8,
    offset: u8,
}

struct Vrtf {
    stride: usize,
    elems: Vec<VElem>,
}

fn parse_vrtf(d: &[u8]) -> R<Vrtf> {
    let mut r = Reader::new(d);
    let _tag = r.fourcc()?;
    let _ver = r.u32()?;
    let stride = r.i32()? as usize;
    let n = r.i32()?.max(0) as usize;
    let extended = r.u32()? != 0;
    let mut elems = Vec::with_capacity(n);
    for _ in 0..n {
        let e = if extended {
            VElem {
                usage: r.u32()? as u8,
                usage_index: r.u32()? as u8,
                format: r.u32()? as u8,
                offset: r.u32()? as u8,
            }
        } else {
            VElem { usage: r.u8()?, usage_index: r.u8()?, format: r.u8()?, offset: r.u8()? }
        };
        elems.push(e);
    }
    Ok(Vrtf { stride, elems })
}

fn decode_ibuf(d: &[u8]) -> R<Vec<u32>> {
    let mut r = Reader::new(d);
    let _tag = r.fourcc()?;
    let _ver = r.u32()?;
    let flags = r.u32()?;
    let _dl = r.u32()?;
    let delta = flags & 1 != 0;
    let wide = flags & 2 != 0;
    let mut out = Vec::new();
    let mut last: i64 = 0;
    if wide {
        while r.remaining() >= 4 {
            let v = r.i32()? as i64;
            let cur = if delta { last + v } else { v };
            out.push(cur as u32);
            last = cur;
        }
    } else {
        while r.remaining() >= 2 {
            let v = r.i16()? as i64;
            let cur = if delta { (last + v) & 0xFFFF } else { v & 0xFFFF };
            out.push(cur as u32);
            last = cur;
        }
    }
    Ok(out)
}

/// One drawable mesh group with decoded vertex attributes (game coordinates).
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub name_hash: u32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Second texture coordinate set: the zw half of a float4 texcoord0, else texcoord1 (empty when absent).
    pub uvs1: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub material: Material,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

fn color_ubyte4_vec(e: &[u8]) -> [f32; 3] {
    let f = |c: u8| if c == 0 { -1.0 } else { (c as f32 + 1.0) / 128.0 - 1.0 };
    [f(e[2]), f(e[1]), f(e[0])]
}

/// Decodes every mesh in an MLOD chunk of `rcol`.
pub fn decode_mlod(rcol: &Rcol, mlod: &[u8]) -> R<Vec<MeshData>> {
    let mut r = Reader::new(mlod);
    let tag = r.fourcc()?;
    if &tag != b"MLOD" {
        return Err(Eof);
    }
    let version = r.u32()?;
    let count = r.i32()?.max(0) as usize;
    let mut ibuf_cache: HashMap<usize, Vec<u32>> = HashMap::new();
    let mut out = Vec::new();
    for _ in 0..count {
        let size = r.u32()? as usize;
        let start = r.pos;
        let name_hash = r.u32()?;
        let material_ref = r.u32()?;
        let vrtf_ref = r.u32()?;
        let vbuf_ref = r.u32()?;
        let ibuf_ref = r.u32()?;
        let prim = r.u32()? & 0xFF;
        let stream_offset = r.u32()? as usize;
        let _start_vertex = r.i32()?;
        let start_index = r.i32()?.max(0) as usize;
        let _min_vertex = r.i32()?;
        let vertex_count = r.i32()?.max(0) as usize;
        let prim_count = r.i32()?.max(0) as usize;
        let bmin = r.vec3()?;
        let bmax = r.vec3()?;
        let _ = version;
        r.pos = start + size;

        if prim != 3 || vrtf_ref == 0 {
            continue;
        }
        let material = resolve_material(rcol, material_ref, 0).unwrap_or_default();
        if material.is_shadow() {
            continue;
        }
        let Some((_, vrtf_d)) = rcol.resolve(vrtf_ref) else { continue };
        let Some((vbuf_i, vbuf_d)) = rcol.resolve(vbuf_ref) else { continue };
        let Some((ibuf_i, ibuf_d)) = rcol.resolve(ibuf_ref) else { continue };
        if rcol.chunks[vbuf_i].key.t == 0x0229684B {
            continue;
        }
        let vrtf = parse_vrtf(vrtf_d)?;
        let vdata = vbuf_d.get(16..).ok_or(Eof)?;
        if !ibuf_cache.contains_key(&ibuf_i) {
            ibuf_cache.insert(ibuf_i, decode_ibuf(ibuf_d)?);
        }
        let all_idx = &ibuf_cache[&ibuf_i];
        let uv_scales = material.floats(P_UV_SCALES).map(|v| v.to_vec()).unwrap_or_default();

        let mut m = MeshData {
            name_hash,
            bounds_min: bmin,
            bounds_max: bmax,
            ..Default::default()
        };
        m.positions.reserve(vertex_count);
        let mut has_normal = false;
        let mut has_uv = false;
        for v in 0..vertex_count {
            let base = stream_offset + v * vrtf.stride;
            let Some(vert) = vdata.get(base..base + vrtf.stride) else { return Err(Eof) };
            let mut pos = [0.0f32; 3];
            let mut nrm = [0.0, 1.0, 0.0];
            let mut uv = [0.0f32; 2];
            let mut uv1 = None;
            let mut uv0_zw = None;
            for e in &vrtf.elems {
                let o = e.offset as usize;
                let b = &vert[o.min(vert.len())..];
                let f32_at = |i: usize| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
                let i16_at = |i: usize| i16::from_le_bytes(b[i * 2..i * 2 + 2].try_into().unwrap());
                let u16_at = |i: usize| u16::from_le_bytes(b[i * 2..i * 2 + 2].try_into().unwrap());
                match e.usage {
                    0 => {
                        pos = match e.format {
                            2 | 3 if b.len() >= 12 => [f32_at(0), f32_at(1), f32_at(2)],
                            7 | 12 if b.len() >= 8 => {
                                let mut s = u16_at(3) as f32;
                                if s == 0.0 {
                                    s = if e.format == 7 { 32767.0 } else { 512.0 };
                                }
                                [i16_at(0) as f32 / s, i16_at(1) as f32 / s, i16_at(2) as f32 / s]
                            }
                            _ => pos,
                        }
                    }
                    1 if e.format == 5 && b.len() >= 4 => {
                        nrm = color_ubyte4_vec(b);
                        has_normal = true;
                    }
                    1 if (e.format == 2 || e.format == 3) && b.len() >= 12 => {
                        nrm = [f32_at(0), f32_at(1), f32_at(2)];
                        has_normal = true;
                    }
                    2 if e.usage_index <= 1 => {
                        let scale = uv_scales
                            .get(e.usage_index as usize)
                            .copied()
                            .filter(|s| *s != 0.0)
                            .or_else(|| uv_scales.first().copied())
                            .unwrap_or(1.0 / 32767.0);
                        if e.format == 3 && e.usage_index == 0 && b.len() >= 16 {
                            // Float4 texcoord: a second, differently-scaled UV pair (road overlays).
                            uv0_zw = Some([f32_at(2), f32_at(3)]);
                        }
                        let t = match e.format {
                            1 | 3 if b.len() >= 8 => [f32_at(0), f32_at(1)],
                            6 if b.len() >= 4 => [i16_at(0) as f32 * scale, i16_at(1) as f32 * scale],
                            7 if b.len() >= 4 => [i16_at(0) as f32 / 32767.0, i16_at(1) as f32 / 32767.0],
                            _ => [0.0; 2],
                        };
                        if e.usage_index == 0 {
                            uv = t;
                            has_uv = true;
                        } else {
                            uv1 = Some(t);
                        }
                    }
                    _ => {}
                }
            }
            let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
            if l > 1e-6 {
                nrm = [nrm[0] / l, nrm[1] / l, nrm[2] / l];
            }
            m.positions.push(pos);
            m.normals.push(nrm);
            m.uvs.push(uv);
            if let Some(t) = uv0_zw.or(uv1) {
                m.uvs1.push(t);
            }
        }
        let _ = (has_normal, has_uv);
        let end = start_index + prim_count * 3;
        let Some(idx) = all_idx.get(start_index..end) else { continue };
        m.indices = idx.iter().map(|&i| i.min(vertex_count.saturating_sub(1) as u32)).collect();
        m.material = material;
        out.push(m);
    }
    Ok(out)
}

/// LOD entry of a MODL.
#[derive(Clone, Copy, Debug)]
pub struct LodEntry {
    pub reference: u32,
    pub lod_id: u32,
}

pub fn parse_modl(d: &[u8]) -> R<(Vec<LodEntry>, [f32; 3], [f32; 3])> {
    let mut r = Reader::new(d);
    let tag = r.fourcc()?;
    if &tag != b"MODL" {
        return Err(Eof);
    }
    let version = r.u32()?;
    let n = r.i32()?.max(0) as usize;
    let bmin = r.vec3()?;
    let bmax = r.vec3()?;
    if version >= 258 {
        let extra = r.i32()?.max(0) as usize;
        r.skip(extra * 24)?;
        r.skip(8)?;
    }
    let mut lods = Vec::with_capacity(n);
    for _ in 0..n {
        let reference = r.u32()?;
        let _flags = r.u32()?;
        let lod_id = r.u32()?;
        let _minz = r.f32()?;
        let _maxz = r.f32()?;
        lods.push(LodEntry { reference, lod_id });
    }
    Ok((lods, bmin, bmax))
}

/// Loads the highest-detail meshes of a MODL resource (following external MLODs).
pub fn load_model(pkgs: &PackageSet, modl_key: &ResourceKey) -> Option<Vec<MeshData>> {
    let data = pkgs.read(modl_key)?;
    let rcol = Rcol::parse(&data).ok()?;
    let mi = rcol.find_tag(b"MODL")?;
    let (lods, _, _) = parse_modl(rcol.chunk_data(mi)?).ok()?;
    let lod = lods
        .iter()
        .filter(|l| l.lod_id & 0x10000 == 0)
        .min_by_key(|l| l.lod_id)?;
    match rcol.decode_ref(lod.reference) {
        ChunkRef::Chunk(i) => decode_mlod(&rcol, rcol.chunk_data(i)?).ok(),
        ChunkRef::External(i) => {
            let key = *rcol.external.get(i)?;
            load_mlod_resource(pkgs, &key)
        }
        ChunkRef::Null => None,
    }
}

pub fn load_mlod_resource(pkgs: &PackageSet, key: &ResourceKey) -> Option<Vec<MeshData>> {
    let data = pkgs.read(key).or_else(|| pkgs.read_ti(key.t, key.i))?;
    let rcol = Rcol::parse(&data).ok()?;
    let mi = rcol.find_tag(b"MLOD")?;
    decode_mlod(&rcol, rcol.chunk_data(mi)?).ok()
}

/// Finds the MODL keys referenced by a VPXY resource.
pub fn vpxy_models(d: &[u8]) -> Vec<ResourceKey> {
    let keys = vpxy_keys(d);
    let mut models: Vec<ResourceKey> = keys.iter().filter(|k| k.t == types::MODL).copied().collect();
    if models.is_empty() {
        models = keys.iter().filter(|k| k.t == types::MLOD).copied().collect();
    }
    models
}

/// Every resource a VPXY refers to (models, lights, materials...).
pub fn vpxy_keys(d: &[u8]) -> Vec<ResourceKey> {
    let Ok(rcol) = Rcol::parse(d) else { return Vec::new() };
    let Some(i) = rcol.find_tag(b"VPXY") else { return Vec::new() };
    let Some(c) = rcol.chunk_data(i) else { return Vec::new() };
    tgi_table_at(c, 8).unwrap_or_default()
}

/// Reads a relative TGI table whose offset field is at `off_pos` ("TGI" order: type, group, instance).
pub fn tgi_table_at(d: &[u8], off_pos: usize) -> R<Vec<ResourceKey>> {
    let mut r = Reader::at(d, off_pos);
    let rel = r.u32()? as usize;
    let mut t = Reader::at(d, off_pos + 4 + rel);
    let n = t.i32()?.max(0) as usize;
    if n > 4096 {
        return Err(Eof);
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let ty = t.u32()?;
        let g = t.u32()?;
        let i = t.u64()?;
        out.push(ResourceKey::new(ty, g, i));
    }
    Ok(out)
}
