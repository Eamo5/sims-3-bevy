//! Lot buildings: walls, floors, foundations and furniture of the pre-built houses, converted
//! from the lot's wall/room graphs and placed objects into a compact baked form.

use std::collections::HashMap;

use s3formats::catalog::Complate;
use s3formats::lot::{LotBuildData, WallGraph, enclosed_floor};
use s3formats::lotdesign as ld;
use s3formats::objn::PlacedObject;
use s3formats::world::LotInfo;
use s3pkg::Package;

use crate::types::*;

/// Height of one storey and of a foundation (metres).
pub const LEVEL_HEIGHT: f32 = 3.0;
pub const FOUNDATION_HEIGHT: f32 = 0.75;

const OBJD: u32 = 0x319E4F1D;

/// What a room is used for, judged by its furniture; picks floor and wall styles.
fn room_kind_of(script: &str) -> Option<u8> {
    let s = script.to_ascii_lowercase();
    if s.contains("toilet") || s.contains("shower") || s.contains("bathtub") || s.contains("sinkpedestal") {
        Some(ROOM_BATH)
    } else if s.contains("fridge") || s.contains("stove") || s.contains("counter") || s.contains("dishwasher") {
        Some(ROOM_KITCHEN)
    } else if s.contains(".beds.") || s.contains("crib") {
        Some(ROOM_BED)
    } else {
        None
    }
}

/// A wall or floor covering to render at bake time.
pub struct CoverJob {
    pub key: Key,
    pub floor: bool,
    pub source: CoverSource,
}

pub enum CoverSource {
    /// One of the lot's own designs, with the resources its references resolve to.
    Design { complate: Complate, keys: Vec<s3pkg::ResourceKey> },
    /// A catalogue pattern (CWAL), drawn in its first swatch.
    Pattern(s3pkg::ResourceKey),
}

/// The lot's covering data and the coverings chosen so far.
struct Covers {
    lot: u64,
    refs: HashMap<u16, s3pkg::ResourceKey>,
    designs: HashMap<u16, ld::Design>,
    keys: Vec<Key>,
    index: HashMap<Key, u16>,
    jobs: Vec<CoverJob>,
}

impl Covers {
    fn load(pkg: &Package, lot: u64) -> Self {
        let read = |t: u32, g: u32| pkg.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| pkg.read(e).ok());
        let refs = read(0x05ED1226, 0).and_then(|d| s3formats::objn::parse_refs(&d).ok()).unwrap_or_default();
        let designs = read(ld::T_DESIGNS, 0).map(|d| ld::parse_designs(&d)).unwrap_or_default();
        Self { lot, refs, designs, keys: Vec::new(), index: HashMap::new(), jobs: Vec::new() }
    }

    /// The covering index for a design (by REFS index), falling back to a catalogue pattern.
    fn pick(&mut self, design: Option<u16>, pattern: Option<u16>, floor: bool) -> u16 {
        let group = if floor { 2 } else { 1 };
        if let Some(i) = design
            && let Some(d) = self.designs.get(&i)
        {
            let key = (T_COVER, group, s3pkg::fnv64(&format!("{:016x}:{i}", self.lot)));
            let keys: Vec<s3pkg::ResourceKey> = d.refs.iter().map(|r| self.refs.get(r).copied().unwrap_or(s3pkg::ResourceKey::new(0, 0, 0))).collect();
            let complate = d.complate.clone();
            return self.add(key, floor, || CoverSource::Design { complate, keys });
        }
        if let Some(k) = pattern.and_then(|i| self.refs.get(&i)).copied().filter(|k| k.t == s3formats::catalog::T_CWAL) {
            return self.add((T_COVER, group + 2, k.i), floor, || CoverSource::Pattern(k));
        }
        NO_COVER
    }

    fn add(&mut self, key: Key, floor: bool, source: impl FnOnce() -> CoverSource) -> u16 {
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = self.keys.len() as u16;
        self.keys.push(key);
        self.index.insert(key, i);
        self.jobs.push(CoverJob { key, floor, source: source() });
        i
    }
}

/// Whether `p` lies on the segment q→r (within a centimetre), and where along it (0..1).
fn along_segment(p: [f32; 2], q: [f32; 2], r: [f32; 2]) -> Option<f32> {
    let (dx, dz) = (r[0] - q[0], r[1] - q[1]);
    let len2 = dx * dx + dz * dz;
    if len2 < 1e-6 {
        return None;
    }
    let t = ((p[0] - q[0]) * dx + (p[1] - q[1]) * dz) / len2;
    let (cx, cz) = (q[0] + dx * t - p[0], q[1] + dz * t - p[1]);
    ((-0.01..=1.01).contains(&t) && cx * cx + cz * cz < 1e-4).then_some(t.clamp(0.0, 1.0))
}

/// The lot's staircases (0x04A09283: `u32 version, u32 n`, then per staircase `u32 ?, u16 REFS
/// index of its CSTR style, u64 guid, u32 dir, f32 x1, z1, u32 top level, f32 x2, z2, u32 bottom
/// level, u32 n railings × (u32, u16 CRAL, f32 x, z)`); their pieces are filled in later.
pub fn lot_stairs(pkg: &Package, lot: u64) -> Vec<StairBaked> {
    lot_stairs_with_ids(pkg, lot).into_iter().map(|(_, s)| s).collect()
}

/// A lot's staircases, each with the id of its stair object. (Version 6, from later packs: four
/// style references for each staircase and each railing, the first its own.)
pub fn lot_stairs_with_ids(pkg: &Package, lot: u64) -> Vec<(u64, StairBaked)> {
    let read = |t: u32, g: u32| pkg.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| pkg.read(e).ok());
    let (Some(d), Some(refs)) = (read(0x04A09283, 0), read(0x05ED1226, 0).and_then(|r| s3formats::objn::parse_refs(&r).ok())) else { return Vec::new() };
    let mut r = s3formats::util::Reader::new(&d);
    let mut out = Vec::new();
    let (Ok(version), Ok(n)) = (r.u32(), r.u32()) else { return out };
    for _ in 0..n.min(256) {
        let Ok(_) = r.u32() else { break };
        let Ok(style) = r.u16() else { break };
        if version >= 6 && r.skip(6).is_err() {
            break;
        }
        let (Ok(guid), Ok(dir)) = (r.u64(), r.u32()) else { break };
        let (Ok(x1), Ok(z1), Ok(top)) = (r.f32(), r.f32(), r.u32()) else { break };
        let (Ok(x2), Ok(z2), Ok(bottom)) = (r.f32(), r.f32(), r.u32()) else { break };
        let Ok(n_rails) = r.u32() else { break };
        let mut rails = Vec::new();
        for _ in 0..n_rails.min(64) {
            let (Ok(_), Ok(cral)) = (r.u32(), r.u16()) else { break };
            if version >= 6 && r.skip(6).is_err() {
                break;
            }
            let (Ok(x), Ok(z)) = (r.f32(), r.f32()) else { break };
            if let Some(k) = refs.get(&cral) {
                rails.push(StairRail { at: [x, z], style: key_of(k), rail: None, start: None, post: None });
            }
        }
        let Some(k) = refs.get(&style) else { continue };
        let lo = |a: f32, b: f32| a.min(b).max(0.0).round() as u16;
        let hi = |a: f32, b: f32| a.max(b).max(0.0).round() as u16;
        out.push((guid, StairBaked {
            style: key_of(k),
            min: [lo(x1, x2), lo(z1, z2)],
            max: [hi(x1, x2), hi(z1, z2)],
            dir: dir as u8,
            bottom: if bottom == u32::MAX { BELOW_GROUND } else { bottom.min(32) as u8 },
            top: top.min(32) as u8,
            flight: None,
            start: None,
            side: None,
            rails,
        }));
    }
    out
}

/// A style's (CSTR, CRAL: the same layout) VPXY models with their bounds: the TGI list sits at
/// the offset in its second word + 8, counted.
fn style_models(pkgs: &s3pkg::PackageSet, style: &s3pkg::ResourceKey) -> Vec<(Key, [f32; 3], [f32; 3])> {
    let Some(d) = pkgs.read(style).or_else(|| pkgs.read_ti(style.t, style.i)) else { return Vec::new() };
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let Some(at) = u32_at(4).map(|o| o as usize + 8) else { return Vec::new() };
    let n = u32_at(at).unwrap_or(0).min(32) as usize;
    let mut out = Vec::new();
    for i in 0..n {
        let o = at + 4 + i * 16;
        let (Some(t), Some(g), Some(lo), Some(hi)) = (u32_at(o), u32_at(o + 4), u32_at(o + 8), u32_at(o + 12)) else { break };
        if t != 0x736884F1 {
            continue;
        }
        let vk = s3pkg::ResourceKey::new(t, g, (hi as u64) << 32 | lo as u64);
        let Some(v) = pkgs.read(&vk).or_else(|| pkgs.read_ti(vk.t, vk.i)) else { continue };
        let Some(mk) = s3formats::model::vpxy_models(&v).into_iter().next() else { continue };
        let meshes = s3formats::model::load_model(pkgs, &mk).unwrap_or_default();
        if meshes.is_empty() {
            continue;
        }
        let (mut lo3, mut hi3) = ([f32::MAX; 3], [f32::MIN; 3]);
        for m in &meshes {
            for a in 0..3 {
                lo3[a] = lo3[a].min(m.bounds_min[a]);
                hi3[a] = hi3[a].max(m.bounds_max[a]);
            }
        }
        out.push((key_of(&mk), lo3, hi3));
    }
    out
}

/// A railing style's (CRAL 0x04C58103) pieces: a tile's sloped rail, the short first rail, and
/// the post (told apart by shape).
pub fn rail_pieces(pkgs: &s3pkg::PackageSet, cral: &s3pkg::ResourceKey) -> (Option<Key>, Option<Key>, Option<Key>) {
    let (mut rail, mut start, mut post) = (None, None, None);
    for (k, lo, hi) in style_models(pkgs, cral) {
        let (len, wide) = (hi[0] - lo[0], hi[2] - lo[2]);
        let slot = if len < 0.2 && wide < 0.2 {
            &mut post
        } else if len < 0.5 {
            &mut start
        } else {
            &mut rail
        };
        slot.get_or_insert(k);
    }
    (rail, start, post)
}

/// A stair style's (CSTR 0x049CA4CD) pieces: the models of the VPXYs it names (its TGI list is
/// at the offset in its second word, counted), told apart by shape: the side panel is thin, the
/// first step short, the flight a tile's run.
pub fn stair_pieces(pkgs: &s3pkg::PackageSet, cstr: &s3pkg::ResourceKey) -> (Option<Key>, Option<Key>, Option<Key>) {
    let Some(d) = pkgs.read(cstr).or_else(|| pkgs.read_ti(cstr.t, cstr.i)) else { return (None, None, None) };
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let Some(at) = u32_at(4).map(|o| o as usize + 8) else { return (None, None, None) };
    let n = u32_at(at).unwrap_or(0).min(32) as usize;
    let (mut flight, mut start, mut side) = (None, None, None);
    for i in 0..n {
        let o = at + 4 + i * 16;
        let (Some(t), Some(g), Some(lo), Some(hi)) = (u32_at(o), u32_at(o + 4), u32_at(o + 8), u32_at(o + 12)) else { break };
        if t != 0x736884F1 {
            continue;
        }
        let vk = s3pkg::ResourceKey::new(t, g, (hi as u64) << 32 | lo as u64);
        let Some(v) = pkgs.read(&vk).or_else(|| pkgs.read_ti(vk.t, vk.i)) else { continue };
        let Some(mk) = s3formats::model::vpxy_models(&v).into_iter().next() else { continue };
        let meshes = s3formats::model::load_model(pkgs, &mk).unwrap_or_default();
        let (mut lo3, mut hi3) = ([f32::MAX; 3], [f32::MIN; 3]);
        for m in &meshes {
            for a in 0..3 {
                lo3[a] = lo3[a].min(m.bounds_min[a]);
                hi3[a] = hi3[a].max(m.bounds_max[a]);
            }
        }
        if meshes.is_empty() {
            continue;
        }
        let (len, wide) = (hi3[0] - lo3[0], hi3[2] - lo3[2]);
        let slot = if wide < 0.4 {
            &mut side
        } else if len < 0.6 {
            &mut start
        } else {
            &mut flight
        };
        slot.get_or_insert(key_of(&mk));
    }
    (flight, start, side)
}

pub fn bake_building(
    pkg: &Package,
    lot_index: usize,
    lot: &LotInfo,
    objects: &[PlacedObject],
    hm: &s3formats::world::Heightmap,
    fences: Vec<FenceBaked>,
    containers: &HashMap<s3pkg::ResourceKey, Vec<s3formats::model::Slot>>,
) -> Option<(LotBuildingBaked, Vec<CoverJob>)> {
    let data = LotBuildData::load(pkg, lot.id).unwrap_or_default();
    let read = |t: u32, g: u32| pkg.find(&s3pkg::ResourceKey::new(t, g, lot.id)).and_then(|e| pkg.read(e).ok());
    let mut covers = Covers::load(pkg, lot.id);
    if data.walls.edges.is_empty() && objects.is_empty() {
        return None;
    }
    let (w, d) = (
        data.rooms.width.max(data.walls.width).max(lot.width + 1).saturating_sub(1),
        data.rooms.depth.max(data.walls.depth).max(lot.depth + 1).saturating_sub(1),
    );
    // The lot's level grids (0x0093D6D4, heights relative to the lot): the ground's index `g`
    // (1 when there's a level below it for pools and basements), and whether the house stands
    // on a foundation (the level above the ground is 0.75 m up under it).
    let terrain = s3formats::lot::LotTerrain::load(pkg, lot.id);
    // (A houseboat (Isla Paradiso's) meets no terrain, out on the water: its ground is its
    // deck, the level at the lot's own height, its hull going down from the deck's edge to
    // the water a storey under it.)
    let middle_of = |v: &[f32]| {
        let mut v = v.to_vec();
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let mut hull_y = None;
    let (ground_index, has_foundation) = match &terrain {
        Some(t) => {
            // (Where the lot's edge meets the world's terrain.)
            let fit = crate::ponds::ground_level(t, lot, hm);
            let g = match fit {
                Some((g, mid, _)) if (mid - lot.corner[1]).abs() <= 1.0 => g,
                None => 0,
                Some(_) => {
                    let deck = (0..t.levels.len()).min_by(|&a, &b| middle_of(&t.levels[a]).abs().total_cmp(&middle_of(&t.levels[b]).abs())).unwrap_or(0);
                    hull_y = Some(lot.corner[1] + middle_of(&t.levels[deck]) - LEVEL_HEIGHT - 0.3);
                    deck
                }
            };
            let f = t.levels.get(g + 1).is_some_and(|l| l.iter().filter(|h| (0.4..1.2).contains(*h)).count() >= 4);
            (g as u32, f)
        }
        None => (0, data.rooms.segments().any(|s| s.2 == 0)),
    };
    let ground = lot.corner[1];
    let base = ground + if has_foundation { FOUNDATION_HEIGHT } else { 0.0 };
    let top_level = data
        .walls
        .vertices
        .values()
        .chain(data.rooms.vertices.values())
        .map(|v| v.level)
        .filter(|&l| l < 64)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut levels = vec![ground];
    for l in 1..=top_level + 1 {
        levels.push(base + (l - 1) as f32 * LEVEL_HEIGHT);
    }
    // The floor grid, and each floor tile's storey and height. A tile's storey is its grid
    // level above the ground's, less one where its column stands on a foundation (the level
    // above the ground there a foundation's height up, not a storey's): a house can have a
    // room on a foundation beside rooms without one, whose upper storey is then on the same
    // grid level as the foundation room's floor (the Koffis', the Wainwrights').
    let floor_grid = read(ld::T_GRID, ld::G_FLOOR_GRID);
    let floor_grid = floor_grid.as_deref().and_then(|d| ld::Grid::parse(d).ok());
    let grid_level = |level: u32| if level == 0 { ground_index } else { ground_index + level + has_foundation as u32 - 1 };
    // (Storey → its tiles: x, z, triangle mask, height, grid level.)
    let mut storeys: std::collections::BTreeMap<u32, Vec<(u32, u32, u8, Option<f32>, u32)>> = std::collections::BTreeMap::new();
    // (The columns standing on a foundation.)
    let mut founded: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    if let Some(g) = floor_grid.as_ref() {
        let mask_at = |gl: u32, x: u32, z: u32| g.quad(gl, x, z).map(|q| (0..4).filter(|&t| q[t] != 0).fold(0u8, |m, t| m | (1 << t))).filter(|m| *m != 0);
        match terrain.as_ref() {
            Some(t) => {
                let gi = ground_index as usize;
                let rise = |x: u32, z: u32| t.levels.get(gi + 1).map(|_| (t.at(gi + 1, x as usize, z as usize), t.at(gi, x as usize, z as usize)));
                let local = |x: u32, z: u32| rise(x, z).is_some_and(|(up, here)| (0.3..2.4).contains(&(up - here)));
                // (Where the house meets the ground: the middle of the heights of the ground
                // level's floors not on a foundation. On a hillside (Cormorant's) that's up the
                // hill, not the lot's own level at its foot.)
                let mut on_ground: Vec<f32> = (0..g.width.min(w + 1))
                    .flat_map(|x| (0..g.depth.min(d + 1)).map(move |z| (x, z)))
                    .filter(|&(x, z)| mask_at(ground_index, x, z).is_some() && !local(x, z))
                    .map(|(x, z)| t.at(gi, x as usize, z as usize))
                    .collect();
                on_ground.sort_by(f32::total_cmp);
                let floor_ref = on_ground.get(on_ground.len() / 2).copied().unwrap_or(0.0);
                // (A foundation's height over the ground there; or, where the ground falls away
                // under a floor, a stilted house's (Twinbrook's), over the lot's own level, or a
                // floor out over a hillside a foundation's height over the house's ground floor.)
                let on_foundation = |x: u32, z: u32| {
                    local(x, z)
                        || rise(x, z).is_some_and(|(up, here)| {
                            up - here >= 2.4 && ((0.3..2.4).contains(&up) || (0.3..1.5).contains(&(up - floor_ref))) && mask_at(gi as u32 + 1, x, z).is_some()
                        })
                };
                for x in 0..g.width.min(w + 1) {
                    for z in 0..g.depth.min(d + 1) {
                        if on_foundation(x, z) {
                            founded.insert((x, z));
                        }
                    }
                }
                // (A floor over the level above the ground is one floor, on a foundation or not as
                // most of it is: one flat floor out over a hillside stands higher over the ground
                // at its foot than up the slope (Isla Paradiso's Ichtaca house), and isn't a
                // storey up there.)
                let (gw, gd) = (g.width.min(w + 1), g.depth.min(d + 1));
                let up_floor = |x: u32, z: u32| mask_at(gi as u32 + 1, x, z).is_some() && gi + 1 < t.levels.len();
                let mut seen: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
                for x in 0..gw {
                    for z in 0..gd {
                        if seen.contains(&(x, z)) || !up_floor(x, z) {
                            continue;
                        }
                        let mut part = vec![(x, z)];
                        seen.insert((x, z));
                        let mut i = 0;
                        while i < part.len() {
                            let (px, pz) = part[i];
                            i += 1;
                            let h = t.at(gi + 1, px as usize, pz as usize);
                            for (dx, dz) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                                let (nx, nz) = (px as i32 + dx, pz as i32 + dz);
                                if nx < 0 || nz < 0 || nx as u32 >= gw || nz as u32 >= gd {
                                    continue;
                                }
                                let (nx, nz) = (nx as u32, nz as u32);
                                if !seen.contains(&(nx, nz)) && up_floor(nx, nz) && (t.at(gi + 1, nx as usize, nz as usize) - h).abs() < 0.3 {
                                    seen.insert((nx, nz));
                                    part.push((nx, nz));
                                }
                            }
                        }
                        let on = part.iter().filter(|c| founded.contains(c)).count();
                        // (Only where it's split: a floor wholly on or off a foundation stays so.)
                        if on > 0 && on < part.len() {
                            if on * 2 >= part.len() {
                                founded.extend(part.iter().copied());
                            } else {
                                for c in &part {
                                    founded.remove(c);
                                }
                            }
                        }
                    }
                }
                let on_foundation = |x: u32, z: u32| founded.contains(&(x, z));
                // (Higher up, a tile over a floor is a storey up from it (the Koffis' second floor
                // over their foundation room and their third over the rooms beside it share a
                // grid level; the Altos' over a tall foundation). One with no floor under it is on
                // the one of its two storeys its height fits, the storey its grid level makes it
                // or one less on a foundation, by the heights of the storeys found below: a
                // balcony out past the foundation's edge is on the storey of the floor it leads
                // off (Wolff's).)
                let mut heights: std::collections::BTreeMap<u32, Vec<f32>> = std::collections::BTreeMap::new();
                let mut below: HashMap<(u32, u32), (u32, f32, u32)> = HashMap::new();
                let middle = |v: &Vec<f32>| {
                    let mut v = v.clone();
                    v.sort_by(f32::total_cmp);
                    v[v.len() / 2]
                };
                for gl in ground_index..g.levels.min(t.levels.len() as u32) {
                    let tiles: Vec<(u32, u32, u8, f32)> = (0..g.width.min(w + 1))
                        .flat_map(|x| (0..g.depth.min(d + 1)).map(move |z| (x, z)))
                        .filter_map(|(x, z)| Some((x, z, mask_at(gl, x, z)?, t.at(gl as usize, x as usize, z as usize))))
                        .collect();
                    // (First those a storey over a floor, then the rest by the heights with them.)
                    let over = |x: u32, z: u32, h: f32| {
                        below.get(&(x, z)).filter(|b| gl >= ground_index + 2 && b.2 + 1 == gl && h - b.1 > LEVEL_HEIGHT * 0.66).map(|&(s0, h0, _)| s0 + ((h - h0) / LEVEL_HEIGHT).round().max(1.0) as u32)
                    };
                    let mut found: Vec<(u32, u32, u8, f32, u32)> = tiles.iter().filter_map(|&(x, z, mask, h)| Some((x, z, mask, h, over(x, z, h)?))).collect();
                    let mut first = heights.clone();
                    for &(_, _, _, h, s) in &found {
                        first.entry(s).or_default().push(h);
                    }
                    let expected = |s: u32| first.get(&s).map(middle).or_else(|| first.get(&s.checked_sub(1)?).map(|v| middle(v) + LEVEL_HEIGHT));
                    for &(x, z, mask, h) in &tiles {
                        if over(x, z, h).is_some() {
                            continue;
                        }
                        let by_column = gl as i32 - ground_index as i32 + 1 - on_foundation(x, z) as i32;
                        let s = if gl >= ground_index + 2 {
                            let (lo, hi) = (gl - ground_index, gl - ground_index + 1);
                            // (The storey whose floor it's on or a little over: a platform raised
                            // a step over a storey, as on a foundation (Oasis Landing's and
                            // Bridgeport's half-storeys, 1.13 m up), is that storey's.)
                            let fits = [lo - 1, lo, hi]
                                .into_iter()
                                .filter(|s| *s >= 1)
                                .filter_map(|s| Some((s, h - expected(s)?)))
                                .filter(|(_, d)| (-0.3..LEVEL_HEIGHT * 0.8).contains(d))
                                .min_by(|a, b| a.1.total_cmp(&b.1));
                            match (fits, expected(lo), expected(hi)) {
                                (Some((s, _)), ..) => s as i32,
                                // (Either known, the other a storey from it.)
                                (None, Some(a), Some(b)) => (if (h - a).abs() <= (h - b).abs() { lo } else { hi }) as i32,
                                (None, Some(a), None) => (if h - a < LEVEL_HEIGHT * 0.5 { lo } else { hi }) as i32,
                                (None, None, Some(b)) => (if b - h < LEVEL_HEIGHT * 0.5 { hi } else { lo }) as i32,
                                (None, None, None) => by_column,
                            }
                        } else {
                            by_column
                        };
                        if s >= 0 {
                            found.push((x, z, mask, h, s as u32));
                        }
                    }
                    for &(x, z, mask, h, s) in &found {
                        storeys.entry(s).or_default().push((x, z, mask, Some(ground + h), gl));
                        below.insert((x, z), (s, h, gl));
                    }
                    let found: Vec<(u32, f32)> = found.into_iter().map(|(_, _, _, h, s)| (s, h)).collect();
                    // (STOREYDEBUG=<lot name part>: each grid level's storeys, counts and heights.)
                    if std::env::var("STOREYDEBUG").is_ok_and(|n| lot.internal_name.contains(&n)) {
                        let mut by: std::collections::BTreeMap<u32, Vec<f32>> = std::collections::BTreeMap::new();
                        for &(s, h) in &found {
                            by.entry(s).or_default().push(h);
                        }
                        let parts: Vec<String> = by.iter().map(|(s, v)| format!("s{s}:{}@{:.2}", v.len(), middle(v))).collect();
                        eprintln!("{} gl{gl}: {} (expected {:?})", lot.internal_name, parts.join(" "), (1..6).map(|s| expected(s).map(|e| (e * 100.0).round() / 100.0)).collect::<Vec<_>>());
                    }
                    for (s, h) in found {
                        heights.entry(s).or_default().push(h);
                    }
                }
            }
            None => {
                // (Without heights, by storey: the ground's paving only under a foundation.)
                for level in (if has_foundation { 0 } else { 1 })..=top_level {
                    let gl = grid_level(level);
                    for x in 0..g.width.min(w + 1) {
                        for z in 0..g.depth.min(d + 1) {
                            if let Some(mask) = mask_at(gl, x, z) {
                                storeys.entry(level).or_default().push((x, z, mask, None, gl));
                            }
                        }
                    }
                }
            }
        }
    }
    // (A platform raised on a storey's floor (Into the Future's) is on the storey's tiles: the
    // storey's floor there is the platform's top, the one under it hidden.)
    for v in storeys.values_mut() {
        let mut top: HashMap<(u32, u32), usize> = HashMap::new();
        for (i, c) in v.iter().enumerate() {
            let e = top.entry((c.0, c.1)).or_insert(i);
            if c.4 > v[*e].4 {
                *e = i;
            }
        }
        if top.len() < v.len() {
            let keep: std::collections::HashSet<usize> = top.into_values().collect();
            let mut i = 0;
            v.retain(|_| {
                i += 1;
                keep.contains(&(i - 1))
            });
        }
    }
    // Each wall's storey. A wall's level counts from the ground's grid level (below it, a pool's
    // or a basement's: not drawn), so a house without a foundation has its ground floor's walls
    // on level 0 (Condor's) and one on a foundation on level 1 (Capitola's). It stands on that
    // grid level's floors beside it, a storey with them; without any, a storey up a level from
    // the ground floor (less the foundation's level, on one).
    let tile_storey: HashMap<(u32, u32, u32), u32> = storeys.iter().flat_map(|(s, v)| v.iter().map(move |c| ((c.4, c.0, c.1), *s))).collect();
    let level_storey = |level: u32, near: &[(f32, f32)]| -> Option<u32> {
        let l = level as i32;
        if !(0..64).contains(&l) {
            return None;
        }
        if l == 0 {
            return Some(1);
        }
        let tiles: Vec<(u32, u32)> = near.iter().filter(|p| p.0 >= 0.0 && p.1 >= 0.0).map(|p| (p.0.floor() as u32, p.1.floor() as u32)).collect();
        tiles.iter().filter_map(|&(x, z)| tile_storey.get(&(ground_index + level, x, z)).copied()).min().or_else(|| {
            let on_foundation = if terrain.is_some() { tiles.iter().any(|t| founded.contains(t)) } else { has_foundation };
            Some(level + 1 - on_foundation as u32)
        })
    };
    let wall_storey = |level: u32, a: [f32; 2], b: [f32; 2]| {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        let (nx, nz) = (-dz / len * 0.3, dx / len * 0.3);
        let mid = ((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5);
        level_storey(level, &[(mid.0 + nx, mid.1 + nz), (mid.0 - nx, mid.1 - nz)])
    };
    // (Floors above the walls' storeys, roof terraces and balconies, are storeys too.)
    let top_level = top_level.max(storeys.keys().copied().max().unwrap_or(0)).max(data.walls.segments().filter_map(|s| wall_storey(s.2, s.0, s.1)).max().unwrap_or(0));
    while levels.len() < top_level as usize + 2 {
        let l = levels[levels.len() - 1];
        levels.push(l + LEVEL_HEIGHT);
    }
    let has_elevator = objects.iter().any(|o| o.script.as_deref().is_some_and(|s| s.contains("Elevator")));
    // Each storey's floor where its tiles are (the middle of their heights): most foundations
    // are 0.75 m up, but a house on a slope may stand higher (the Goths' manor is 1.64 m above
    // its lot), a ground floor without one is on the ground, and the furniture is placed at
    // the true height.
    for l in 1..levels.len() {
        let mut hs: Vec<f32> = storeys.get(&(l as u32)).map(|v| v.iter().filter_map(|c| c.3).collect()).unwrap_or_default();
        if hs.len() < 4 {
            continue;
        }
        hs.sort_by(f32::total_cmp);
        let h = hs[hs.len() / 2];
        // (Only where it's a storey's sensible height above the one below; or, on a lot with an
        // elevator, a floor at the top of a tower (Bridgeport's rooftop clubs and penthouses:
        // the storeys between are the tower's, not built), reached by the elevator.)
        let tower_top = l >= 2 && has_elevator && h > levels[l - 1] + LEVEL_HEIGHT * 2.0;
        let sensible = tower_top || if l == 1 { h > levels[0] - 0.1 } else { h > levels[l - 1] + 0.3 && (h - levels[l - 1] - LEVEL_HEIGHT).abs() < 0.6 };
        if sensible {
            let shift = h - levels[l];
            for v in &mut levels[l..] {
                *v += shift;
            }
        }
    }
    // Floor heights by storey and tile (the furniture's storeys and the walls' footings).
    let tile_y: HashMap<(u32, u32, u32), f32> = storeys.iter().flat_map(|(s, v)| v.iter().filter_map(move |c| Some(((*s, c.0, c.1), c.3?)))).collect();

    // World <-> lot-local transform: world = corner + rotY(rotation) * (x, 0, z).
    let (s, c) = lot.rotation.sin_cos();
    let to_local = |p: [f32; 3]| {
        let (dx, dz) = (p[0] - lot.corner[0], p[2] - lot.corner[2]);
        [dx * c - dz * s, dx * s + dz * c]
    };

    // Furniture. Objects slotted into another (chairs at tables, sinks in counters) carry no
    // position of their own: put them by their parent instead.
    let by_guid: HashMap<u64, &PlacedObject> = objects.iter().map(|o| (o.guid, o)).collect();
    let mut sibling_count: HashMap<u64, usize> = HashMap::new();
    let mut objs = Vec::new();
    // (Each stair object's storey, by its id.)
    let mut stair_levels: HashMap<u64, u32> = HashMap::new();
    for o in objects {
        let (Some(cat), Some(mut p)) = (o.catalog, o.position) else { continue };
        // Trees on the lot are drawn with the world's trees. (Nor the game's jigs: the unseen
        // footprints it routes round, 21 of them about the Buckshots' pool table.)
        if cat.t != OBJD || !o.trees.is_empty() || o.speedtree.is_some() || o.script.as_deref().is_some_and(|s| s.ends_with(".Jig")) {
            continue;
        }
        let mut rotation = o.rotation;
        if p == [0.0, 0.0, 0.0] {
            let Some(parent) = by_guid.get(&o.parent).filter(|_| o.parent != 0) else { continue };
            let Some(pp) = parent.position.filter(|pp| *pp != [0.0, 0.0, 0.0]) else { continue };
            let script = o.script.as_deref().unwrap_or("").to_ascii_lowercase();
            let [qx, qy, qz, qw] = parent.rotation;
            // (Its place in the parent: the parent's container slot it's in, where the parent
            // has it (a dining table's nine), else round the parent as below.)
            let slot = parent.catalog.and_then(|k| containers.get(&k)).and_then(|v| v.iter().find(|s| s.name == o.parent_slot && o.parent_slot != 0));
            // Parent axes on the ground (rotation of +X and +Z by the parent quaternion).
            let rot = |v: [f32; 3]| -> [f32; 3] {
                let (x, y, z) = (v[0], v[1], v[2]);
                let (ix, iy, iz, iw) = (qw * x + qy * z - qz * y, qw * y + qz * x - qx * z, qw * z + qx * y - qy * x, -qx * x - qy * y - qz * z);
                [ix * qw + iw * -qx + iy * -qz - iz * -qy, iy * qw + iw * -qy + iz * -qx - ix * -qz, iz * qw + iw * -qz + ix * -qy - iy * -qx]
            };
            if let Some(s) = slot {
                // (Where the slot is in the parent, turned the way it faces: a chair's to the
                // table.)
                let w = rot(s.pos);
                p = [pp[0] + w[0], pp[1] + w[1], pp[2] + w[2]];
                let yaw = 2.0 * qy.atan2(qw) + s.rot[0][2].atan2(s.rot[2][2]);
                rotation = [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()];
            } else if script.contains("seating") || script.contains("chair") || script.contains("stool") {
                // Chairs go round the table: front, back, left, right.
                let k = sibling_count.entry(o.parent).or_default();
                let (off, face) = match *k % 4 {
                    0 => ([0.0, 0.0, -0.75], 0.0f32),
                    1 => ([0.0, 0.0, 0.75], std::f32::consts::PI),
                    2 => ([-0.75, 0.0, 0.0], std::f32::consts::FRAC_PI_2),
                    _ => ([0.75, 0.0, 0.0], -std::f32::consts::FRAC_PI_2),
                };
                *k += 1;
                let w = rot(off);
                p = [pp[0] + w[0], pp[1], pp[2] + w[2]];
                // Parent yaw plus the turn to face the table.
                let yaw = 2.0 * qy.atan2(qw) + face;
                rotation = [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()];
            } else {
                p = pp;
                rotation = parent.rotation;
            }
        }
        // Its storey: the highest floor on its tile at or just under it (or, for what stands on
        // a tile's edge, as a painting on a wall, on a tile beside it), else by the storeys'
        // heights.
        let l = to_local(p);
        let on = |dx: f32, dz: f32| {
            let (tx, tz) = ((l[0] + dx).floor(), (l[1] + dz).floor());
            (tx >= 0.0 && tz >= 0.0).then(|| (0..=top_level + 1).rev().find(|s| tile_y.get(&(*s, tx as u32, tz as u32)).is_some_and(|y| p[1] >= y - 0.3))).flatten()
        };
        let on_tile = on(0.0, 0.0)
            .or_else(|| [(-0.3, 0.0), (0.3, 0.0), (0.0, -0.3), (0.0, 0.3)].iter().filter_map(|&(dx, dz)| on(dx, dz)).max())
            .or_else(|| [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)].iter().filter_map(|&(dx, dz)| on(dx, dz)).max());
        let by_height = levels.iter().enumerate().skip(1).filter(|(_, y)| p[1] >= **y - 0.3).map(|(s, _)| s as u32).last().unwrap_or(0);
        let mut level = on_tile.unwrap_or(by_height) as u8;
        if o.script.as_deref().is_some_and(|s| s.ends_with(".Stairs")) {
            stair_levels.insert(o.guid, level as u32);
        }
        // Ceiling lights hang from the floor above but light (and belong to) the room below.
        if level > 1 && o.script.as_deref().unwrap_or("").contains("LightingCeiling") {
            level -= 1;
        }
        // Its own design, drawn with the walls' and floors'. (Many name a legacy pattern,
        // "OLD\defaultWood", but carry the patterns they're drawn with all the same.)
        let design = o.design.as_ref().map(|(c, keys)| {
            let canon = c.canonical(keys);
            let key = (crate::types::T_LOT_DESIGN, 0, s3pkg::fnv64(&canon));
            covers.jobs.push(CoverJob { key, floor: true, source: CoverSource::Design { complate: c.clone(), keys: keys.clone() } });
            key
        });
        objs.push(LotObjectBaked {
            objd: key_of(&cat),
            position: p,
            rotation,
            script: o.script.clone().unwrap_or_default(),
            level,
            local: to_local(p),
            design,
        });
    }

    // Floors: per storey, the tiles laid on its grid level (storey `n` is grid level
    // `g + n + F - 1`: on a foundation the ground floor is its own level above the ground);
    // indoors where walls enclose them, porches and paving elsewhere.
    let mut floors = Vec::new();
    let floor_palette = read(ld::T_FLOOR_PALETTE, ld::G_FLOOR_PALETTE).and_then(|d| ld::parse_floor_palette(&d).ok()).unwrap_or_default();
    // (A basement's tiles, on the level below the ground inside walls there: a pool has none.)
    // (The later packs' pools are walled at level -1 as basements are, and both have the game's
    // pool helper: a walled region is a basement where something stands in it that isn't a
    // pool's (a ladder, a light), a pool where nothing does.)
    let basement_cells: Vec<s3formats::lot::FloorCell> = {
        let edges: Vec<([f32; 2], [f32; 2])> = data.walls.segments().filter(|s| s.2 as i32 == -1).map(|s| (s.0, s.1)).collect();
        let cells = if edges.is_empty() { Vec::new() } else { enclosed_floor(&edges, w, d) };
        let region_of: HashMap<(u32, u32), u32> = cells.iter().map(|c| ((c.x, c.z), c.region)).collect();
        let furnished: std::collections::HashSet<u32> = objs
            .iter()
            .filter(|o| o.position[1] < ground - 1.0 && !o.script.contains(".Pools.") && !o.script.contains("Pool"))
            .filter_map(|o| (o.local[0] >= 0.0 && o.local[1] >= 0.0).then(|| region_of.get(&(o.local[0] as u32, o.local[1] as u32)).copied()).flatten())
            .collect();
        cells.into_iter().filter(|c| furnished.contains(&c.region)).collect()
    };
    let basement: std::collections::HashSet<(u32, u32)> = basement_cells.iter().map(|c| (c.x, c.z)).collect();
    // A walk-out lower floor (Bridgeport's hillside Wolff house): furnished rooms on the ground's
    // grid level a storey under the house's ground floor, walled on that level, dug into the
    // hill. They're the house's storey 0 as a basement's rooms are, each tile at its own height.
    // (Storey 0's tiles there: mask, height, grid level.)
    let walkout: HashMap<(u32, u32), (u8, f32, u32)> = if !basement_cells.is_empty() || terrain.is_none() {
        HashMap::new()
    } else {
        let low: HashMap<(u32, u32), (u8, f32, u32)> = storeys
            .get(&0)
            .into_iter()
            .flatten()
            .filter(|c| c.4 == ground_index)
            .filter_map(|c| Some(((c.0, c.1), (c.2, c.3?, c.4))))
            .filter(|(_, (_, y, _))| *y < levels[1] - LEVEL_HEIGHT * 0.6)
            .collect();
        let edges: Vec<([f32; 2], [f32; 2])> = if low.is_empty() { Vec::new() } else { data.walls.segments().filter(|s| s.2 == 0).map(|s| (s.0, s.1)).collect() };
        let cells = if edges.is_empty() { Vec::new() } else { enclosed_floor(&edges, w, d) };
        let region_of: HashMap<(u32, u32), u32> = cells.iter().filter(|c| low.contains_key(&(c.x, c.z))).map(|c| ((c.x, c.z), c.region)).collect();
        let furnished: std::collections::HashSet<u32> = objs
            .iter()
            .filter(|o| o.level == 0 && o.position[1] < levels[1] - LEVEL_HEIGHT * 0.5)
            .filter_map(|o| (o.local[0] >= 0.0 && o.local[1] >= 0.0).then(|| region_of.get(&(o.local[0] as u32, o.local[1] as u32)).copied()).flatten())
            .collect();
        region_of.into_iter().filter(|(_, r)| furnished.contains(r)).filter_map(|(c, _)| Some((c, *low.get(&c)?))).collect()
    };
    // (A pool's tiles, on the level below the ground: the ground's outdoor tiles over them are
    // its water's surface, not paving.)
    let pool_below: std::collections::HashSet<(u32, u32)> = match (ground_index, floor_grid.as_ref()) {
        (1.., Some(g)) => (0..g.width.min(w + 1))
            .flat_map(|x| (0..g.depth.min(d + 1)).map(move |z| (x, z)))
            .filter(|&(x, z)| g.quad(ground_index - 1, x, z).is_some_and(|q| q.iter().any(|v| *v != 0)) && !basement.contains(&(x, z)))
            .collect(),
        _ => Default::default(),
    };
    let mut floor_cover = |gl: u32, x: u32, z: u32| -> [u16; 4] {
        let Some(q) = floor_grid.as_ref().and_then(|g| g.quad(gl, x, z)) else { return [NO_COVER; 4] };
        q.map(|id| match floor_palette.get(&(id as u32)) {
            Some(&(cwal, comp)) if id != 0 => covers.pick(Some(comp), Some(cwal), true),
            _ => NO_COVER,
        })
    };
    for level in 1..=top_level {
        let bounds: Vec<([f32; 2], [f32; 2])> = data
            .rooms
            .segments()
            .filter(|s| s.2 == level || (level == 1 && s.2 == 0))
            .map(|s| (s.0, s.1))
            .collect();
        let wall_edges: Vec<([f32; 2], [f32; 2])> = data.walls.segments().filter(|s| wall_storey(s.2, s.0, s.1) == Some(level)).map(|s| (s.0, s.1)).collect();
        // (Without a level grid, the floor is wherever the rooms' boundaries enclose.)
        let cells: Vec<(u32, u32, u8, Option<f32>, u32)> = if floor_grid.is_some() {
            storeys.get(&level).cloned().unwrap_or_default()
        } else {
            enclosed_floor(&bounds, w, d).iter().map(|c| (c.x, c.z, c.mask, None, 0)).collect()
        };
        let inside = enclosed_floor(&wall_edges, w, d);
        let indoor: HashMap<(u32, u32), (u8, u32)> = inside.iter().map(|c| ((c.x, c.z), (c.mask, c.region))).collect();
        // Room kinds from the furniture standing in each indoor region.
        let mut kinds: HashMap<u32, u8> = HashMap::new();
        for o in objs.iter().filter(|o| o.level == level as u8) {
            let (x, z) = (o.local[0].floor() as i64, o.local[1].floor() as i64);
            if x < 0 || z < 0 {
                continue;
            }
            if let (Some(&(_, region)), Some(k)) = (indoor.get(&(x as u32, z as u32)), room_kind_of(&o.script)) {
                let e = kinds.entry(region).or_insert(k);
                // Bathrooms win over kitchens (sinks), kitchens over bedrooms.
                *e = (*e).min(k);
            }
        }
        for (x, z, mask, y, gl) in cells {
            let (kind, region) = match indoor.get(&(x, z)) {
                Some(&(m, region)) if m & mask != 0 => (kinds.get(&region).copied().unwrap_or(ROOM_LIVING), region),
                _ => (ROOM_PORCH, 0),
            };
            let cover = floor_cover(gl, x, z);
            // (A tile with no covering outdoors is no floor at all; nor one over a pool.)
            if kind == ROOM_PORCH && floor_grid.is_some() && (cover.iter().all(|&c| c == NO_COVER) || (gl == ground_index && pool_below.contains(&(x, z)))) {
                continue;
            }
            // (Its own height only where it isn't the storey's.)
            let y = y.filter(|y| (y - levels[level as usize]).abs() > 0.05);
            floors.push(FloorBaked { level: level as u8, x: x as u16, z: z as u16, mask, kind, region: region as u16, cover, y });
        }
    }

    // Ground-level paving (paths, patios) laid on the lot's ground where the house stands on a
    // foundation (elsewhere the ground level is the ground floor's, done above).
    for &(x, z, mask, _, gl) in storeys.get(&0).into_iter().flatten() {
        let cover = floor_cover(gl, x, z);
        if cover.iter().all(|&c| c == NO_COVER) || pool_below.contains(&(x, z)) || walkout.contains_key(&(x, z)) {
            continue;
        }
        floors.push(FloorBaked { level: 0, x: x as u16, z: z as u16, mask, kind: ROOM_PORCH, region: 0, cover, y: None });
    }

    // A pool: the tiles of the level below the ground (and how deep it is).
    let (pool, pool_depth) = match (ground_index, floor_grid.as_ref(), terrain.as_ref()) {
        (1.., Some(g), Some(t)) => {
            let below = ground_index - 1;
            let mut tiles = Vec::new();
            for x in 0..g.width.min(w + 1) {
                for z in 0..g.depth.min(d + 1) {
                    let Some(q) = g.quad(below, x, z) else { continue };
                    let mask = (0..4).filter(|&t| q[t] != 0).fold(0u8, |m, t| m | (1 << t));
                    if mask == 0 || basement.contains(&(x, z)) {
                        continue;
                    }
                    let cover = q.map(|id| match floor_palette.get(&(id as u32)) {
                        Some(&(cwal, comp)) if id != 0 => covers.pick(Some(comp), Some(cwal), true),
                        _ => NO_COVER,
                    });
                    tiles.push(FloorBaked { level: 0, x: x as u16, z: z as u16, mask, kind: ROOM_OUTSIDE, region: 0, cover, y: None });
                }
            }
            let depth = tiles.iter().map(|f| t.at(below as usize, f.x as usize, f.z as usize) - t.at(ground_index as usize, f.x as usize, f.z as usize)).sum::<f32>() / tiles.len().max(1) as f32;
            (tiles, depth)
        }
        _ => (Vec::new(), 0.0),
    };

    // A basement: the level below the ground inside its walls, storey 0 (each tile at its own
    // height, as the ground's paving has none), the rooms' kinds by their furniture; and its
    // floor's height.
    let mut basement_y = None;
    if let (1.., Some(g), Some(t)) = (ground_index, floor_grid.as_ref(), terrain.as_ref()) {
        let below = ground_index - 1;
        let mut kinds: HashMap<u32, u8> = HashMap::new();
        let region_of: HashMap<(u32, u32), u32> = basement_cells.iter().map(|c| ((c.x, c.z), c.region)).collect();
        for o in objs.iter().filter(|o| o.level == 0 && o.position[1] < ground - 1.0) {
            let (x, z) = (o.local[0].floor(), o.local[1].floor());
            if x >= 0.0 && z >= 0.0
                && let (Some(&region), Some(k)) = (region_of.get(&(x as u32, z as u32)), room_kind_of(&o.script))
            {
                let e = kinds.entry(region).or_insert(k);
                *e = (*e).min(k);
            }
        }
        let mut hs = Vec::new();
        for c in &basement_cells {
            let Some(q) = g.quad(below, c.x, c.z) else { continue };
            let mask = (0..4).filter(|&k| q[k] != 0).fold(0u8, |m, k| m | (1 << k)) & c.mask;
            if mask == 0 {
                continue;
            }
            let cover = q.map(|id| match floor_palette.get(&(id as u32)) {
                Some(&(cwal, comp)) if id != 0 => covers.pick(Some(comp), Some(cwal), true),
                _ => NO_COVER,
            });
            let y = ground + t.at(below as usize, c.x as usize, c.z as usize);
            hs.push(y);
            let kind = kinds.get(&c.region).copied().unwrap_or(ROOM_LIVING);
            floors.push(FloorBaked { level: 0, x: c.x as u16, z: c.z as u16, mask, kind, region: c.region as u16, cover, y: Some(y) });
        }
        if !hs.is_empty() {
            hs.sort_by(f32::total_cmp);
            basement_y = Some(hs[hs.len() / 2]);
        }
    }
    if !walkout.is_empty() {
        let edges: Vec<([f32; 2], [f32; 2])> = data.walls.segments().filter(|s| s.2 == 0).map(|s| (s.0, s.1)).collect();
        let region_of: HashMap<(u32, u32), u32> = enclosed_floor(&edges, w, d).iter().map(|c| ((c.x, c.z), c.region)).collect();
        let mut kinds: HashMap<u32, u8> = HashMap::new();
        for o in objs.iter().filter(|o| o.level == 0) {
            let (x, z) = (o.local[0].floor(), o.local[1].floor());
            if x >= 0.0 && z >= 0.0
                && walkout.contains_key(&(x as u32, z as u32))
                && let (Some(&region), Some(k)) = (region_of.get(&(x as u32, z as u32)), room_kind_of(&o.script))
            {
                let e = kinds.entry(region).or_insert(k);
                *e = (*e).min(k);
            }
        }
        let mut hs = Vec::new();
        for (&(x, z), &(mask, y, gl)) in &walkout {
            let region = region_of.get(&(x, z)).copied().unwrap_or(0);
            let kind = kinds.get(&region).copied().unwrap_or(ROOM_LIVING);
            hs.push(y);
            let cover = floor_grid.as_ref().and_then(|g| g.quad(gl, x, z)).map_or([NO_COVER; 4], |q| {
                q.map(|id| match floor_palette.get(&(id as u32)) {
                    Some(&(cwal, comp)) if id != 0 => covers.pick(Some(comp), Some(cwal), true),
                    _ => NO_COVER,
                })
            });
            floors.push(FloorBaked { level: 0, x: x as u16, z: z as u16, mask, kind, region: region as u16, cover, y: Some(y) });
        }
        hs.sort_by(f32::total_cmp);
        basement_y = Some(hs[hs.len() / 2]);
    }

    // Walls with the kind of room on each side (outdoors when no indoor floor is there). (On
    // storey 0, the basement's floors, not the ground's paving.)
    let floor_at: HashMap<(u8, u16, u16), (u8, u8)> = floors.iter().filter(|f| f.level > 0 || f.y.is_some()).map(|f| ((f.level, f.x, f.z), (f.mask, f.kind))).collect();
    let basement_floor_y: HashMap<(u16, u16), f32> = floors.iter().filter(|f| f.level == 0).filter_map(|f| Some(((f.x, f.z), f.y?))).collect();
    let has_floor = |level: u32, p: [f32; 2]| -> bool {
        p[0] >= 0.0 && p[1] >= 0.0 && floor_at.contains_key(&(level as u8, p[0].floor() as u16, p[1].floor() as u16))
    };
    let side_kind = |level: u32, p: [f32; 2]| -> u8 {
        let (x, z) = (p[0].floor(), p[1].floor());
        if x < 0.0 || z < 0.0 {
            return ROOM_OUTSIDE;
        }
        match floor_at.get(&(level as u8, x as u16, z as u16)) {
            Some(&(mask, kind)) if kind != ROOM_PORCH => {
                // Which of the tile's triangles holds the point.
                let (fx, fz) = (p[0] - x - 0.5, p[1] - z - 0.5);
                let t = if fz.abs() > fx.abs() { if fz < 0.0 { 0 } else { 2 } } else if fx > 0.0 { 1 } else { 3 };
                if mask & (1 << t) != 0 { kind } else { ROOM_OUTSIDE }
            }
            _ => ROOM_OUTSIDE,
        }
    };
    let mut walls = Vec::new();
    // The wall graph names the room on each side (0 = outdoors). Which side is "left" is
    // settled per lot by agreement with where the floors are.
    let normal_of = |a: [f32; 2], b: [f32; 2]| {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        [-dz / len, dx / len]
    };
    // A foundation's sides, in the wall graph of the later packs' lots (its level-0 edges, the
    // rooms graph's foundation outline, with no floor beside them on their own level): drawn
    // as the foundation, not as walls (they'd stand round its decks and porches, doorless).
    let unit_edges = |a: [f32; 2], b: [f32; 2]| -> Vec<((i32, i32), (i32, i32))> {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let steps = dx.abs().max(dz.abs()).round() as i32;
        let (sx, sz) = if steps == 0 { (0, 0) } else { ((dx / steps as f32).round() as i32, (dz / steps as f32).round() as i32) };
        let (ax, az) = (a[0].round() as i32, a[1].round() as i32);
        (0..steps)
            .map(|s| {
                let (p, q) = ((ax + sx * s, az + sz * s), (ax + sx * (s + 1), az + sz * (s + 1)));
                if p < q { (p, q) } else { (q, p) }
            })
            .collect()
    };
    let foundation_edges: std::collections::HashSet<((i32, i32), (i32, i32))> = data.rooms.segments().filter(|s| s.2 == 0).flat_map(|s| unit_edges(s.0, s.1)).collect();
    let foundation_side = |grid_level: u32, a: [f32; 2], b: [f32; 2]| -> bool {
        if grid_level != 0 || foundation_edges.is_empty() {
            return false;
        }
        let edges = unit_edges(a, b);
        if edges.is_empty() || !edges.iter().all(|e| foundation_edges.contains(e)) {
            return false;
        }
        let n = normal_of(a, b);
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let floored = |s: f32, gl: u32| {
            let p = [mid[0] + n[0] * 0.3 * s, mid[1] + n[1] * 0.3 * s];
            p[0] >= 0.0 && p[1] >= 0.0 && floor_grid.as_ref().is_some_and(|g| g.quad(gl, p[0] as u32, p[1] as u32).is_some_and(|q| q.iter().any(|v| *v != 0)))
        };
        // (On a houseboat's deck, floored all round, the rim of a platform raised on it: its
        // floor on one side only.)
        if hull_y.is_some() && has_foundation {
            return floored(1.0, ground_index + 1) != floored(-1.0, ground_index + 1);
        }
        !floored(1.0, ground_index) && !floored(-1.0, ground_index)
    };
    let mut agree = 0i32;
    for (a, b, level, e) in data.walls.segments() {
        if foundation_side(level, a, b) {
            continue;
        }
        if (e.left == 0) == (e.right == 0) {
            continue;
        }
        let Some(level) = wall_storey(level, a, b) else { continue };
        let n = normal_of(a, b);
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let plus = has_floor(level, [mid[0] + n[0] * 0.3, mid[1] + n[1] * 0.3]);
        let minus = has_floor(level, [mid[0] - n[0] * 0.3, mid[1] - n[1] * 0.3]);
        if plus != minus {
            // Convention "left = +normal": the outdoor room is on the side without floor.
            agree += if (e.left == 0) == !plus { 1 } else { -1 };
        }
    }
    let left_is_plus = agree >= 0;
    let cover_graph = read(0x312E7545, 0x2E7B1E).and_then(|d| WallGraph::parse(&d).ok()).unwrap_or_default();
    let side_ids = |g: u32| -> HashMap<u32, ld::WallSides> {
        read(ld::T_WALL_SIDES, g).and_then(|d| ld::parse_wall_sides(&d).ok()).unwrap_or_default().into_iter().map(|s| (s.edge, s)).collect()
    };
    let (design_sides, pattern_sides) = (side_ids(ld::G_WALL_DESIGNS), side_ids(ld::G_WALL_PATTERNS));
    let palette = |g: u32| read(ld::T_PALETTE, g).and_then(|d| ld::parse_palette(&d).ok()).unwrap_or_default();
    let (design_palette, pattern_palette) = (palette(ld::G_WALL_DESIGN_PALETTE), palette(ld::G_WALL_PATTERN_PALETTE));
    let cover_edges: Vec<([f32; 2], [f32; 2], u32, u32)> = cover_graph.segments().map(|(a, b, l, e)| (a, b, l, e.id)).collect();
    for (a, b, grid_level, e) in data.walls.segments() {
        // (Walls below the ground are a basement's, storey 0, or a pool's, not drawn.)
        if foundation_side(grid_level, a, b) {
            continue;
        }
        let walkout_wall = grid_level == 0 && !walkout.is_empty() && {
            let n = normal_of(a, b);
            let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            [1.0f32, -1.0].iter().any(|s| {
                let p = [mid[0] + n[0] * 0.3 * s, mid[1] + n[1] * 0.3 * s];
                p[0] >= 0.0 && p[1] >= 0.0 && walkout.contains_key(&(p[0] as u32, p[1] as u32))
            })
        };
        let level = if (grid_level == u32::MAX && basement_y.is_some()) || walkout_wall {
            0
        } else {
            let Some(level) = wall_storey(grid_level, a, b) else { continue };
            level
        };
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dz * dz).sqrt();
        if len < 1e-3 {
            continue;
        }
        let n = normal_of(a, b);
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let side = |sign: f32| [mid[0] + n[0] * 0.3 * sign, mid[1] + n[1] * 0.3 * sign];
        // Upper-storey "walls" with no floor on either side are beams over open structures.
        if level > 1 && !has_floor(level, side(1.0)) && !has_floor(level, side(-1.0)) {
            continue;
        }
        let (room_plus, room_minus) = if left_is_plus { (e.left, e.right) } else { (e.right, e.left) };
        let kind = |sign: f32, room: u32| {
            if room == 0 {
                ROOM_OUTSIDE
            } else {
                match side_kind(level, side(sign)) {
                    ROOM_OUTSIDE | ROOM_PORCH => ROOM_LIVING,
                    k => k,
                }
            }
        };
        let (left, right) = (kind(1.0, room_plus), kind(-1.0, room_minus));
        // (A basement's walls stand on its floor beside them.)
        let wall_y = (level == 0)
            .then(|| {
                [side(1.0), side(-1.0)]
                    .iter()
                    .filter(|p| p[0] >= 0.0 && p[1] >= 0.0)
                    .filter_map(|p| basement_floor_y.get(&(p[0].floor() as u16, p[1].floor() as u16)).copied())
                    .reduce(f32::min)
                    .or(basement_y)
            })
            .flatten();
        // The covering edges on this wall, in order along it. Side A of a covering edge is
        // its left (by the wall graph's rooms, taken in the edge's direction).
        let mut pieces: Vec<(f32, f32, [u16; 2])> = Vec::new();
        for &(ca, cb, cl, id) in &cover_edges {
            if cl != grid_level {
                continue;
            }
            let (Some(t0), Some(t1)) = (along_segment(ca, a, b), along_segment(cb, a, b)) else { continue };
            if (t1 - t0).abs() * len < 0.05 {
                continue;
            }
            let side = |sides: &HashMap<u32, ld::WallSides>, pal: &HashMap<u32, u16>| {
                let s = sides.get(&id)?;
                let (l, r) = if t1 > t0 { (s.a, s.b) } else { (s.b, s.a) };
                Some((l.and_then(|i| pal.get(&i)).copied(), r.and_then(|i| pal.get(&i)).copied()))
            };
            let d = side(&design_sides, &design_palette).unwrap_or((None, None));
            let p = side(&pattern_sides, &pattern_palette).unwrap_or((None, None));
            let (cl, cr) = (covers.pick(d.0, p.0, false), covers.pick(d.1, p.1, false));
            // Left/right of the wall graph edge → this wall's +normal / -normal sides.
            let (cplus, cminus) = if left_is_plus { (cl, cr) } else { (cr, cl) };
            pieces.push((t0.min(t1), t0.max(t1), [cplus, cminus]));
        }
        pieces.sort_by(|x, y| x.0.total_cmp(&y.0));
        let at = |t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        if pieces.is_empty() {
            walls.push(WallBaked { a, b, level: level as u8, left, right, cover: [NO_COVER; 2], y: wall_y });
        } else {
            // Gaps between covering edges keep the wall uncovered there.
            let mut t = 0.0f32;
            for (t0, t1, cover) in pieces {
                if t0 > t + 0.01 {
                    walls.push(WallBaked { a: at(t), b: at(t0), level: level as u8, left, right, cover: [NO_COVER; 2], y: wall_y });
                }
                if t1 > t0.max(t) + 0.01 {
                    walls.push(WallBaked { a: at(t0.max(t)), b: at(t1), level: level as u8, left, right, cover, y: wall_y });
                }
                t = t.max(t1);
            }
            if t < 0.99 {
                walls.push(WallBaked { a: at(t), b, level: level as u8, left, right, cover: [NO_COVER; 2], y: wall_y });
            }
        }
    }
    // A houseboat's hull: down from its deck's outer edge to under the waterline.
    if let (Some(y), Some(g)) = (hull_y, floor_grid.as_ref()) {
        let deck = |x: i32, z: i32| x >= 0 && z >= 0 && g.quad(ground_index, x as u32, z as u32).is_some_and(|q| q.iter().any(|v| *v != 0));
        for x in 0..g.width as i32 {
            for z in 0..g.depth as i32 {
                if !deck(x, z) {
                    continue;
                }
                let (fx, fz) = (x as f32, z as f32);
                // (Each side with no deck beyond it, the wall's outside to the left of a to b.)
                for (dx, dz, a, b) in [(0, -1, [fx + 1.0, fz], [fx, fz]), (1, 0, [fx + 1.0, fz + 1.0], [fx + 1.0, fz]), (0, 1, [fx, fz + 1.0], [fx + 1.0, fz + 1.0]), (-1, 0, [fx, fz], [fx, fz + 1.0])] {
                    if !deck(x + dx, z + dz) {
                        walls.push(WallBaked { a, b, level: 0, left: ROOM_OUTSIDE, right: ROOM_OUTSIDE, cover: [NO_COVER; 2], y: Some(y) });
                    }
                }
            }
        }
    }
    // The floor either side of a wall's middle on its storey: where a room on a foundation
    // stands beside rooms without one, its walls (and its foundation's sides) go up from its
    // floor.
    let beside = |level: u32, a: [f32; 2], b: [f32; 2]| -> Vec<f32> {
        let n = normal_of(a, b);
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        [1.0f32, -1.0]
            .iter()
            .filter_map(|s| {
                let p = [mid[0] + n[0] * 0.3 * s, mid[1] + n[1] * 0.3 * s];
                (p[0] >= 0.0 && p[1] >= 0.0).then(|| tile_y.get(&(level, p[0].floor() as u32, p[1].floor() as u32)).copied()).flatten()
            })
            .collect()
    };
    for wb in walls.iter_mut().filter(|w| w.level > 0) {
        let level = wb.level as u32;
        if let Some(y) = beside(level, wb.a, wb.b).into_iter().reduce(f32::min).filter(|y| (y - levels[level as usize]).abs() > 0.05) {
            wb.y = Some(y);
        }
    }
    let foundation: Vec<([f32; 2], [f32; 2])> =
        if has_foundation { data.rooms.segments().filter(|s| s.2 == 0).map(|s| (s.0, s.1)).collect() } else { Vec::new() };
    let foundation_top: Vec<f32> = foundation.iter().map(|&(a, b)| beside(1, a, b).into_iter().reduce(f32::max).unwrap_or(levels[1])).collect();
    let foundation_top = if foundation_top.iter().any(|y| (y - levels[1]).abs() > 0.05) { foundation_top } else { Vec::new() };

    // Fences and railings. Their levels count as the walls' do: a balcony's railing is on its
    // floor's storey (on the ground, 0). A run on the edge of a wall on its level, or the level
    // up, is the edge of the foundation under that wall (the fence graph holds the rooms'
    // edges too), not a fence: it would stand in the wall and across its doors.
    let unit_edges = |a: [f32; 2], b: [f32; 2]| -> Vec<((i32, i32), (i32, i32))> {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let steps = dx.abs().max(dz.abs()).round() as i32;
        if steps == 0 {
            return Vec::new();
        }
        let (sx, sz) = ((dx / steps as f32).round() as i32, (dz / steps as f32).round() as i32);
        let (ax, az) = (a[0].round() as i32, a[1].round() as i32);
        (0..steps)
            .map(|s| {
                let (p, q) = ((ax + sx * s, az + sz * s), (ax + sx * (s + 1), az + sz * (s + 1)));
                if p < q { (p, q) } else { (q, p) }
            })
            .collect()
    };
    let wall_edges: std::collections::HashSet<((i32, i32), (i32, i32), u32)> =
        data.walls.segments().flat_map(|(a, b, l, _)| unit_edges(a, b).into_iter().map(move |(p, q)| (p, q, l))).collect();
    let fences: Vec<FenceBaked> = fences
        .into_iter()
        .filter_map(|mut f| {
            let raw = f.level as u32;
            if f.a != f.b && unit_edges(f.a, f.b).iter().all(|&(p, q)| wall_edges.contains(&(p, q, raw)) || wall_edges.contains(&(p, q, raw + 1))) {
                return None;
            }
            if raw >= 1 {
                let mid = ((f.a[0] + f.b[0]) * 0.5, (f.a[1] + f.b[1]) * 0.5);
                let (dx, dz) = (f.b[0] - f.a[0], f.b[1] - f.a[1]);
                let len = (dx * dx + dz * dz).sqrt();
                let near: Vec<(f32, f32)> = if len > 1e-3 {
                    let (nx, nz) = (-dz / len * 0.3, dx / len * 0.3);
                    vec![(mid.0 + nx, mid.1 + nz), (mid.0 - nx, mid.1 - nz)]
                } else {
                    [(0.3, 0.3), (-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3)].iter().map(|(u, v)| (mid.0 + u, mid.1 + v)).collect()
                };
                f.level = level_storey(raw, &near)? as u8;
            }
            Some(f)
        })
        .collect();
    let jobs = std::mem::take(&mut covers.jobs);
    Some((
        LotBuildingBaked {
            stairs: {
                // (Each staircase from the storey its stair object stands on, where the record's
                // levels don't count the storeys as the house does: a tower's rooftop floors.)
                let mut v = lot_stairs_with_ids(pkg, lot.id);
                for (guid, s) in &mut v {
                    // (The record's levels count as the walls' do: on their storeys, by the
                    // floors at either end; from the ground (level 0) only onto the ground floor.)
                    let d = s.climb();
                    let half = s.run_width().0 as f32 * 0.5 + 0.5;
                    let mid = ((s.min[0] + s.max[0]) as f32 * 0.5, (s.min[1] + s.max[1]) as f32 * 0.5);
                    let end = |k: f32| (mid.0 + d[0] * k, mid.1 + d[1] * k);
                    if std::env::var("STOREYDEBUG").is_ok_and(|n| lot.internal_name.contains(&n)) {
                        let at = |k: f32| {
                            let p = end(k);
                            (0..8u32).filter_map(|gl| tile_storey.get(&(gl, p.0.floor() as u32, p.1.floor() as u32)).map(|s| (gl, *s))).collect::<Vec<_>>()
                        };
                        eprintln!("{} stairs {:?}-{:?} raw {}->{}: bottom end {:?} (grid, storey), top end {:?}", lot.internal_name, s.min, s.max, s.bottom, s.top, at(-half), at(half));
                    }
                    // (Up from a basement, its storey 0.)
                    if s.bottom == BELOW_GROUND {
                        if basement_y.is_some()
                            && let Some(top) = level_storey(s.top as u32, &[end(half)])
                        {
                            (s.bottom, s.top) = (0, top as u8);
                        }
                    } else if s.top > s.bottom {
                        if let Some(top) = level_storey(s.top as u32, &[end(half)]) {
                            let bottom = if s.bottom == 0 { (top > 1) as u32 } else { level_storey(s.bottom as u32, &[end(-half)]).unwrap_or(s.bottom as u32) };
                            // (Or both on one storey: the steps up onto a platform raised on it.)
                            if top >= bottom {
                                (s.bottom, s.top) = (bottom as u8, top as u8);
                            }
                        }
                    }
                    let far_up = |l: u32| has_elevator && levels.get(l as usize).zip(levels.get(s.bottom as usize)).is_some_and(|(a, b)| a - b > LEVEL_HEIGHT * 2.0);
                    if let Some(o) = stair_levels.get(guid).copied().filter(|l| *l as u8 != s.bottom && far_up(*l)) {
                        let span = s.top.saturating_sub(s.bottom).max(1);
                        s.bottom = o as u8;
                        s.top = o as u8 + span;
                    }
                }
                // (Those from below the ground with no basement drawn, a pool's, go.)
                v.into_iter().map(|(_, s)| s).filter(|s| s.bottom != BELOW_GROUND).collect()
            },
            lot: lot_index as u32,
            width: w,
            depth: d,
            levels,
            walls,
            floors,
            foundation,
            foundation_top,
            objects: objs,
            covers: covers.keys,
            ground: Vec::new(),
            pool,
            pool_depth,
            fences,
            basement: basement_y,
        },
        jobs,
    ))
}
