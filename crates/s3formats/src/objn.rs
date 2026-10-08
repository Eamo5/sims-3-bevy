//! World object placement (OBJN 0x4D1A5589) and its reference tables (REFS 0x05ED1226).

use std::collections::HashMap;

use crate::util::{Eof, R, Reader};
use s3pkg::ResourceKey;

pub const T_OBJN: u32 = 0x4D1A5589;
pub const T_REFS: u32 = 0x05ED1226;
pub const REFS_GROUP_OBJN: u32 = 0x00F0B54D;
pub const T_SPEEDTREE: u32 = 0x00B552EA;

pub fn parse_refs(d: &[u8]) -> R<HashMap<u16, ResourceKey>> {
    let mut r = Reader::new(d);
    let ver = r.u16()?;
    let wide = if ver >= 3 { r.u8()? != 0 } else { false };
    let n = r.u32()? as usize;
    let mut out = HashMap::with_capacity(n);
    for _ in 0..n {
        let t = r.u32()?;
        let g = r.u32()?;
        let i = r.u64()?;
        let idx = if wide { r.u32()? as u16 } else { r.u16()? };
        out.insert(idx, ResourceKey::new(t, g, i));
    }
    Ok(out)
}

const C_LOCATION: u32 = 0x461922C8;
const C_TRANSFORM: u32 = 0x54CB7EBB;
const C_MODEL: u32 = 0x2954E734;
const C_ANIMATION: u32 = 0xEE17C6AD;
const C_SCRIPT: u32 = 0x23177498;
const C_PHYSICS: u32 = 0x1A8FEB14;
const C_TREE: u32 = 0xC602CD31;
const C_EFFECT: u32 = 0x80D91E9E;
const C_SIM: u32 = 0x22706EFA;
const C_STEERING: u32 = 0x61BD317C;
const C_SACS: u32 = 0x3AE9A8E7;
const C_SLOT: u32 = 0x2EF1E401;
const C_LIGHTING: u32 = 0xDA6C50FD;
const C_VISUALSTATE: u32 = 0x50B3D17C;
const C_FOOTPRINT: u32 = 0xC807312A;
const C_AUDIO: u32 = 0x3FC40859;

/// One instance of a SpeedTree: 4x4 row-major matrix (row 3 = position) and scale.
#[derive(Clone, Debug)]
pub struct TreeInstance {
    pub matrix: [f32; 16],
    pub scale: f32,
}

#[derive(Clone, Debug, Default)]
pub struct PlacedObject {
    pub guid: u64,
    pub catalog: Option<ResourceKey>,
    pub vpxy: Option<ResourceKey>,
    pub model: Option<ResourceKey>,
    pub position: Option<[f32; 3]>,
    pub rotation: [f32; 4],
    pub parent: u64,
    pub script: Option<String>,
    pub speedtree: Option<ResourceKey>,
    pub trees: Vec<TreeInstance>,
    /// The design it's in (its colours and patterns, as chosen for this one): a complate and
    /// the resources it refers to.
    pub design: Option<(crate::catalog::Complate, Vec<ResourceKey>)>,
}

fn skip_anim(r: &mut Reader) -> R<()> {
    let _one = r.u32()?;
    let cnt = r.u32()?;
    for _ in 0..cnt {
        let _prio = r.u32()?;
        loop {
            if r.u32()? == 0 {
                break;
            }
            let mut t = r.u32()?;
            if t == 1 {
                t = r.u32()?;
            }
            if t == 0 {
                r.skip(16)?;
                continue;
            }
            r.u32()?;
            r.u32()?;
            r.u8()?;
            match t {
                0x060CAEEF => r.skip(24)?,
                0x0664ED68 => {
                    let c1 = r.u32()? as usize;
                    r.u32()?;
                    let c2 = r.u32()? as usize;
                    r.skip(c1 * c2 * 4)?;
                    r.skip(16)?;
                }
                0x0664FB68 => r.skip(20)?,
                0x067C5CAB => r.skip(16)?,
                0x0681C688 | 0x073B9D8C | 0x07A11BA8 | 0x34F49F94 => r.skip(4)?,
                0xB489FC2C => {
                    r.u32()?;
                    let flag = r.u32()?;
                    r.skip(32)?;
                    r.u32()?;
                    let c2 = r.u32()?;
                    r.i32()?;
                    if c2 > 0 {
                        loop {
                            r.u64()?;
                            while r.u32()? != 0 {}
                            let a = r.u32()?;
                            let sl = r.u32()?;
                            if a == 0 && sl == 0 {
                                break;
                            }
                        }
                    }
                    r.u64()?;
                    if flag != 0 {
                        let pc = r.u32()? as usize;
                        r.skip(pc * 28)?;
                    }
                    r.skip(12 + 12 + 3 + 16)?;
                }
                _ => return Err(Eof),
            }
        }
    }
    Ok(())
}

/// Parses the objects of an OBJN resource; `refs` is its matching REFS table.
pub fn parse_objn(d: &[u8], refs: &HashMap<u16, ResourceKey>) -> R<Vec<PlacedObject>> {
    let mut r = Reader::new(d);
    let _ver = r.u32()?;
    r.u32()?;
    r.u32()?;
    let n = r.u32()? as usize;
    r.skip(n * 12)?;
    r.u32()?;
    r.u32()?;
    let n2 = r.u32()? as usize;
    r.skip(n2 * 12)?;
    r.skip(16)?;
    r.u32()?;
    r.u32()?;
    r.i32()?;
    let _len = r.u32()?;
    let count = r.u32()? as usize;
    let mut out = Vec::with_capacity(count);
    // (Worlds saved by later versions of the game have a byte more at the end of the model's
    // visual state, and a few after the object: whichever way the world has it, found as the way
    // an object ends where the next begins (or just short of it), and kept to.)
    let mut tail = 3;
    for i in 0..count {
        let start = r.pos;
        let fits = |t: usize| {
            let mut rr = Reader::at(d, start);
            let o = parse_object(&mut rr, refs, t).ok()?;
            if i + 1 >= count {
                return Some((o, rr.pos));
            }
            let next = (rr.pos..=rr.pos + 8).find(|&p| header_ok(d, p))?;
            Some((o, next))
        };
        let other = if tail == 3 { 4 } else { 3 };
        if let Some((o, end)) = fits(tail).or_else(|| fits(other).inspect(|_| tail = other)) {
            out.push(o);
            r.pos = end;
            continue;
        }
        // (Otherwise what can be read of it, and on from the next object's header: the first
        // after this one's own.)
        if let Ok(o) = parse_object(&mut Reader::at(d, start), refs, tail) {
            out.push(o);
        }
        if i + 1 >= count {
            break;
        }
        match scan_header(d, start + 16, 1 << 18) {
            Some(p) => r.pos = p,
            None => break,
        }
    }
    Ok(out)
}

const KNOWN: [u32; 16] = [
    C_LOCATION, C_TRANSFORM, C_MODEL, C_ANIMATION, C_SCRIPT, C_PHYSICS, C_TREE, C_EFFECT, C_SIM, C_STEERING, C_SACS,
    C_SLOT, C_LIGHTING, C_VISUALSTATE, C_FOOTPRINT, C_AUDIO,
];

/// Whether an object record plausibly starts at `p` (guid, small version, known components).
fn header_ok(d: &[u8], p: usize) -> bool {
    let u = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let (Some(ver), Some(n)) = (u(p + 8), u(p + 12)) else { return false };
    if !(8..=16).contains(&ver) || !(1..=24).contains(&n) {
        return false;
    }
    (0..n as usize).all(|k| u(p + 16 + k * 4).is_some_and(|h| KNOWN.contains(&h)))
}

fn scan_header(d: &[u8], from: usize, window: usize) -> Option<usize> {
    (from..(from + window).min(d.len().saturating_sub(16))).find(|&p| header_ok(d, p))
}

fn parse_object(r: &mut Reader, refs: &HashMap<u16, ResourceKey>, tail: usize) -> R<PlacedObject> {
    let resolve = |i: u16| refs.get(&i).copied().filter(|k| k.t != 0 || k.i != 0);
    let mut o = PlacedObject { rotation: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
    o.guid = r.u64()?;
    r.u32()?;
    let nc = r.u32()? as usize;
    if nc > 32 {
        return Err(Eof);
    }
    let mut comps = Vec::with_capacity(nc);
    for _ in 0..nc {
        let c = r.u32()?;
        if !KNOWN.contains(&c) {
            return Err(Eof);
        }
        comps.push(c);
    }
    let has = |c: u32| comps.contains(&c);
    o.catalog = resolve(r.u16()?);
    r.u16()?;
    o.vpxy = resolve(r.u16()?);
    if has(C_LOCATION) {
        o.position = Some(r.vec3()?);
    }
    if has(C_TRANSFORM) {
        o.rotation = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
        o.parent = r.u64()?;
        if o.parent != 0 {
            r.u32()?;
            r.u16()?;
        }
    }
    if has(C_MODEL) {
        o.model = resolve(r.u16()?);
        r.u16()?;
        r.u16()?;
        let c4 = r.u32()?;
        if c4 > 64 {
            return Err(Eof);
        }
        for _ in 0..c4 {
            r.u16()?;
            r.u32()?;
            let ln = r.u32()? as usize;
            r.skip(ln + 1)?;
        }
        let cpl = r.u16()?;
        if resolve(cpl).is_some() {
            r.u32()?;
            r.u32()?;
            let off = r.u32()? as usize;
            if off != 0 {
                // The design: a material block (u16, the TGI list's offset and size, the
                // complate), then the REFS indices its resources resolve through.
                let start = r.pos;
                let mut m = Reader::at(r.data, start);
                let complate = (|| -> R<crate::catalog::Complate> {
                    m.u16()?;
                    m.u32()?;
                    m.u32()?;
                    crate::catalog::read_complate(&mut m)
                })();
                r.skip(off)?;
                let c6 = r.u8()? as usize;
                let idx = (0..c6).map(|_| r.u16()).collect::<R<Vec<u16>>>()?;
                if let Ok(c) = complate {
                    let keys = idx.iter().map(|i| refs.get(i).copied().unwrap_or(ResourceKey::new(0, 0, 0))).collect();
                    o.design = Some((c, keys));
                }
            }
        }
        if has(C_VISUALSTATE) {
            r.skip(9 + 24 + tail)?;
        }
    }
    // Everything that places the object has been read by now. (What can't be read of the rest
    // doesn't cost the object: it's kept, and whoever reads on finds the next object's header.)
    let script_from = r.pos;
    // (The script component, found by its name, within this object: before the next begins.)
    let data = r.data;
    let find_script = |from: usize| {
        let d = data;
        let u = |p: usize| d.get(p..p + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
        let end = scan_header(d, from, 16384).unwrap_or((from + 16384).min(d.len().saturating_sub(16)));
        (from..end).find(|&p| u(p + 4).is_some_and(|ln| (8..=256).contains(&ln)) && d.get(p + 8..p + 14) == Some(b"Sims3.".as_slice()))
    };
    let read_script = |r: &mut Reader| -> R<String> {
        r.u32()?;
        let ln = r.u32()? as usize;
        if ln > 512 {
            return Err(Eof);
        }
        let s = String::from_utf8_lossy(r.bytes(ln)?).into_owned();
        r.u8()?;
        r.u32()?;
        r.u32()?;
        Ok(s)
    };
    if has(C_ANIMATION) {
        let anim = (|| -> R<()> {
            r.u16()?;
            if r.u8()? != 0 {
                skip_anim(r)?;
            }
            Ok(())
        })();
        // (An animation record of a kind not known here, which later packs add: on to the
        // script, found by its name.)
        if anim.is_err() {
            match find_script(script_from).filter(|_| has(C_SCRIPT)) {
                Some(p) => r.pos = p,
                None => return Ok(o),
            }
        }
    }
    if has(C_SCRIPT) {
        let at = r.pos;
        match read_script(r) {
            Ok(s) => o.script = Some(s),
            Err(_) => {
                // (Where the animation state was misread: the script by its name.)
                let Some(p) = find_script(script_from).filter(|&p| p != at) else { return Ok(o) };
                r.pos = p;
                match read_script(r) {
                    Ok(s) => o.script = Some(s),
                    Err(_) => return Ok(o),
                }
            }
        }
    }
    // (What follows the script isn't needed to place the object, and later packs lengthen some
    // of it (a Sim's SACS block): read as far as it goes, the object kept either way.)
    let rest = |r: &mut Reader, o: &mut PlacedObject| -> R<()> {
        if has(C_PHYSICS) {
            r.u8()?;
        }
        if has(C_TREE) {
            o.speedtree = resolve(r.u16()?);
            r.u32()?;
            let c = r.u32()? as usize;
            if c > 100_000 {
                return Err(Eof);
            }
            for _ in 0..c {
                let mut m = [0f32; 16];
                for v in &mut m {
                    *v = r.f32()?;
                }
                let scale = r.f32()?;
                o.trees.push(TreeInstance { matrix: m, scale });
            }
        }
        if has(C_EFFECT) {
            r.u16()?;
            r.u16()?;
            r.u8()?;
            let ln = r.u8()? as usize;
            r.skip(ln)?;
        }
        if has(C_SIM) {
            r.skip(4)?;
        }
        if has(C_STEERING) {
            r.skip(5)?;
        }
        if has(C_SACS) {
            r.u16()?;
            r.u8()?;
            let c = r.u8()? as usize;
            r.skip(c * 4)?;
        }
        if has(C_SLOT) {
            r.u16()?;
        }
        if has(C_LIGHTING) {
            r.u16()?;
            r.skip(16)?;
        }
        if has(C_VISUALSTATE) {
            r.skip(6)?;
            r.i32()?;
        }
        if has(C_FOOTPRINT) {
            r.u8()?;
            r.u16()?;
            let c = r.u32()? as usize;
            r.skip(c * 4)?;
            r.u16()?;
            let c = r.u32()? as usize;
            r.skip(c * 4)?;
            r.u8()?;
            r.f32()?;
        }
        if has(C_AUDIO) {
            r.skip(20)?;
        }
        Ok(())
    };
    let _ = rest(r, &mut o);
    Ok(o)
}

/// Every placed object in a world package, keyed by the OBJN instance (lot id or layer id).
pub fn load_world_objects(pkg: &s3pkg::Package) -> HashMap<u64, Vec<PlacedObject>> {
    let mut out = HashMap::new();
    for e in pkg.of_type(T_OBJN) {
        let refs_key = ResourceKey::new(T_REFS, REFS_GROUP_OBJN, e.key.i);
        let Some(re) = pkg.find(&refs_key) else { continue };
        let (Ok(rd), Ok(od)) = (pkg.read(re), pkg.read(e)) else { continue };
        let Ok(refs) = parse_refs(&rd) else { continue };
        if let Ok(mut objs) = parse_objn(&od, &refs) {
            // (Sims standing in the world as the world builders left them: the town's people
            // are made from the households, not from these.)
            objs.retain(|o| o.script.as_deref() != Some("Sims3.Gameplay.Actors.Sim"));
            out.insert(e.key.i, objs);
        }
    }
    out
}
