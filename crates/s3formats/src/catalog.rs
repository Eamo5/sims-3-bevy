//! Wall and floor pattern catalogue entries (CWAL, 0x515CA4CD): each holds one or more
//! materials, a complate (texture recipe) with parameter overrides and per-pattern sub-blocks.
//! Layout reverse-engineered against s3pi's `WallFloorPatternCatalogResource` dump.

use crate::util::{Eof, R, Reader};
use s3pkg::ResourceKey;

pub const T_CWAL: u32 = 0x515CA4CD;
/// Roof pattern (catalogue resource "CRMT").
pub const T_ROOF_PATTERN: u32 = 0xF1EDBD86;
pub const T_COMPLATE_XML: u32 = 0x0333406C;

/// Pattern kinds of a CWAL.
pub const PATTERN_FLOOR: u32 = 1;
pub const PATTERN_WALL: u32 = 2;

/// Parameter names the compact complate encoding refers to by index (s3pi's
/// `ComplateString.stringTable`; entry 0x40 is an escape: `0x40 n` means index `0x40 + n`).
const STRINGS: [&str; 112] = [
    "", "filename", "X:", "-1", "assetRoot", "daeFileName", "daeFilePath", "Color", "ObjectRgbMask", "rgbmask", "specmap",
    "Background Image", "HSVShift Bg", "H Bg", "V Bg", "S Bg", "Base H Bg", "Base V Bg", "Base S Bg", "Mask", "Multiplier",
    "Dirt Layer", "1X Multiplier", "Specular", "Overlay", "Face", "partType", "gender", "bodyType", "age", "A", "M", "Stencil A",
    "Stencil B", "Stencil C", "Stencil D", "Stencil A Enabled", "Stencil B Enabled", "Stencil C Enabled", "Stencil D Enabled",
    "Stencil A Tiling", "Stencil B Tiling", "Stencil C Tiling", "Stencil D Tiling", "Stencil A Rotation", "Stencil B Rotation",
    "Stencil C Rotation", "Stencil D Rotation", "Pattern A", "Pattern B", "Pattern C", "Pattern A Enabled", "Pattern B Enabled",
    "Pattern C Enabled", "Pattern A Linked", "Pattern B Linked", "Pattern C Linked", "Pattern A Rotation", "Pattern B Rotation",
    "Pattern C Rotation", "Pattern A Tiling", "Pattern B Tiling", "Pattern C Tiling", "?0x3F", "", "MaskWidth", "MaskHeight",
    "ObjectRgbaMask", "RndColors", "Flat Color", "Alpha", "Color 0", "Color 1", "Color 2", "Color 3", "Color 4", "Channel 1",
    "Channel 2", "Channel 3", "Pattern D", "Pattern D Tiling", "Pattern D Enabled", "Pattern D Linked", "Pattern D Rotation",
    "HSVShift 1", "HSVShift 2", "HSVShift 3", "Channel 1 Enabled", "Channel 2 Enabled", "Channel 3 Enabled", "Base H 1",
    "Base V 1", "Base S 1", "Base H 2", "Base V 2", "Base S 2", "Base H 3", "Base V 3", "Base S 3", "H 1", "S 1", "V 1", "H 2",
    "S 2", "V 2", "H 3", "V 3", "S 3", "true", "1,0,0,0", "defaultFlatColor", "solidColor_1",
];

/// A complate parameter value.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CValue {
    Str(String),
    Argb(u32),
    /// Index into the material's resource list.
    Tgi(u8),
    Float(f32),
    Xy([f32; 2]),
    Xyz([f32; 3]),
    Bool(bool),
}

impl CValue {
    /// The value as the text a complate expression would substitute.
    pub fn text(&self) -> String {
        match self {
            CValue::Str(s) => s.clone(),
            CValue::Argb(c) => {
                let f = |s: u32| ((c >> s) & 255) as f32 / 255.0;
                format!("{},{},{},{}", f(16), f(8), f(0), f(24))
            }
            CValue::Tgi(i) => format!("#tgi{i}"),
            CValue::Float(v) => v.to_string(),
            CValue::Xy([a, b]) => format!("{a},{b}"),
            CValue::Xyz([a, b, c]) => format!("{a},{b},{c}"),
            CValue::Bool(b) => if *b { "True" } else { "False" }.into(),
        }
    }
}

/// A complate instance: which recipe (`xml`, an index into the resource list), its name, the
/// pattern slot it fills in its parent ("Pattern A"…), parameter overrides and the patterns
/// it contains.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Complate {
    pub xml: u8,
    pub name: String,
    pub pattern: String,
    pub overrides: Vec<(String, CValue)>,
    pub blocks: Vec<Complate>,
}

impl Complate {
    pub fn get(&self, name: &str) -> Option<&CValue> {
        self.overrides.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v)
    }
    /// The sub-complate filling a pattern slot.
    pub fn block(&self, slot: &str) -> Option<&Complate> {
        self.blocks.iter().find(|b| b.pattern.eq_ignore_ascii_case(slot))
    }
}

/// One swatch of a pattern: its complate and the resources it refers to.
#[derive(Clone, Debug, Default)]
pub struct PatternMaterial {
    pub complate: Complate,
    pub keys: Vec<ResourceKey>,
}

#[derive(Clone, Debug, Default)]
pub struct WallFloorPattern {
    pub materials: Vec<PatternMaterial>,
    pub name: String,
    /// `PATTERN_FLOOR` or `PATTERN_WALL`.
    pub pattern_type: u32,
    pub keys: Vec<ResourceKey>,
    /// String-table key of the display name, the price, and whether the catalogue shows it.
    pub name_guid: u64,
    pub price: f32,
    pub in_catalog: bool,
}

fn cstring(r: &mut Reader) -> R<String> {
    let b = r.u8()?;
    if b & 0x80 != 0 {
        let n = (b & 0x7F) as usize;
        return Ok(String::from_utf8_lossy(r.bytes(n)?).into_owned());
    }
    let i = if b == 0x40 { 0x40 + r.u8()? as usize } else { b as usize };
    Ok(STRINGS.get(i).copied().unwrap_or("").to_string())
}

/// Reads a complate in the compact catalogue encoding.
pub fn read_complate(r: &mut Reader) -> R<Complate> {
    complate(r, 0)
}

fn complate(r: &mut Reader, depth: usize) -> R<Complate> {
    if depth > 4 {
        return Err(Eof);
    }
    let xml = r.u8()?;
    let name = cstring(r)?;
    let pattern = cstring(r)?;
    let n = r.u32()? as usize;
    if n > 512 {
        return Err(Eof);
    }
    let mut overrides = Vec::with_capacity(n);
    for _ in 0..n {
        let key = cstring(r)?;
        let v = match r.u8()? {
            1 => CValue::Str(cstring(r)?),
            2 => CValue::Argb(r.u32()?),
            3 => CValue::Tgi(r.u8()?),
            4 => CValue::Float(r.f32()?),
            5 => CValue::Xy([r.f32()?, r.f32()?]),
            6 => CValue::Xyz([r.f32()?, r.f32()?, r.f32()?]),
            7 => CValue::Bool(r.u8()? != 0),
            _ => return Err(Eof),
        };
        overrides.push((key, v));
    }
    let nb = r.u32()? as usize;
    if nb > 64 {
        return Err(Eof);
    }
    let blocks = (0..nb).map(|_| complate(r, depth + 1)).collect::<R<Vec<_>>>()?;
    Ok(Complate { xml, name, pattern, overrides, blocks })
}

fn tgi_list(r: &mut Reader) -> R<Vec<ResourceKey>> {
    let n = r.u32()? as usize;
    if n > 4096 {
        return Err(Eof);
    }
    (0..n)
        .map(|_| {
            let t = r.u32()?;
            let g = r.u32()?;
            let i = r.u64()?;
            Ok(ResourceKey::new(t, g, i))
        })
        .collect()
}

impl WallFloorPattern {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        let _version = r.u32()?;
        let rel = r.u32()? as usize;
        let tgi_pos = r.pos + rel;
        let _size = r.u32()?;
        let nm = r.u32()? as usize;
        if nm > 64 {
            return Err(Eof);
        }
        let mut materials = Vec::with_capacity(nm);
        for _ in 0..nm {
            let _kind = r.u8()?;
            let len = r.u32()? as usize;
            let end = r.pos + len;
            let _unknown = r.u16()?;
            let rel = r.u32()? as usize;
            let keys_at = r.pos + rel;
            let _size = r.u32()?;
            let c = complate(&mut r, 0)?;
            r.pos = keys_at;
            let keys = tgi_list(&mut r)?;
            r.pos = end;
            // Trailing per-material fields.
            r.skip(16)?;
            materials.push(PatternMaterial { complate: c, keys });
        }
        // Catalogue common block.
        let _cver = r.u32()?;
        let name_guid = r.u64()?;
        r.u64()?;
        let utf16be = |r: &mut Reader| -> R<String> {
            let raw = r.string_bytes_7bit()?;
            Ok(String::from_utf16_lossy(&raw.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()))
        };
        let name = utf16be(&mut r).unwrap_or_default();
        let _desc = utf16be(&mut r).unwrap_or_default();
        let price = r.f32()?;
        r.skip(8)?; // niceness, crap score
        let status = r.u8()?;
        r.u64()?; // png instance
        r.u8()?;
        r.f32()?; // environment score
        r.u32()?; // fire type
        r.u8()?;
        r.u8()?;
        r.u32()?; // sort priority
        let pattern_type = r.u32().unwrap_or(0);
        r.pos = tgi_pos;
        let keys = tgi_list(&mut r).unwrap_or_default();
        Ok(Self { materials, name, pattern_type, keys, name_guid, price, in_catalog: status & 1 != 0 })
    }
}
