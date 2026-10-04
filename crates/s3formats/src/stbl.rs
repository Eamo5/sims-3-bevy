//! String tables (STBL, 0x220557DA): localised text keyed by FNV64 hashes.

use std::collections::HashMap;

use crate::util::{Eof, R, Reader};
use s3pkg::{PackageSet, types};

pub fn parse_stbl(d: &[u8], out: &mut HashMap<u64, String>) -> R<()> {
    let mut r = Reader::new(d);
    if r.bytes(4)? != b"STBL" {
        return Err(Eof);
    }
    let _version = r.u8()?;
    let _u1 = r.u16()?;
    let count = r.u32()? as usize;
    let _u2 = r.u16()?;
    let _u3 = r.u32()?;
    for _ in 0..count {
        let key = r.u64()?;
        let len = r.i32()?.max(0) as usize;
        let b = r.bytes(len * 2)?;
        let s = String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>());
        out.entry(key).or_insert(s);
    }
    Ok(())
}

/// Loads every English string table in the package set (instance top byte 0x00).
pub fn load_english(pkgs: &PackageSet) -> HashMap<u64, String> {
    let mut out = HashMap::new();
    let mut keys: Vec<_> = pkgs.keys_of_type(types::STBL).copied().filter(|k| k.i >> 56 == 0).collect();
    keys.sort();
    // Later (higher-priority) tables should win, so read in reverse and keep first insert.
    for k in keys.iter().rev() {
        if let Some(d) = pkgs.read(k) {
            let _ = parse_stbl(&d, &mut out);
        }
    }
    out
}
