//! Texture compositor (TXTC, 0x033A1435): a list of render steps that bake a texture from
//! pattern images, masks and colours.

use std::collections::HashMap;

use crate::util::{Eof, R, Reader};
use s3pkg::ResourceKey;

pub const STEP_ID: u32 = 0x687720A6;
pub const STEP_DRAW_IMAGE: u32 = 0xA15200B1;
pub const STEP_COLOR_FILL: u32 = 0x9CD1269D;
pub const STEP_DRAW_FABRIC: u32 = 0x034210A5;
pub const STEP_CHANNEL_SELECT: u32 = 0x1E363B9B;
pub const STEP_REMAPPED_CHANNEL_SELECT: u32 = 0x890805DB;
pub const STEP_SET_TARGET: u32 = 0xD6BD8695;
pub const STEP_HSV_TO_RGB: u32 = 0xDC0984B9;
pub const STEP_SKIN_TONE: u32 = 0x43B554E3;
pub const STEP_HAIR_TONE: u32 = 0x5D7C85D4;

pub const P_IMAGE_KEY: u32 = 0xF6CC8471;
pub const P_MASK_KEY: u32 = 0x49DE3B16;
pub const P_DEFAULT_FABRIC: u32 = 0xDCFF6D7B;
pub const P_COLOR: u32 = 0xB01748DA;
pub const P_DEFAULT_COLOR: u32 = 0x64399EC5;
pub const P_CHANNEL_SELECT: u32 = 0xD0E69002;
pub const P_MASK_SELECT: u32 = 0x1F091259;
pub const P_HSV_SHIFT: u32 = 0xB67C2EF8;
pub const P_SRC_RECT: u32 = 0xA3AAFC98;
pub const P_DST_RECT: u32 = 0xE1D6D01F;
pub const P_SRC_BLEND: u32 = 0xE055EE36;
pub const P_DST_BLEND: u32 = 0x048F7567;
pub const P_ENABLE_BLENDING: u32 = 0xFBF310C7;
pub const P_COLOR_WRITE: u32 = 0xB07B3B93;
pub const P_RENDER_TARGET: u32 = 0xA2C91332;
pub const P_WIDTH: u32 = 0x182E64EB;
pub const P_HEIGHT: u32 = 0x4C47D5C0;
pub const P_IMAGE_SOURCE: u32 = 0x8A7006DB;
pub const P_MASK_SOURCE: u32 = 0x10DA0B6A;
pub const P_ROTATION: u32 = 0x49F996DB;
pub const P_MASK_BIAS: u32 = 0x3A3260E6;
pub const P_DESCRIPTION: u32 = 0x6B7119C1;
pub const P_SKIP_DETAIL: u32 = 0x331178DF;
pub const P_MIN_DETAIL: u32 = 0xAE5FE82A;

#[derive(Clone, Debug)]
pub enum Value {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f32),
    Vec4([f32; 4]),
    Tgi(u8),
    Str(String),
}

impl Value {
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::Int(v) => Some(*v as u32),
            Value::UInt(v) => Some(*v as u32),
            Value::Bool(b) => Some(*b as u32),
            Value::Tgi(i) => Some(*i as u32),
            _ => None,
        }
    }
    pub fn as_vec4(&self) -> Option<[f32; 4]> {
        match self {
            Value::Vec4(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Step {
    pub props: Vec<(u32, Value)>,
}

impl Step {
    pub fn get(&self, p: u32) -> Option<&Value> {
        self.props.iter().find(|(k, _)| *k == p).map(|(_, v)| v)
    }
    pub fn kind(&self) -> u32 {
        self.get(STEP_ID).and_then(|v| v.as_u32()).unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Txtc {
    pub version: u32,
    pub fabrics: Vec<(u8, Txtc)>,
    pub steps: Vec<Step>,
    pub keys: Vec<ResourceKey>,
}

impl Txtc {
    pub fn parse(d: &[u8]) -> R<Self> {
        let mut r = Reader::new(d);
        Self::parse_at(&mut r)
    }

    fn parse_at(r: &mut Reader) -> R<Self> {
        let version = r.u32()?;
        let rel = r.u32()? as usize;
        let tgi_pos = r.pos + rel;
        let mut fabrics = Vec::new();
        if version >= 7 {
            let n = r.u8()? as usize;
            for _ in 0..n {
                let idx = r.u8()?;
                let size = r.u32()? as usize;
                let start = r.pos;
                let f = Self::parse_at(r)?;
                r.skip(3)?;
                r.pos = start + size;
                fabrics.push((idx, f));
            }
        }
        let _pattern_size = r.u32()?;
        let _data_type = r.u32()?;
        let _u3 = r.u8()?;
        let count = r.i32()?.max(0) as usize;
        if version >= 8 {
            r.u8()?;
        }
        if count > 512 {
            return Err(Eof);
        }
        let mut steps = Vec::with_capacity(count);
        for _ in 0..count {
            let mut step = Step::default();
            loop {
                let prop = r.u32()?;
                if prop == 0 {
                    break;
                }
                let _unk = r.u8()?;
                let dt = r.u8()?;
                let v = match dt {
                    0x00 => Value::Bool(r.u8()? != 0),
                    0x01 => Value::Int(r.u8()? as i8 as i64),
                    0x05 => Value::UInt(r.u8()? as u64),
                    0x0C => Value::Tgi(r.u8()?),
                    0x02 => Value::Int(r.i16()? as i64),
                    0x06 => Value::UInt(r.u16()? as u64),
                    0x03 => Value::Int(r.i32()? as i64),
                    0x07 => Value::UInt(r.u32()? as u64),
                    0x04 => Value::Int(r.u64()? as i64),
                    0x08 => Value::UInt(r.u64()?),
                    0x09 => Value::Float(r.f32()?),
                    0x0A | 0x0B => Value::Vec4([r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
                    0x0D => {
                        let n = r.u16()? as usize;
                        Value::Str(String::from_utf8_lossy(r.bytes(n)?).into_owned())
                    }
                    _ => return Err(Eof),
                };
                step.props.push((prop, v));
            }
            steps.push(step);
        }
        r.pos = tgi_pos;
        let n = r.u8()? as usize;
        let mut keys = Vec::with_capacity(n);
        for _ in 0..n {
            let i = r.u64()?;
            let g = r.u32()?;
            let t = r.u32()?;
            keys.push(ResourceKey::new(t, g, i));
        }
        Ok(Self { version, fabrics, steps, keys })
    }

    pub fn step_name(kind: u32) -> &'static str {
        match kind {
            STEP_DRAW_IMAGE => "DrawImage",
            STEP_COLOR_FILL => "ColorFill",
            STEP_DRAW_FABRIC => "DrawFabric",
            STEP_CHANNEL_SELECT => "ChannelSelect",
            STEP_REMAPPED_CHANNEL_SELECT => "RemappedChannelSelect",
            STEP_SET_TARGET => "SetTarget",
            STEP_HSV_TO_RGB => "HSVtoRGB",
            STEP_SKIN_TONE => "SkinTone",
            STEP_HAIR_TONE => "HairTone",
            0xC6B6AC1F => "CASPickData",
            _ => "?",
        }
    }

    pub fn prop_name(p: u32) -> &'static str {
        let names: HashMap<u32, &str> = [
            (P_IMAGE_KEY, "ImageKey"),
            (P_MASK_KEY, "MaskKey"),
            (P_DEFAULT_FABRIC, "DefaultFabric"),
            (P_COLOR, "Color"),
            (P_DEFAULT_COLOR, "DefaultColor"),
            (P_CHANNEL_SELECT, "ChannelSelect"),
            (P_MASK_SELECT, "MaskSelect"),
            (P_HSV_SHIFT, "HSVShift"),
            (P_SRC_RECT, "SrcRect"),
            (P_DST_RECT, "DstRect"),
            (P_SRC_BLEND, "SrcBlend"),
            (P_DST_BLEND, "DstBlend"),
            (P_ENABLE_BLENDING, "EnableBlending"),
            (P_COLOR_WRITE, "ColorWrite"),
            (P_RENDER_TARGET, "RenderTarget"),
            (P_WIDTH, "Width"),
            (P_HEIGHT, "Height"),
            (P_IMAGE_SOURCE, "ImageSource"),
            (P_MASK_SOURCE, "MaskSource"),
            (P_ROTATION, "Rotation"),
            (P_MASK_BIAS, "MaskBias"),
            (P_DESCRIPTION, "Description"),
            (STEP_ID, "ID"),
            (P_SKIP_DETAIL, "SkipDetail"),
            (P_MIN_DETAIL, "MinDetail"),
            (0xD92A4C8B, "UIVisible"),
            (0xE27FE962, "EnableFiltering"),
            (0x06A775CE, "SkipShaderModel"),
            (0x2EDF5F53, "MinShaderModel"),
        ]
        .into_iter()
        .collect();
        names.get(&p).copied().unwrap_or("?")
    }
}
