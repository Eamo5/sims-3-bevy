//! Catalog objects: OBJD (definition) -> OBJK (components) -> VPXY -> MODL.

use crate::model::tgi_table_at;
use crate::util::{R, Reader};
use s3pkg::{PackageSet, ResourceKey, types};

/// The parts of an OBJK we use.
#[derive(Clone, Debug, Default)]
pub struct ObjKey {
    pub components: Vec<u32>,
    pub model_key: Option<ResourceKey>,
    pub footprint_key: Option<ResourceKey>,
    pub script_class: Option<String>,
}

pub fn parse_objk(d: &[u8]) -> R<ObjKey> {
    let keys = tgi_table_at(d, 4)?;
    let mut r = Reader::at(d, 12);
    let nc = r.u8()? as usize;
    let mut components = Vec::with_capacity(nc);
    for _ in 0..nc {
        components.push(r.u32()?);
    }
    let nd = r.u8()? as usize;
    let mut out = ObjKey { components, ..Default::default() };
    for _ in 0..nd {
        let kl = r.i32()?.max(0) as usize;
        let key = String::from_utf8_lossy(r.bytes(kl)?).into_owned();
        let code = r.u8()?;
        match code {
            0 | 3 => {
                let l = r.i32()?.max(0) as usize;
                let s = String::from_utf8_lossy(r.bytes(l)?).into_owned();
                if key == "scriptClass" {
                    out.script_class = Some(s);
                }
            }
            1 | 2 => {
                let idx = r.i32()?;
                let k = keys.get(idx.max(0) as usize).copied();
                match key.as_str() {
                    "modelKey" => out.model_key = k,
                    "footprintKey" => out.footprint_key = k,
                    _ => {}
                }
            }
            4 => {
                r.u32()?;
            }
            _ => break,
        }
    }
    Ok(out)
}

/// The key table of an OBJD (catalog object definition).
pub fn objd_keys(d: &[u8]) -> R<Vec<ResourceKey>> {
    tgi_table_at(d, 4)
}

/// A key as a pack's patches write the keys of the pack's own resources: without the pack's
/// byte (World Adventures' 0x08: the group's top byte, and the instance's fourth), which the game
/// takes from the resource naming it (`pack`, the top byte of its group). As written when that's
/// there; else with the pack's byte, when that is.
pub fn in_pack(pkgs: &PackageSet, k: ResourceKey, pack: u32) -> ResourceKey {
    let there = |k: &ResourceKey| pkgs.get_entry(k).is_some() || pkgs.find_ti(k.t, k.i).is_some();
    if pack == 0 || k.g >> 24 != 0 || there(&k) {
        return k;
    }
    let p = ResourceKey::new(k.t, k.g | pack << 24, k.i | (pack as u64) << 24);
    if there(&p) { p } else { k }
}

/// Resolves the OBJK of an object definition (its keys in its pack, as the game reads them: see
/// [`in_pack`]).
pub fn objd_objk(pkgs: &PackageSet, objd: &[u8]) -> Option<ObjKey> {
    let keys = objd_keys(objd).ok()?;
    let k = keys.iter().find(|k| k.t == types::OBJK)?;
    let data = pkgs.read(k).or_else(|| pkgs.read_ti(k.t, k.i))?;
    let mut o = parse_objk(&data).ok()?;
    let pack = k.g >> 24;
    o.model_key = o.model_key.map(|m| in_pack(pkgs, m, pack));
    o.footprint_key = o.footprint_key.map(|m| in_pack(pkgs, m, pack));
    Some(o)
}

/// Finds the MODL resources making up an object, given its OBJD key.
pub fn object_models(pkgs: &PackageSet, objd_key: &ResourceKey) -> Vec<ResourceKey> {
    let Some(objd) = pkgs.read(objd_key) else { return Vec::new() };
    let Some(objk) = objd_objk(pkgs, &objd) else { return Vec::new() };
    let Some(mk) = objk.model_key else { return Vec::new() };
    match mk.t {
        types::VPXY => pkgs
            .read(&mk)
            .or_else(|| pkgs.read_ti(mk.t, mk.i))
            .map(|d| crate::model::vpxy_models(&d).into_iter().map(|m| in_pack(pkgs, m, mk.g >> 24)).collect())
            .unwrap_or_default(),
        types::MODL => vec![mk],
        _ => Vec::new(),
    }
}

/// An object's effect slots (where its water, steam and flames come from), in model space:
/// OBJD -> OBJK -> VPXY -> RSLT.
pub fn object_fx_slots(pkgs: &PackageSet, objd_key: &ResourceKey) -> Vec<[f32; 3]> {
    let Some(objd) = pkgs.read(objd_key) else { return Vec::new() };
    let Some(objk) = objd_objk(pkgs, &objd) else { return Vec::new() };
    let Some(mk) = objk.model_key.filter(|k| k.t == types::VPXY) else { return Vec::new() };
    let Some(v) = pkgs.read(&mk).or_else(|| pkgs.read_ti(mk.t, mk.i)) else { return Vec::new() };
    let Some(rk) = crate::model::vpxy_keys(&v).into_iter().find(|k| k.t == crate::model::T_RSLT).map(|k| in_pack(pkgs, k, mk.g >> 24)) else { return Vec::new() };
    pkgs.read(&rk).or_else(|| pkgs.read_ti(rk.t, rk.i)).and_then(|d| crate::model::parse_rslt(&d)).map(|s| s.effects.iter().map(|e| e.pos).collect()).unwrap_or_default()
}

/// A counter sink's bowl (model space): the points of it low enough to be under a counter's
/// top, the cut where it's set in a counter found from them there (the game cuts surfaces by the
/// OBJD's surface cutout mask). Its faucet and taps, higher, left out.
pub fn object_basin(pkgs: &PackageSet, objd_key: &ResourceKey) -> Option<Vec<[f32; 3]>> {
    let mut pts: Vec<[f32; 3]> = Vec::new();
    for mk in object_models(pkgs, objd_key) {
        let meshes = if mk.t == types::MODL { crate::model::load_model(pkgs, &mk) } else { crate::model::load_mlod_resource(pkgs, &mk) };
        for m in meshes.unwrap_or_default() {
            pts.extend(m.positions.iter().filter(|p| p[1] < BOWL_UNDER).copied());
        }
        if !pts.is_empty() {
            break;
        }
    }
    // (Rounded to millimetres, once each.)
    let mut seen = std::collections::HashSet::new();
    pts.retain(|p| seen.insert([(p[0] * 1000.0) as i32, (p[1] * 1000.0) as i32, (p[2] * 1000.0) as i32]));
    (pts.len() >= 3).then_some(pts)
}

/// Above this (model space) a counter sink's points are its taps and faucet.
const BOWL_UNDER: f32 = 0.2;

/// The convex hull of some points (anticlockwise, Andrew's monotone chain).
pub fn convex_hull(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<[f32; 2]> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f32; 2]> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// An object's container slots (model space): where what's slotted into it sits (a dining
/// table's chairs, a counter's sink), by the slot name's hash.
pub fn object_container_slots(pkgs: &PackageSet, objd_key: &ResourceKey) -> Vec<crate::model::Slot> {
    let Some(objd) = pkgs.read(objd_key) else { return Vec::new() };
    let Some(objk) = objd_objk(pkgs, &objd) else { return Vec::new() };
    let Some(mk) = objk.model_key.filter(|k| k.t == types::VPXY) else { return Vec::new() };
    let Some(v) = pkgs.read(&mk).or_else(|| pkgs.read_ti(mk.t, mk.i)) else { return Vec::new() };
    let Some(rk) = crate::model::vpxy_keys(&v).into_iter().find(|k| k.t == crate::model::T_RSLT).map(|k| in_pack(pkgs, k, mk.g >> 24)) else { return Vec::new() };
    pkgs.read(&rk).or_else(|| pkgs.read_ti(rk.t, rk.i)).and_then(|d| crate::model::parse_rslt(&d)).map(|s| s.containers).unwrap_or_default()
}

/// An object's slots as its RSLT has them (all of them, by their names' hashes).
pub fn object_rslt(pkgs: &PackageSet, objd_key: &ResourceKey) -> Option<crate::model::Slots> {
    let objd = pkgs.read(objd_key)?;
    let objk = objd_objk(pkgs, &objd)?;
    let mk = objk.model_key.filter(|k| k.t == types::VPXY)?;
    let v = pkgs.read(&mk).or_else(|| pkgs.read_ti(mk.t, mk.i))?;
    let rk = crate::model::vpxy_keys(&v).into_iter().find(|k| k.t == crate::model::T_RSLT).map(|k| in_pack(pkgs, k, mk.g >> 24))?;
    pkgs.read(&rk).or_else(|| pkgs.read_ti(rk.t, rk.i)).and_then(|d| crate::model::parse_rslt(&d))
}

/// An object's routing slots (`routingSlot_N`, model space): where a Sim stands to use it, by
/// slot number, and the way they face there.
pub fn object_route_slots(pkgs: &PackageSet, objd_key: &ResourceKey) -> Vec<(u8, [f32; 3], [f32; 3])> {
    let Some(objd) = pkgs.read(objd_key) else { return Vec::new() };
    let Some(objk) = objd_objk(pkgs, &objd) else { return Vec::new() };
    let Some(mk) = objk.model_key.filter(|k| k.t == types::VPXY) else { return Vec::new() };
    let Some(v) = pkgs.read(&mk).or_else(|| pkgs.read_ti(mk.t, mk.i)) else { return Vec::new() };
    let Some(rk) = crate::model::vpxy_keys(&v).into_iter().find(|k| k.t == crate::model::T_RSLT).map(|k| in_pack(pkgs, k, mk.g >> 24)) else { return Vec::new() };
    let Some(slots) = pkgs.read(&rk).or_else(|| pkgs.read_ti(rk.t, rk.i)).and_then(|d| crate::model::parse_rslt(&d)) else { return Vec::new() };
    let mut out: Vec<(u8, [f32; 3], [f32; 3])> = slots
        .routing
        .iter()
        .filter_map(|s| {
            let n = (0..32u8).find(|i| s3pkg::fnv32(&format!("routingslot_{i}")) == s.name)?;
            // (The slot's facing: its rotation's third column.)
            Some((n, s.pos, [s.rot[0][2], s.rot[1][2], s.rot[2][2]]))
        })
        .collect();
    out.sort_by_key(|s| s.0);
    out
}

/// Reads a .NET-style 7-bit length prefixed UTF-16BE string (STR7).
fn str7(r: &mut Reader) -> R<String> {
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
    Ok(String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()))
}

/// Catalog information from an OBJD.
#[derive(Clone, Debug, Default)]
pub struct ObjdInfo {
    pub version: u32,
    /// Where the fields after the buy categories begin (bytes; 0 when the record ends early).
    pub after_buy: usize,
    pub instance_name: String,
    pub name: String,
    pub desc: String,
    pub name_guid: u64,
    pub desc_guid: u64,
    pub price: f32,
    pub show_in_catalog: bool,
    /// Whether a Kleptomaniac or a burglar may take it (`IsStealable`).
    pub stealable: bool,
    pub objk_index: u32,
    pub keys: Vec<ResourceKey>,
    /// Where buy mode lists it: by room (`roomCategoryFlags`, `roomSubCategoryFlags`) and by
    /// function (`functionCategoryFlags`, its subcategories in two 64-bit sets), and its place
    /// in the list (`uiSortPriority`). Zero when the record ends early.
    pub buy: BuyFlags,
    /// The object's designs (the catalogue's colour and pattern presets): each a complate with
    /// the resources it refers to. The first is the default.
    pub presets: Vec<crate::catalog::PatternMaterial>,
}

/// An object's buy-mode categories (see [`ObjdInfo::buy`]).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BuyFlags {
    pub room: u32,
    pub function: u32,
    pub function_sub: u64,
    pub function_sub2: u64,
    pub room_sub: u64,
    pub build: u32,
    pub sort: u32,
}

/// The OBJD fields after the OBJK index, up to the buy categories.
fn buy_flags(r: &mut Reader, version: u32, sort: u32) -> R<BuyFlags> {
    r.u32()?; // object type flags
    if version >= 0x1A {
        r.u32()?;
    }
    r.u32()?; // wall placement
    r.u32()?; // movement
    r.u32()?; // wall cutout tiles per level
    r.u32()?; // levels
    let cutouts = r.u8()? as usize;
    r.skip(cutouts * 24)?;
    r.u8()?; // script enabled
    r.u32()?; // diagonal OBJD
    r.u32()?; // ambience
    let room = r.u32()?;
    let function = r.u32()?;
    let function_sub = r.u64()?;
    let function_sub2 = if version >= 0x1C { r.u64()? } else { 0 };
    let room_sub = r.u64()?;
    let build = r.u32()?;
    Ok(BuyFlags { room, function, function_sub, function_sub2, room_sub, build, sort })
}

/// A catalogue resource's materials (an OBJD's designs, a CFEN's): each `u8 type, [u32 when the
/// type isn't 1], u32 size`, then a u16, the TGI list's offset (relative) and size, the complate
/// and the TGI list; then a u32. (Those that won't read are skipped.)
fn material_list(r: &mut Reader, count: usize) -> R<Vec<crate::catalog::PatternMaterial>> {
    let mut out = Vec::new();
    for _ in 0..count {
        let mtype = r.u8()?;
        if mtype != 1 {
            r.u32()?;
        }
        let len = r.u32()? as usize;
        let end = r.pos + len;
        let mut m = Reader::at(r.data, r.pos);
        let preset = (|| -> R<crate::catalog::PatternMaterial> {
            m.u16()?;
            let rel = m.u32()? as usize;
            let keys_at = m.pos + rel;
            m.u32()?;
            let complate = crate::catalog::complate(&mut m, 0)?;
            m.pos = keys_at;
            let keys = crate::catalog::tgi_list(&mut m)?;
            Ok(crate::catalog::PatternMaterial { complate, keys })
        })();
        if let Ok(p) = preset {
            out.push(p);
        }
        r.pos = end;
        r.u32()?;
    }
    Ok(out)
}

/// The materials of a catalogue resource whose list (a u32 count first) starts at `at`.
pub fn materials_at(d: &[u8], at: usize) -> Vec<crate::catalog::PatternMaterial> {
    let mut r = Reader::at(d, at);
    r.u32().ok().and_then(|n| material_list(&mut r, n as usize).ok()).unwrap_or_default()
}

/// An OBJD's slot placement flags (what slots it goes in, by size and kind): the field after the
/// surface cutout, and the floor cutout's (version 0x17 on), the shell states' list (0x18) and the
/// level-below index (0x19).
pub fn objd_slot_placement(d: &[u8], o: &ObjdInfo) -> Option<u32> {
    Reader::at(d, slot_placement_at(d, o)?).u32().ok()
}

/// Where an OBJD's slot placement flags are (see `objd_slot_placement`).
fn slot_placement_at(d: &[u8], o: &ObjdInfo) -> Option<usize> {
    if o.after_buy == 0 {
        return None;
    }
    let mut r = Reader::at(d, o.after_buy);
    r.u32().ok()?;
    if o.version >= 0x17 {
        r.skip(12).ok()?;
    }
    if o.version >= 0x18 {
        let n = r.u32().ok()? as usize;
        r.skip(n * 4).ok()?;
    }
    if o.version >= 0x19 {
        r.u32().ok()?;
    }
    Some(r.pos)
}

/// What an object's catalogue entry says it does (the Buy mode preview's): the moodlet it gives
/// and how much (the game's `MoodletGiven`, `MoodletScore`: Comfy, Well Rested, Enjoying Music,
/// "+" to "+++"), and its topics' ratings (`Moodlet` table effects: Environment 1, Energy 4,
/// Stress Relief 5, Fun 6, the skills; 11 is "+ skill", 0 just the name).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjdRatings {
    pub icon: u32,
    pub degree: u32,
    pub topics: Vec<(u32, u32)>,
}

/// An OBJD's ratings: after its slot placement flags and two material words (UTF-16 runs).
pub fn objd_ratings(d: &[u8], o: &ObjdInfo) -> Option<ObjdRatings> {
    let mut r = Reader::at(d, slot_placement_at(d, o)? + 4);
    // (Skipped by their lengths, not read: an object whose fields here aren't these has none.)
    for _ in 0..2 {
        let n = r.u8().ok()?;
        if n & 0x80 != 0 || n % 2 != 0 {
            return None;
        }
        r.skip(n as usize).ok()?;
    }
    let icon = r.u32().ok()?;
    let degree = r.u32().ok()?;
    let n = r.u32().ok()? as usize;
    if n > 8 || icon > 64 || degree > 3 {
        return None;
    }
    let mut topics = Vec::new();
    for _ in 0..n {
        let (t, v) = (r.u32().ok()?, r.u32().ok()?);
        if t != 0 && t < 256 && v < 256 {
            topics.push((t, v));
        }
    }
    Some(ObjdRatings { icon, degree, topics })
}

pub fn parse_objd(d: &[u8]) -> R<ObjdInfo> {
    let keys = tgi_table_at(d, 4)?;
    let mut r = Reader::new(d);
    let version = r.u32()?;
    r.skip(8)?;
    let mat_count = r.i32()?.max(0) as usize;
    let presets = material_list(&mut r, mat_count)?;
    let instance_name = if version >= 0x16 { str7(&mut r)? } else { String::new() };
    let common_version = r.u32()?;
    let name_guid = r.u64()?;
    let desc_guid = r.u64()?;
    let name = str7(&mut r)?;
    let desc = str7(&mut r)?;
    let price = r.f32()?;
    let _niceness = r.f32()?;
    let _crap = r.f32()?;
    let status = r.u8()?;
    let _png = r.u64()?;
    let _u7 = r.u8()?;
    let _env = r.f32()?;
    let _fire = r.u32()?;
    let steal = r.u8()?;
    let _repo = r.u8()?;
    let sort = r.u32()?;
    if common_version >= 0x0D {
        r.u8()?;
        if common_version >= 0x0E {
            r.u8()?;
            if common_version >= 0x0F {
                r.u32()?;
            }
        }
    }
    let objk_index = r.u32()?;
    let buy = buy_flags(&mut r, version, sort);
    let after_buy = if buy.is_ok() { r.pos } else { 0 };
    let buy = buy.unwrap_or(BuyFlags { sort, ..Default::default() });
    Ok(ObjdInfo {
        version,
        after_buy,
        instance_name,
        name,
        desc,
        name_guid,
        desc_guid,
        price,
        show_in_catalog: status & 1 != 0,
        stealable: steal != 0,
        objk_index,
        keys,
        presets,
        buy,
    })
}
