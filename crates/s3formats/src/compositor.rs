//! CPU implementation of the TXTC texture compositor.

use std::collections::HashMap;

use crate::dds::{self, Rgba};
use crate::txtc::*;
use s3pkg::{PackageSet, ResourceKey, types};

const RT_A: u32 = 0x021E9CD2;
const RT_B: u32 = 0x021E9CD4;

type Buf = Vec<[f32; 4]>;

fn blend_factor(f: i64, src: [f32; 4], dst: [f32; 4], c: usize) -> f32 {
    match f {
        0 => 0.0,
        1 => 1.0,
        2 => src[c],
        3 => 1.0 - src[c],
        4 => src[3],
        5 => 1.0 - src[3],
        6 => dst[3],
        7 => 1.0 - dst[3],
        8 => dst[c],
        9 => 1.0 - dst[c],
        10 => {
            if c == 3 {
                1.0
            } else {
                src[3].min(1.0 - dst[3])
            }
        }
        _ => 1.0,
    }
}

fn argb(c: u32) -> [f32; 4] {
    [
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
        ((c >> 24) & 255) as f32 / 255.0,
    ]
}

fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let h = if d < 1e-6 {
        0.0
    } else if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0) / 6.0
    } else if max == c[1] {
        ((c[2] - c[0]) / d + 2.0) / 6.0
    } else {
        ((c[0] - c[1]) / d + 4.0) / 6.0
    };
    let s = if max < 1e-6 { 0.0 } else { d / max };
    [h, s, max]
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

enum Src {
    Color([f32; 4]),
    Image(Rgba),
    Target(Buf),
    Channel(Rgba, [f32; 4]),
    Hsv(Rgba, [f32; 4]),
}

pub struct Compositor<'a> {
    pkgs: &'a PackageSet,
    images: HashMap<ResourceKey, Option<Rgba>>,
    pub max_size: usize,
    base: Option<Buf>,
}

impl<'a> Compositor<'a> {
    pub fn new(pkgs: &'a PackageSet) -> Self {
        Self { pkgs, images: HashMap::new(), max_size: 512, base: None }
    }

    fn image(&mut self, key: ResourceKey) -> Option<Rgba> {
        if !self.images.contains_key(&key) {
            let data = self.pkgs.read(&key).or_else(|| self.pkgs.read_ti(key.t, key.i));
            let img = data.filter(|_| key.t == types::DDS).and_then(|d| dds::decode(&d, self.max_size));
            self.images.insert(key, img);
        }
        self.images.get(&key).and_then(|o| o.as_ref()).map(|i| Rgba { width: i.width, height: i.height, data: i.data.clone() })
    }

    /// Picks an output size from the first DDS the compositor references.
    pub fn output_size(&mut self, t: &Txtc) -> (usize, usize) {
        for k in t.keys.clone() {
            if k.t == types::DDS
                && let Some(img) = self.image(k)
            {
                return (img.width.max(64), img.height.max(64));
            }
        }
        (256, 256)
    }

    fn source(&mut self, t: &Txtc, step: &Step, targets: &HashMap<u32, Buf>, fabrics: &mut HashMap<u8, Rgba>) -> Option<Src> {
        let kind = step.kind();
        match kind {
            STEP_COLOR_FILL => Some(Src::Color(argb(step.get(P_COLOR).and_then(|v| v.as_u32()).unwrap_or(0)))),
            STEP_DRAW_IMAGE | STEP_CHANNEL_SELECT | STEP_HSV_TO_RGB => {
                if let Some(Value::Tgi(i)) = step.get(P_IMAGE_KEY) {
                    let img = self.image(*t.keys.get(*i as usize)?)?;
                    Some(match kind {
                        STEP_CHANNEL_SELECT => Src::Channel(
                            img,
                            step.get(P_CHANNEL_SELECT).and_then(|v| v.as_vec4()).unwrap_or([1.0, 0.0, 0.0, 0.0]),
                        ),
                        STEP_HSV_TO_RGB => Src::Hsv(img, step.get(P_HSV_SHIFT).and_then(|v| v.as_vec4()).unwrap_or([0.0; 4])),
                        _ => Src::Image(img),
                    })
                } else {
                    let id = step.get(P_IMAGE_SOURCE).and_then(|v| v.as_u32())?;
                    let rt = if id == RT_A || id == RT_A + 1 { RT_A } else { RT_B };
                    Some(Src::Target(targets.get(&rt)?.clone()))
                }
            }
            STEP_DRAW_FABRIC => {
                let i = match step.get(P_DEFAULT_FABRIC) {
                    Some(Value::Tgi(i)) => *i,
                    _ => return None,
                };
                let fw = step.get(P_WIDTH).and_then(|v| v.as_u32()).unwrap_or(256).clamp(16, 512) as usize;
                let fh = step.get(P_HEIGHT).and_then(|v| v.as_u32()).unwrap_or(256).clamp(16, 512) as usize;
                if !fabrics.contains_key(&i) {
                    let embedded = t.fabrics.iter().find(|(fi, _)| *fi == i).map(|(_, f)| {
                        // Embedded fabrics index images through the parent's key table.
                        let mut f = f.clone();
                        if f.keys.is_empty() {
                            f.keys = t.keys.clone();
                        }
                        f
                    });
                    let fab = embedded.or_else(|| {
                        let k = t.keys.get(i as usize)?;
                        let d = self.pkgs.read(k).or_else(|| self.pkgs.read_ti(k.t, k.i))?;
                        Txtc::parse(&d).ok()
                    })?;
                    let img = self.run(&fab, fw, fh);
                    fabrics.insert(i, img);
                }
                let f = fabrics.get(&i)?;
                Some(Src::Image(Rgba { width: f.width, height: f.height, data: f.data.clone() }))
            }
            _ => None,
        }
    }

    /// Runs a compositor program and returns the final RGBA image.
    pub fn run(&mut self, t: &Txtc, w: usize, h: usize) -> Rgba {
        self.run_limited(t, w, h, usize::MAX)
    }

    /// Runs a program with render target A pre-filled from `base` (e.g. skin under clothing).
    pub fn run_with_base(&mut self, t: &Txtc, w: usize, h: usize, base: Option<Rgba>) -> Rgba {
        self.base = base.map(|b| {
            let mut buf = vec![[0.0; 4]; w * h];
            for y in 0..h {
                for x in 0..w {
                    buf[y * w + x] = b.sample((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                }
            }
            buf
        });
        self.run_limited(t, w, h, usize::MAX)
    }

    pub fn run_limited(&mut self, t: &Txtc, w: usize, h: usize, limit: usize) -> Rgba {
        let mut targets: HashMap<u32, Buf> = HashMap::new();
        let base = self.base.take().filter(|b| b.len() == w * h);
        targets.insert(RT_A, base.unwrap_or_else(|| vec![[0.0; 4]; w * h]));
        targets.insert(RT_B, vec![[0.0; 4]; w * h]);
        let mut current = RT_A;
        let mut fabrics: HashMap<u8, Rgba> = HashMap::new();

        for step in t.steps.iter().take(limit) {
            if step.kind() == STEP_SET_TARGET {
                if let Some(rt) = step.get(P_RENDER_TARGET).and_then(|v| v.as_u32())
                    && targets.contains_key(&rt)
                {
                    current = rt;
                }
                continue;
            }
            let Some(src) = self.source(t, step, &targets, &mut fabrics) else { continue };
            let write = step.get(P_COLOR_WRITE).and_then(|v| v.as_u32()).unwrap_or(15);
            let blending = matches!(step.get(P_ENABLE_BLENDING), Some(Value::Bool(true)));
            let sb = step.get(P_SRC_BLEND).and_then(|v| v.as_u32()).map(|v| v as i32 as i64).unwrap_or(1);
            let db = step.get(P_DST_BLEND).and_then(|v| v.as_u32()).map(|v| v as i32 as i64).unwrap_or(0);
            let dst_rect = step.get(P_DST_RECT).and_then(|v| v.as_vec4()).unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let src_rect = step.get(P_SRC_RECT).and_then(|v| v.as_vec4()).unwrap_or([0.0, 0.0, 1.0, 1.0]);

            let buf = targets.get_mut(&current).unwrap();
            let x0 = (dst_rect[0] * w as f32).round().max(0.0) as usize;
            let y0 = (dst_rect[1] * h as f32).round().max(0.0) as usize;
            let x1 = ((dst_rect[2] * w as f32).round().max(0.0) as usize).min(w);
            let y1 = ((dst_rect[3] * h as f32).round().max(0.0) as usize).min(h);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            for y in y0..y1 {
                let v = ((y - y0) as f32 + 0.5) / (y1 - y0) as f32;
                let sv = src_rect[1] + v * (src_rect[3] - src_rect[1]);
                for x in x0..x1 {
                    let u = ((x - x0) as f32 + 0.5) / (x1 - x0) as f32;
                    let su = src_rect[0] + u * (src_rect[2] - src_rect[0]);
                    let s: [f32; 4] = match &src {
                        Src::Color(c) => *c,
                        Src::Image(img) => img.sample(su, sv),
                        Src::Target(b) => b[y * w + x],
                        Src::Channel(img, sel) => {
                            let p = img.sample(su, sv);
                            let val = p[0] * sel[0] + p[1] * sel[1] + p[2] * sel[2] + p[3] * sel[3];
                            [val; 4]
                        }
                        Src::Hsv(img, shift) => {
                            let p = img.sample(su, sv);
                            let hsv = rgb_to_hsv([p[0], p[1], p[2]]);
                            let rgb = hsv_to_rgb(
                                hsv[0] + shift[0],
                                (hsv[1] + shift[1]).clamp(0.0, 1.0),
                                (hsv[2] + shift[2]).clamp(0.0, 1.0),
                            );
                            [rgb[0], rgb[1], rgb[2], p[3]]
                        }
                    };
                    let d = buf[y * w + x];
                    let mut out = d;
                    for c in 0..4 {
                        if write & (1 << c) == 0 {
                            continue;
                        }
                        out[c] = if blending {
                            (s[c] * blend_factor(sb, s, d, c) + d[c] * blend_factor(db, s, d, c)).clamp(0.0, 1.0)
                        } else {
                            s[c]
                        };
                    }
                    buf[y * w + x] = out;
                }
            }
        }
        let buf = &targets[&current];
        let mut data = Vec::with_capacity(w * h * 4);
        for p in buf {
            for c in p {
                data.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        Rgba { width: w, height: h, data }
    }
}

/// Composites a TXTC resource with its default fabrics into an RGBA image.
pub fn composite(pkgs: &PackageSet, txtc: &[u8], max_size: usize) -> Option<Rgba> {
    let t = Txtc::parse(txtc).ok()?;
    let mut c = Compositor::new(pkgs);
    c.max_size = max_size;
    let (w, h) = c.output_size(&t);
    let limit: usize = std::env::var("TXTC_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    Some(c.run_limited(&t, w, h, limit))
}
