//! RefPack (QFS) decompression as used by Sims 3 DBPF packages.

use std::fmt;

#[derive(Debug)]
pub struct RefPackError(pub &'static str);

impl fmt::Display for RefPackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "refpack: {}", self.0)
    }
}

impl std::error::Error for RefPackError {}

pub fn decompress(src: &[u8]) -> Result<Vec<u8>, RefPackError> {
    if src.len() < 2 || src[1] != 0xFB {
        return Err(RefPackError("bad header"));
    }
    let flags = src[0];
    let large = flags & 0x80 != 0;
    let size_len = if large { 4 } else { 3 };
    let mut pos = 2;
    if flags & 0x01 != 0 {
        pos += size_len;
    }
    if src.len() < pos + size_len {
        return Err(RefPackError("truncated header"));
    }
    let mut out_len = 0usize;
    for _ in 0..size_len {
        out_len = (out_len << 8) | src[pos] as usize;
        pos += 1;
    }
    let mut out: Vec<u8> = Vec::with_capacity(out_len);

    loop {
        let Some(&b0) = src.get(pos) else {
            return Err(RefPackError("unexpected end"));
        };
        let (plain, copy_len, offset, used, stop) = if b0 < 0x80 {
            let b1 = *src.get(pos + 1).ok_or(RefPackError("eof"))? as usize;
            let b0 = b0 as usize;
            (b0 & 3, ((b0 & 0x1C) >> 2) + 3, ((b0 & 0x60) << 3) + b1 + 1, 2, false)
        } else if b0 < 0xC0 {
            let b1 = *src.get(pos + 1).ok_or(RefPackError("eof"))? as usize;
            let b2 = *src.get(pos + 2).ok_or(RefPackError("eof"))? as usize;
            let b0 = b0 as usize;
            ((b1 >> 6) & 3, (b0 & 0x3F) + 4, ((b1 & 0x3F) << 8) + b2 + 1, 3, false)
        } else if b0 < 0xE0 {
            let b1 = *src.get(pos + 1).ok_or(RefPackError("eof"))? as usize;
            let b2 = *src.get(pos + 2).ok_or(RefPackError("eof"))? as usize;
            let b3 = *src.get(pos + 3).ok_or(RefPackError("eof"))? as usize;
            let b0 = b0 as usize;
            (
                b0 & 3,
                ((b0 & 0x0C) << 6) + b3 + 5,
                ((b0 & 0x10) << 12) + (b1 << 8) + b2 + 1,
                4,
                false,
            )
        } else if b0 < 0xFC {
            ((((b0 & 0x1F) as usize) << 2) + 4, 0, 0, 1, false)
        } else {
            ((b0 & 3) as usize, 0, 0, 1, true)
        };
        pos += used;
        let plain_src = src
            .get(pos..pos + plain)
            .ok_or(RefPackError("literal overrun"))?;
        out.extend_from_slice(plain_src);
        pos += plain;
        if copy_len > 0 {
            if offset > out.len() {
                return Err(RefPackError("bad back-reference"));
            }
            let start = out.len() - offset;
            if offset >= copy_len {
                out.extend_from_within(start..start + copy_len);
            } else {
                for i in 0..copy_len {
                    let b = out[start + i];
                    out.push(b);
                }
            }
        }
        if stop {
            break;
        }
    }
    Ok(out)
}
