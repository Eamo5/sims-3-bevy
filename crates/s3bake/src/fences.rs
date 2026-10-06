//! Fences and railings. A lot keeps its fence posts (`0x913381F2`: `u32 version, u32 count`,
//! then `u32 level, f32 x, f32 z, u16 REFS index` of the fence's CFEN); the runs between them
//! are edges of the lot's room graph. A CFEN (`0x0418FE2A`) names its pieces' models (VPXYs):
//! a straight run (0..1 m along +X), a diagonal one (0..1.414 m) and, for some, a post.

use std::collections::{HashMap, HashSet};

use s3formats::lot::LotBuildData;
use s3formats::world::LotInfo;
use s3pkg::{Package, PackageSet, ResourceKey};

use crate::types::{FenceBaked, Key, key_of};

const T_FENCES: u32 = 0x913381F2;
const T_VPXY: u32 = 0x736884F1;

/// A fence's piece models: straight, diagonal, post.
#[derive(Clone, Copy, Default)]
pub struct Pieces {
    pub straight: Option<Key>,
    pub diagonal: Option<Key>,
    pub post: Option<Key>,
}

/// A CFEN's catalogue entry: its name's string key, its price, and whether the catalogue
/// shows it. (Version 10: a header, no materials, then the catalogue's common block.)
pub fn catalog_entry(d: &[u8]) -> Option<(u64, f32, bool)> {
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    if u32_at(12)? != 0 {
        return None;
    }
    let name_guid = u64::from_le_bytes(d.get(20..28)?.try_into().ok()?);
    let mut o = 36;
    // The name and description keys: 7-bit-length UTF-16 strings.
    for _ in 0..2 {
        let mut n = 0usize;
        let mut shift = 0;
        loop {
            let b = *d.get(o)?;
            o += 1;
            n |= ((b & 0x7F) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        o += n;
    }
    let price = f32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?);
    let status = *d.get(o + 12)?;
    Some((name_guid, price, status & 1 != 0))
}

pub fn pieces(pkgs: &PackageSet, cfen: &ResourceKey) -> Pieces {
    let Some(c) = pkgs.read(cfen).or_else(|| pkgs.read_ti(cfen.t, cfen.i)) else { return Pieces::default() };
    let mut out = Pieces::default();
    let mut o = 0;
    while o + 16 <= c.len() {
        if c[o..o + 4] != T_VPXY.to_le_bytes() {
            o += 1;
            continue;
        }
        let g = u32::from_le_bytes(c[o + 4..o + 8].try_into().unwrap());
        let i = u64::from_le_bytes(c[o + 8..o + 16].try_into().unwrap());
        o += 16;
        let v = ResourceKey::new(T_VPXY, g, i);
        let Some(m) = pkgs.read(&v).or_else(|| pkgs.read_ti(v.t, v.i)).and_then(|d| s3formats::model::vpxy_models(&d).into_iter().next()) else { continue };
        // Which piece: by how far it runs along +X.
        let b = crate::bake::bake_model(pkgs, &m);
        let reach = b.parts.iter().map(|p| p.bmax[0]).fold(0.0f32, f32::max);
        let slot = if reach > 1.2 {
            &mut out.diagonal
        } else if reach > 0.8 {
            &mut out.straight
        } else {
            &mut out.post
        };
        if slot.is_none() {
            *slot = Some(key_of(&m));
        }
    }
    out
}

/// A lot's fence runs and posts.
pub fn bake_fences(pkg: &Package, pkgs: &PackageSet, lot: &LotInfo, cache: &mut HashMap<ResourceKey, Pieces>) -> Vec<FenceBaked> {
    let read = |t: u32| pkg.find(&ResourceKey::new(t, 0, lot.id)).and_then(|e| pkg.read(e).ok());
    let Some(d) = read(T_FENCES) else { return Vec::new() };
    let refs = read(0x05ED1226).and_then(|r| s3formats::objn::parse_refs(&r).ok()).unwrap_or_default();
    let n = d.get(4..8).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let mut posts: HashMap<(i32, i32, u32), u16> = HashMap::new();
    for k in 0..n.min(100_000) {
        let o = 8 + k * 14;
        let Some(b) = d.get(o..o + 14) else { break };
        let level = u32::from_le_bytes(b[0..4].try_into().unwrap());
        let x = f32::from_le_bytes(b[4..8].try_into().unwrap());
        let z = f32::from_le_bytes(b[8..12].try_into().unwrap());
        let ri = u16::from_le_bytes(b[12..14].try_into().unwrap());
        posts.insert((x.round() as i32, z.round() as i32, level), ri);
    }
    if posts.is_empty() {
        return Vec::new();
    }
    let mut style = |ri: u16| -> Pieces {
        let Some(k) = refs.get(&ri).copied() else { return Pieces::default() };
        *cache.entry(k).or_insert_with(|| pieces(pkgs, &k))
    };
    // Runs: the room graph's edges in unit steps, wherever both ends are posts.
    let rooms = LotBuildData::load(pkg, lot.id).map(|b| b.rooms).unwrap_or_default();
    let mut out = Vec::new();
    let mut done = HashSet::new();
    for (a, b, level, _) in rooms.segments() {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let steps = dx.abs().max(dz.abs()).round() as i32;
        if steps == 0 || (dx.abs() > 0.01 && dz.abs() > 0.01 && (dx.abs() - dz.abs()).abs() > 0.01) {
            continue;
        }
        let (sx, sz) = ((dx / steps as f32).round() as i32, (dz / steps as f32).round() as i32);
        let (ax, az) = (a[0].round() as i32, a[1].round() as i32);
        for s in 0..steps {
            let p = (ax + sx * s, az + sz * s);
            let q = (p.0 + sx, p.1 + sz);
            let (Some(&ri), Some(_)) = (posts.get(&(p.0, p.1, level)), posts.get(&(q.0, q.1, level))) else { continue };
            let key = if p < q { (p, q, level) } else { (q, p, level) };
            if !done.insert(key) {
                continue;
            }
            let pc = style(ri);
            let model = if sx != 0 && sz != 0 { pc.diagonal } else { pc.straight };
            if let Some(model) = model {
                out.push(FenceBaked { a: [p.0 as f32, p.1 as f32], b: [q.0 as f32, q.1 as f32], level: level as u8, model });
            }
        }
    }
    // Posts, for fences that have them.
    for (&(x, z, level), &ri) in &posts {
        if let Some(model) = style(ri).post {
            out.push(FenceBaked { a: [x as f32, z as f32], b: [x as f32, z as f32], level: level as u8, model });
        }
    }
    out
}
