//! Trees: the game grows its SpeedTrees from procedural parameters at run time, but each one
//! also ships 360° billboard pictures of the whole tree (its distant LOD). We bake those
//! pictures, find the individual views in the atlas, and take the tree's size from its
//! TREE record, so trees can be drawn as crossed billboards.

use s3pkg::{PackageSet, fnv64};

use crate::types::*;

const T_SPEEDTREE: u32 = 0x00B552EA;
const T_TREE_INFO: u32 = 0x021D7E8C;

/// The texture names a SpeedTree parameter file refers to.
fn texture_names(spt: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, &b) in spt.iter().enumerate() {
        let printable = b.is_ascii_graphic() || b == b' ';
        match (printable, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if let Ok(t) = std::str::from_utf8(&spt[s..i])
                    && let Some(p) = t.to_ascii_lowercase().find(".tga")
                {
                    // Keep the file stem: some names carry the artist's full path.
                    let stem = t[..p].rsplit(['\\', '/']).next().unwrap_or(&t[..p]);
                    out.push(stem.to_string());
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// Bounds (min, max) from the tree's TREE record.
fn tree_bounds(d: &[u8]) -> Option<([f32; 3], [f32; 3])> {
    let p = d.windows(4).position(|w| w == b"TREE")? + 8;
    let f = |i: usize| d.get(p + i * 4..p + i * 4 + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()));
    let (mn, mx) = ([f(0)?, f(1)?, f(2)?], [f(3)?, f(4)?, f(5)?]);
    (mx[1] > mn[1] && mx[1] - mn[1] < 200.0).then_some((mn, mx))
}

/// The pictures of a 360° billboard atlas, cut apart along its empty rows and columns (an XY
/// cut, again and again until each piece is one picture), in the atlas's order: the views round
/// the tree, and the one from above (the odd one out, much bigger than the rest). None when it
/// doesn't come apart into at least three.
pub fn round_views(img: &s3formats::dds::Rgba) -> Option<(Vec<[f32; 4]>, Option<[f32; 4]>)> {
    let (w, h) = (img.width, img.height);
    let solid = |x: usize, y: usize| img.data[(y * w + x) * 4 + 3] > 96;
    // Runs of non-empty lines at least a few pixels long.
    fn runs(v: &[usize]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = None;
        for (i, &n) in v.iter().chain(std::iter::once(&0)).enumerate() {
            match (n > 0, start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    if i - s >= 6 {
                        out.push((s, i));
                    }
                    start = None;
                }
                _ => {}
            }
        }
        out
    }
    let mut cells = Vec::new();
    // (x0, y0, x1, y1, cut rows first, depth)
    let mut todo = vec![(0usize, 0usize, w, h, true, 0u8)];
    while let Some((x0, y0, x1, y1, rows_first, depth)) = todo.pop() {
        let rows: Vec<usize> = (y0..y1).map(|y| (x0..x1).filter(|&x| solid(x, y)).count()).collect();
        let cols: Vec<usize> = (x0..x1).map(|x| (y0..y1).filter(|&y| solid(x, y)).count()).collect();
        let (r, c) = (runs(&rows), runs(&cols));
        let split_rows = if rows_first { r.len() > 1 || c.len() <= 1 } else { c.len() <= 1 && r.len() > 1 };
        if depth < 8 && split_rows && r.len() > 1 {
            for &(a, b) in r.iter().rev() {
                todo.push((x0, y0 + a, x1, y0 + b, false, depth + 1));
            }
        } else if depth < 8 && c.len() > 1 {
            for &(a, b) in c.iter().rev() {
                todo.push((x0 + a, y0, x0 + b, y1, true, depth + 1));
            }
        } else if let (Some(&(ra, rb)), Some(&(ca, cb))) = (r.first(), c.first()) {
            if (cb - ca) * (rb - ra) * 200 > w * h {
                cells.push((x0 + ca, y0 + ra, x0 + cb, y0 + rb));
            }
        }
    }
    if cells.len() < 3 {
        return None;
    }
    let uv = |(x0, y0, x1, y1): (usize, usize, usize, usize)| [x0 as f32 / w as f32, y0 as f32 / h as f32, x1 as f32 / w as f32, y1 as f32 / h as f32];
    let area = |c: &(usize, usize, usize, usize)| ((c.2 - c.0) * (c.3 - c.1)) as f32;
    let mut areas: Vec<f32> = cells.iter().map(area).collect();
    areas.sort_by(|a, b| a.total_cmp(b));
    let median = areas[areas.len() / 2];
    let top = cells.iter().position(|c| area(c) > median * 1.3);
    let views = cells.iter().enumerate().filter(|(i, _)| Some(*i) != top).map(|(_, c)| uv(*c)).collect();
    Some((views, top.map(|i| uv(cells[i]))))
}

/// A composite map's quad as a rectangle (u0, v0, u1, v1).
fn quad_rect(q: &[f32; 8]) -> [f32; 4] {
    let (us, vs) = ([q[0], q[2], q[4], q[6]], [q[1], q[3], q[5], q[7]]);
    [us.iter().copied().fold(1.0, f32::min), vs.iter().copied().fold(1.0, f32::min), us.iter().copied().fold(0.0, f32::max), vs.iter().copied().fold(0.0, f32::max)]
}

/// The average linear colour of what's solid in a picture's rectangles.
pub fn average(img: &s3formats::dds::Rgba, rects: &[[f32; 4]]) -> Option<[f32; 3]> {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (mut sum, mut n) = ([0.0f32; 3], 0usize);
    for r in rects {
        let (x0, y0) = ((r[0] * img.width as f32) as usize, (r[1] * img.height as f32) as usize);
        let (x1, y1) = (((r[2] * img.width as f32) as usize).min(img.width), ((r[3] * img.height as f32) as usize).min(img.height));
        for y in y0..y1 {
            for x in x0..x1 {
                let p = &img.data[(y * img.width + x) * 4..][..4];
                if p[3] > 128 {
                    for k in 0..3 {
                        sum[k] += lin(p[k]);
                    }
                    n += 1;
                }
            }
        }
    }
    (n > 16).then(|| sum.map(|s| s / n as f32))
}

/// What the leaves' own colour's multiplied by to look as the billboard does (each channel's
/// ratio, kept within reason).
pub fn leaf_tint(billboard: &[f32; 3], leaves: &[f32; 3]) -> Option<[f32; 3]> {
    let t = [0, 1, 2].map(|k| (billboard[k] / leaves[k].max(1e-4)).clamp(0.6, 3.5));
    t.iter().all(|v| v.is_finite()).then_some(t)
}

/// How many bands of height a tree's outline is measured in.
pub const ENVELOPE_BANDS: usize = 24;

/// A tree's outline from its 360° billboard: in bands of height from its foot to its top, how
/// far it reaches out from its trunk (of its height), the views round it averaged.
pub fn envelope(img: &s3formats::dds::Rgba, views: &[[f32; 4]]) -> Vec<f32> {
    let (w, h) = (img.width, img.height);
    let solid = |x: usize, y: usize| img.data[(y * w + x) * 4 + 3] > 96;
    let mut sum = vec![0.0f32; ENVELOPE_BANDS];
    let mut n = 0;
    for v in views {
        let (x0, y0, x1, y1) = ((v[0] * w as f32) as usize, (v[1] * h as f32) as usize, ((v[2] * w as f32) as usize).min(w), ((v[3] * h as f32) as usize).min(h));
        if x1 <= x0 + 2 || y1 <= y0 + 4 {
            continue;
        }
        let tall = (y1 - y0) as f32;
        // (The trunk: the middle of what's solid in its lowest rows.)
        let foot: Vec<usize> = (y1.saturating_sub(((y1 - y0) / 30).max(2))..y1).flat_map(|y| (x0..x1).filter(move |&x| solid(x, y))).collect();
        let mid = if foot.is_empty() { (x0 + x1) as f32 * 0.5 } else { foot.iter().sum::<usize>() as f32 / foot.len() as f32 + 0.5 };
        let mut band = vec![0.0f32; ENVELOPE_BANDS];
        for y in y0..y1 {
            let b = (((y1 - 1 - y) as f32 / tall) * ENVELOPE_BANDS as f32) as usize;
            let reach = (x0..x1).filter(|&x| solid(x, y)).map(|x| (x as f32 + 0.5 - mid).abs()).fold(0.0, f32::max);
            band[b.min(ENVELOPE_BANDS - 1)] = band[b.min(ENVELOPE_BANDS - 1)].max(reach / tall);
        }
        for (s, b) in sum.iter_mut().zip(band) {
            *s += b;
        }
        n += 1;
    }
    if n == 0 {
        return Vec::new();
    }
    sum.iter().map(|s| s / n as f32).collect()
}

/// Finds the separate tree pictures in a billboard atlas: connected opaque regions.
/// Returns uv rectangles [u0, v0, u1, v1] of side views (those with a trunk at the bottom).
fn find_views(img: &s3formats::dds::Rgba, side_only: bool) -> Vec<[f32; 4]> {
    let (w, h) = (img.width, img.height);
    const CELL: usize = 4;
    let (cw, ch) = (w.div_ceil(CELL), h.div_ceil(CELL));
    let mut solid = vec![false; cw * ch];
    for y in 0..h {
        for x in 0..w {
            if img.data[(y * w + x) * 4 + 3] > 96 {
                solid[(y / CELL) * cw + x / CELL] = true;
            }
        }
    }
    let mut seen = vec![false; cw * ch];
    let mut blobs = Vec::new();
    for start in 0..cw * ch {
        if !solid[start] || seen[start] {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(c) = stack.pop() {
            let (cx, cy) = (c % cw, c / cw);
            (x0, y0, x1, y1, n) = (x0.min(cx), y0.min(cy), x1.max(cx), y1.max(cy), n + 1);
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                let (nx, ny) = (cx as i64 + dx, cy as i64 + dy);
                if nx < 0 || ny < 0 || nx >= cw as i64 || ny >= ch as i64 {
                    continue;
                }
                let ni = ny as usize * cw + nx as usize;
                if solid[ni] && !seen[ni] {
                    seen[ni] = true;
                    stack.push(ni);
                }
            }
        }
        // Ignore crumbs: views cover a decent part of the atlas.
        if n >= 12 && (x1 - x0 + 1) * CELL >= w / 10 && (y1 - y0 + 1) * CELL >= h / 10 {
            blobs.push((x0 * CELL, y0 * CELL, ((x1 + 1) * CELL).min(w), ((y1 + 1) * CELL).min(h)));
        }
    }
    // Side views stand on a trunk: their bottom rows are much narrower than the crown.
    let row_width = |y: usize, x0: usize, x1: usize| (x0..x1).filter(|&x| img.data[(y * w + x) * 4 + 3] > 96).count();
    let side: Vec<_> = blobs
        .iter()
        .copied()
        .filter(|&(x0, y0, x1, y1)| {
            let widest = (y0..y1).map(|y| row_width(y, x0, x1)).max().unwrap_or(0).max(1);
            let base = (y1.saturating_sub((y1 - y0) / 12).max(y0)..y1).map(|y| row_width(y, x0, x1)).max().unwrap_or(0);
            (base as f32) < widest as f32 * 0.4
        })
        .collect();
    let chosen = if side.is_empty() || !side_only { blobs } else { side };
    chosen
        .into_iter()
        .map(|(x0, y0, x1, y1)| [x0 as f32 / w as f32, y0 as f32 / h as f32, x1 as f32 / w as f32, y1 as f32 / h as f32])
        .collect()
}

/// Bakes the billboard of every SpeedTree kind in `kinds` (SpeedTree resource instances).
pub fn bake_tree_kinds(pkgs: &PackageSet, kinds: &[u64]) -> (Vec<TreeKindBaked>, Vec<(Key, bool)>) {
    let mut out = Vec::new();
    let mut textures = Vec::new();
    for &kind in kinds {
        let dbg = std::env::var_os("S3_DEBUG_TREES").is_some();
        let Some(spt) = pkgs.read_ti(T_SPEEDTREE, kind) else {
            if dbg { eprintln!("tree {kind:016X}: no speedtree resource") }
            continue;
        };
        let names = texture_names(&spt);
        // Trees have 360° billboards; flowers and shrubs only their leaf-card composite.
        let billboard = names.iter().find(|n| n.to_ascii_lowercase().ends_with("_composite_billboards_d"));
        let is_billboard = billboard.is_some();
        let Some(bb_name) = billboard.or_else(|| names.iter().find(|n| n.to_ascii_lowercase().ends_with("_composite_d"))) else {
            if dbg { eprintln!("tree {kind:016X}: no billboard name in {names:?}") }
            continue;
        };
        let tex = s3pkg::ResourceKey::new(s3pkg::types::DDS, 0, fnv64(&bb_name.to_ascii_lowercase()));
        let Some(dds) = pkgs.read(&tex).or_else(|| pkgs.read_ti(tex.t, tex.i)) else {
            if dbg { eprintln!("tree {kind:016X}: billboard {bb_name} ({:016X}) not found", tex.i) }
            continue;
        };
        let Some(img) = s3formats::dds::decode(&dds, 4096) else { continue };
        // A tree's 360° billboard: every view round it and the one from above, cut apart along
        // the atlas's gutters (shrubs and flowers: their leaf cards).
        let (views, top, round) = match is_billboard.then(|| round_views(&img)).flatten() {
            Some((v, t)) => (v, t, true),
            None => (find_views(&img, is_billboard), None, false),
        };
        if views.is_empty() {
            continue;
        }
        let (mn, mx) = pkgs.read_ti(T_TREE_INFO, kind).and_then(|d| tree_bounds(&d)).unwrap_or(([-3.0, 0.0, -3.0], [3.0, 8.0, 3.0]));
        let key = key_of(&tex);
        textures.push((key, false));
        // (A tree's parameters, to grow it up close, with its bark's and leaves' pictures.)
        let speedtree = round.then(|| crate::spt::SpeedTree::parse(&spt)).flatten();
        let tex_of = |stem: &str| {
            let k = s3pkg::ResourceKey::new(s3pkg::types::DDS, 0, fnv64(stem));
            (!stem.is_empty() && (pkgs.read(&k).is_some() || pkgs.read_ti(k.t, k.i).is_some())).then(|| key_of(&k))
        };
        let bark = speedtree.as_ref().and_then(|t| tex_of(&t.bark));
        let composite = speedtree.as_ref().and_then(|t| tex_of(&t.composite));
        for k in [bark, composite].into_iter().flatten() {
            textures.push((k, false));
        }
        let envelope = if speedtree.is_some() { envelope(&img, &views) } else { Vec::new() };
        // (The leaves' colour, as the billboard has them lit, over the composite map's own: the
        // grown tree's leaves made as bright as its pictures are.)
        let leaf_tint = speedtree.as_ref().zip(composite).and_then(|(t, _)| {
            let k = s3pkg::ResourceKey::new(s3pkg::types::DDS, 0, fnv64(&t.composite));
            let comp = pkgs.read(&k).or_else(|| pkgs.read_ti(k.t, k.i)).and_then(|d| s3formats::dds::decode(&d, 512))?;
            let rects: Vec<[f32; 4]> = t.leaf_quads.iter().map(|q| quad_rect(q)).collect();
            leaf_tint(&average(&img, &views)?, &average(&comp, &rects)?)
        });
        out.push(TreeKindBaked {
            kind,
            billboard: key,
            views,
            top,
            round,
            atlas_aspect: img.width as f32 / img.height.max(1) as f32,
            height: mx[1] - mn[1].min(0.0),
            radius: ((mx[0] - mn[0]).max(mx[2] - mn[2])) * 0.5,
            name: bb_name.clone(),
            speedtree: speedtree.filter(|_| composite.is_some()),
            bark,
            composite,
            envelope,
            leaf_tint,
        });
    }
    (out, textures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_are_brightened_to_their_billboard_within_reason() {
        let img = s3formats::dds::Rgba { width: 2, height: 2, data: vec![255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0] };
        let a = average(&img, &[[0.0, 0.0, 1.0, 1.0]]);
        assert!(a.is_none(), "too little that's solid");
        let t = leaf_tint(&[0.2, 0.3, 0.05], &[0.1, 0.1, 0.1]).unwrap();
        assert_eq!(t, [2.0, 3.0, 0.6]);
        assert_eq!(leaf_tint(&[1.0, 1.0, 1.0], &[0.01, 0.01, 0.01]).unwrap(), [3.5; 3]);
    }

    #[test]
    fn a_quad_is_its_corners_extent() {
        assert_eq!(quad_rect(&[0.9925, 0.4925, 0.5075, 0.4925, 0.5075, 0.0075, 0.9925, 0.0075]), [0.5075, 0.0075, 0.9925, 0.4925]);
    }
}
