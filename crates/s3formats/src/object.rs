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

/// Resolves the OBJK of an object definition.
pub fn objd_objk(pkgs: &PackageSet, objd: &[u8]) -> Option<ObjKey> {
    let keys = objd_keys(objd).ok()?;
    let k = keys.iter().find(|k| k.t == types::OBJK)?;
    let data = pkgs.read(k).or_else(|| pkgs.read_ti(k.t, k.i))?;
    parse_objk(&data).ok()
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
            .map(|d| crate::model::vpxy_models(&d))
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
    let Some(rk) = crate::model::vpxy_keys(&v).into_iter().find(|k| k.t == crate::model::T_RSLT) else { return Vec::new() };
    pkgs.read(&rk).or_else(|| pkgs.read_ti(rk.t, rk.i)).and_then(|d| crate::model::parse_rslt(&d)).map(|s| s.effects.iter().map(|e| e.pos).collect()).unwrap_or_default()
}

/// An object's routing slots (`routingSlot_N`, model space): where a Sim stands to use it, by
/// slot number, and the way they face there.
pub fn object_route_slots(pkgs: &PackageSet, objd_key: &ResourceKey) -> Vec<(u8, [f32; 3], [f32; 3])> {
    let Some(objd) = pkgs.read(objd_key) else { return Vec::new() };
    let Some(objk) = objd_objk(pkgs, &objd) else { return Vec::new() };
    let Some(mk) = objk.model_key.filter(|k| k.t == types::VPXY) else { return Vec::new() };
    let Some(v) = pkgs.read(&mk).or_else(|| pkgs.read_ti(mk.t, mk.i)) else { return Vec::new() };
    let Some(rk) = crate::model::vpxy_keys(&v).into_iter().find(|k| k.t == crate::model::T_RSLT) else { return Vec::new() };
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
    pub instance_name: String,
    pub name: String,
    pub desc: String,
    pub name_guid: u64,
    pub desc_guid: u64,
    pub price: f32,
    pub show_in_catalog: bool,
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

pub fn parse_objd(d: &[u8]) -> R<ObjdInfo> {
    let keys = tgi_table_at(d, 4)?;
    let mut r = Reader::new(d);
    let version = r.u32()?;
    r.skip(8)?;
    let mat_count = r.i32()?.max(0) as usize;
    let mut presets = Vec::new();
    for _ in 0..mat_count {
        let mtype = r.u8()?;
        if mtype != 1 {
            r.u32()?;
        }
        let len = r.u32()? as usize;
        let end = r.pos + len;
        // u16, the TGI list's offset (relative) and size, the complate, the TGI list.
        let mut m = Reader::at(d, r.pos);
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
            presets.push(p);
        }
        r.pos = end;
        r.u32()?;
    }
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
    let _steal = r.u8()?;
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
    let buy = buy_flags(&mut r, version, sort).unwrap_or(BuyFlags { sort, ..Default::default() });
    Ok(ObjdInfo {
        version,
        instance_name,
        name,
        desc,
        name_guid,
        desc_guid,
        price,
        show_in_catalog: status & 1 != 0,
        objk_index,
        keys,
        presets,
        buy,
    })
}
