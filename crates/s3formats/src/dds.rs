//! CPU decoding of DDS textures (DXT1/3/5 and common uncompressed formats) to RGBA8.

pub struct Rgba {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Rgba {
    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width + x) * 4;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }

    /// Bilinear sample with wrap-around at UV (0..1 repeating), returning 0..1 floats.
    pub fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let (w, h) = (self.width as f32, self.height as f32);
        let x = u * w - 0.5;
        let y = v * h - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = x - x0;
        let fy = y - y0;
        let wrap = |a: f32, n: usize| (a as i64).rem_euclid(n as i64) as usize;
        let (xa, xb) = (wrap(x0, self.width), wrap(x0 + 1.0, self.width));
        let (ya, yb) = (wrap(y0, self.height), wrap(y0 + 1.0, self.height));
        let p = |x: usize, y: usize| self.get(x, y);
        let (a, b, c, d) = (p(xa, ya), p(xb, ya), p(xa, yb), p(xb, yb));
        let mut out = [0.0; 4];
        for k in 0..4 {
            let top = a[k] as f32 * (1.0 - fx) + b[k] as f32 * fx;
            let bot = c[k] as f32 * (1.0 - fx) + d[k] as f32 * fx;
            out[k] = (top * (1.0 - fy) + bot * fy) / 255.0;
        }
        out
    }
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8]
}

fn color_block(b: &[u8], out: &mut [[u8; 4]; 16], dxt1: bool) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !dxt1 {
        for k in 0..3 {
            pal[2][k] = ((2 * p0[k] as u32 + p1[k] as u32) / 3) as u8;
            pal[3][k] = ((p0[k] as u32 + 2 * p1[k] as u32) / 3) as u8;
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for k in 0..3 {
            pal[2][k] = ((p0[k] as u32 + p1[k] as u32) / 2) as u8;
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for (i, px) in out.iter_mut().enumerate() {
        let idx = ((bits >> (i * 2)) & 3) as usize;
        let a = px[3];
        *px = pal[idx];
        if !dxt1 {
            px[3] = a;
        }
    }
}

fn alpha_block_dxt5(b: &[u8], out: &mut [[u8; 4]; 16]) {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut pal = [0u8; 8];
    pal[0] = a0 as u8;
    pal[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7 {
            pal[i + 1] = (((7 - i) as u32 * a0 + i as u32 * a1) / 7) as u8;
        }
    } else {
        for i in 1..5 {
            pal[i + 1] = (((5 - i) as u32 * a0 + i as u32 * a1) / 5) as u8;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let mut bits: u64 = 0;
    for i in 0..6 {
        bits |= (b[2 + i] as u64) << (8 * i);
    }
    for (i, px) in out.iter_mut().enumerate() {
        px[3] = pal[((bits >> (3 * i)) & 7) as usize];
    }
}

fn alpha_block_dxt3(b: &[u8], out: &mut [[u8; 4]; 16]) {
    for (i, px) in out.iter_mut().enumerate() {
        let nib = (b[i / 2] >> ((i % 2) * 4)) & 15;
        px[3] = nib * 17;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fmt {
    Dxt1,
    Dxt3,
    Dxt5,
    Argb8,
    Xrgb8,
    A8L8,
    L8,
}

/// Decodes the largest mip level whose width is <= `max_size` (or the smallest available).
pub fn decode(d: &[u8], max_size: usize) -> Option<Rgba> {
    if d.len() < 128 || &d[0..4] != b"DDS " {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let height = u(12) as usize;
    let width = u(16) as usize;
    let mips = u(28).max(1) as usize;
    let pf_flags = u(80);
    let fourcc = &d[84..88];
    let bits = u(88);
    let rmask = u(92);
    let amask = u(104);
    let fmt = match fourcc {
        b"DXT1" => Fmt::Dxt1,
        b"DXT3" => Fmt::Dxt3,
        b"DXT5" => Fmt::Dxt5,
        _ if bits == 32 && amask != 0 => Fmt::Argb8,
        _ if bits == 32 => Fmt::Xrgb8,
        _ if bits == 16 && (pf_flags & 0x20000 != 0 || amask == 0xFF00) => Fmt::A8L8,
        _ if bits == 8 => Fmt::L8,
        _ => return None,
    };
    let _ = rmask;
    let level_size = |w: usize, h: usize| match fmt {
        Fmt::Dxt1 => w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * 8,
        Fmt::Dxt3 | Fmt::Dxt5 => w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * 16,
        Fmt::Argb8 | Fmt::Xrgb8 => w * h * 4,
        Fmt::A8L8 => w * h * 2,
        Fmt::L8 => w * h,
    };
    let mut off = 128;
    let (mut w, mut h) = (width.max(1), height.max(1));
    for _ in 0..mips - 1 {
        if w <= max_size && h <= max_size {
            break;
        }
        let sz = level_size(w, h);
        if off + sz + level_size((w / 2).max(1), (h / 2).max(1)) > d.len() {
            break;
        }
        off += sz;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    let src = d.get(off..off + level_size(w, h))?;
    let mut out = vec![0u8; w * h * 4];
    match fmt {
        Fmt::Dxt1 | Fmt::Dxt3 | Fmt::Dxt5 => {
            let bs = if fmt == Fmt::Dxt1 { 8 } else { 16 };
            let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
            for by in 0..bh {
                for bx in 0..bw {
                    let b = &src[(by * bw + bx) * bs..(by * bw + bx + 1) * bs];
                    let mut px = [[0u8, 0, 0, 255]; 16];
                    match fmt {
                        Fmt::Dxt1 => color_block(b, &mut px, true),
                        Fmt::Dxt3 => {
                            alpha_block_dxt3(&b[0..8], &mut px);
                            color_block(&b[8..16], &mut px, false);
                        }
                        _ => {
                            alpha_block_dxt5(&b[0..8], &mut px);
                            color_block(&b[8..16], &mut px, false);
                        }
                    }
                    for py in 0..4 {
                        for pxx in 0..4 {
                            let (x, y) = (bx * 4 + pxx, by * 4 + py);
                            if x < w && y < h {
                                let o = (y * w + x) * 4;
                                out[o..o + 4].copy_from_slice(&px[py * 4 + pxx]);
                            }
                        }
                    }
                }
            }
        }
        Fmt::Argb8 | Fmt::Xrgb8 => {
            for i in 0..w * h {
                let (b, g, r, a) = (src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]);
                out[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, if fmt == Fmt::Argb8 { a } else { 255 }]);
            }
        }
        Fmt::A8L8 => {
            for i in 0..w * h {
                let (l, a) = (src[i * 2], src[i * 2 + 1]);
                out[i * 4..i * 4 + 4].copy_from_slice(&[l, l, l, a]);
            }
        }
        Fmt::L8 => {
            for i in 0..w * h {
                let l = src[i];
                out[i * 4..i * 4 + 4].copy_from_slice(&[l, l, l, 255]);
            }
        }
    }
    Some(Rgba { width: w, height: h, data: out })
}

/// Box-filtered mip chain for an RGBA8 image (level 0 first), concatenated.
pub fn build_mips(img: &Rgba) -> (Vec<u8>, u32) {
    let mut data = img.data.clone();
    let (mut w, mut h) = (img.width, img.height);
    let mut prev = img.data.clone();
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(w - 1);
                        let sy = (y * 2 + dy).min(h - 1);
                        s += prev[(sy * w + sx) * 4 + c] as u32;
                    }
                    next[(y * nw + x) * 4 + c] = (s / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        prev = next;
        w = nw;
        h = nh;
        levels += 1;
    }
    (data, levels)
}
