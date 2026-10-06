//! Writing GPU-ready DDS files: BC1/BC3 encoding of composited images and mip-chain
//! trimming of the game's own DDS textures (no re-encode).

/// Decodes BC3 (DXT5) blocks to RGBA8.
pub fn decode_bc3(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0u8; width * height * 4];
    texpresso::Format::Bc3.decompress(data, width, height, &mut out);
    out
}

use s3formats::dds::{Rgba, build_mips};

fn dds_header(width: u32, height: u32, mips: u32, fourcc: &[u8; 4], top_level_size: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(128);
    h.extend_from_slice(b"DDS ");
    let flags: u32 = 0x1 | 0x2 | 0x4 | 0x1000 | 0x20000 | 0x80000;
    for v in [124u32, flags, height, width, top_level_size, 0, mips] {
        h.extend_from_slice(&v.to_le_bytes());
    }
    h.extend_from_slice(&[0u8; 44]);
    // Pixel format
    h.extend_from_slice(&32u32.to_le_bytes());
    h.extend_from_slice(&0x4u32.to_le_bytes());
    h.extend_from_slice(fourcc);
    h.extend_from_slice(&[0u8; 20]);
    let caps: u32 = 0x1000 | if mips > 1 { 0x400008 } else { 0 };
    h.extend_from_slice(&caps.to_le_bytes());
    h.extend_from_slice(&[0u8; 16]);
    debug_assert_eq!(h.len(), 128);
    h
}

/// Encodes an RGBA image (with generated mips) as a BC1 (opaque) or BC3 DDS file.
pub fn encode_dds(img: &Rgba) -> Vec<u8> {
    let opaque = img.data.chunks_exact(4).all(|p| p[3] >= 250);
    let format = if opaque { texpresso::Format::Bc1 } else { texpresso::Format::Bc3 };
    let (chain, levels) = build_mips(img);
    let params = texpresso::Params { algorithm: texpresso::Algorithm::RangeFit, ..Default::default() };
    let mut out = Vec::new();
    let mut body = Vec::new();
    let (mut w, mut h) = (img.width, img.height);
    let mut off = 0;
    let mut top = 0;
    for level in 0..levels {
        let n = w * h * 4;
        let src = &chain[off..off + n];
        let mut buf = vec![0u8; format.compressed_size(w, h)];
        format.compress(src, w, h, params, &mut buf);
        if level == 0 {
            top = buf.len();
        }
        body.extend_from_slice(&buf);
        off += n;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    out.extend(dds_header(img.width as u32, img.height as u32, levels, if opaque { b"DXT1" } else { b"DXT5" }, top as u32));
    out.extend(body);
    out
}

/// Drops top mip levels of an existing DDS until it is at most `max` texels wide/high.
/// Returns the original bytes when nothing needs trimming or the format is unknown.
pub fn trim_dds(d: &[u8], max: u32) -> Vec<u8> {
    if d.len() < 128 || &d[0..4] != b"DDS " {
        return d.to_vec();
    }
    let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let (height, width, mips) = (u(12), u(16), u(28).max(1));
    let fourcc: [u8; 4] = d[84..88].try_into().unwrap();
    let bits = u(88);
    let block = match &fourcc {
        b"DXT1" => Some(8),
        b"DXT3" | b"DXT5" => Some(16),
        _ => None,
    };
    if &fourcc == b"DX10" {
        return d.to_vec();
    }
    let level_size = |w: u32, h: u32| -> usize {
        match block {
            Some(b) => (w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * b) as usize,
            None => (w * h * (bits / 8).max(1)) as usize,
        }
    };
    let (mut w, mut h, mut m) = (width.max(1), height.max(1), mips);
    let mut off = 128;
    while (w > max || h > max) && m > 1 {
        off += level_size(w, h);
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        m -= 1;
    }
    if off == 128 || off >= d.len() {
        return d.to_vec();
    }
    let mut out = d[..128].to_vec();
    out[12..16].copy_from_slice(&h.to_le_bytes());
    out[16..20].copy_from_slice(&w.to_le_bytes());
    out[20..24].copy_from_slice(&(level_size(w, h) as u32).to_le_bytes());
    out[28..32].copy_from_slice(&m.to_le_bytes());
    out.extend_from_slice(&d[off..]);
    out
}
