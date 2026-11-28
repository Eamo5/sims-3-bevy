//! SpeedTree RT 4 parameter files (the game's `SPT2` resources, `__IdvSpt_02_`): the token
//! stream read (the layout as `spttools`' FORMAT has it), and what's wanted of it for growing the
//! tree: the trunk's and branch levels' shapes, the leaves' textures and placement, the fronds,
//! and the composite map's leaf pictures.

use serde::{Deserialize, Serialize};

/// One token's payload.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    None,
    Int(i32),
    Float(f32),
    Byte(u8),
    Str(String),
    Floats(Vec<f32>),
}

/// What follows a token (`sptparser`'s reads).
enum Layout {
    None,
    Int,
    Float,
    Byte,
    Str,
    Floats(usize),
    /// A count, then that many groups of this many floats.
    Counted(usize),
}

fn layout(t: u32) -> Option<Layout> {
    use Layout::*;
    Some(match t {
        1000 | 2000 | 4003 | 19002 | 6000..=6007 | 6017 | 13005 | 14002 | 18005 | 20002 | 26013 | 26014 | 26018 | 26019 | 26020 | 26022 | 60006 | 70002..=70008 | 71003 => Str,
        1006 | 1014 | 2004 | 2005 | 3001 | 3008 | 6008 | 6009 | 8002 | 8004 | 8007 | 8008 | 9002 | 9007 | 9011 | 11002 | 13002 | 13003 | 13004 | 13006 | 13008 | 13009 | 14007 | 14008 | 16003
        | 16013 | 25003..=25006 | 26008 | 26012 | 27004 | 28004 | 29002 | 40002 | 60007 | 60008 | 71001 | 71004 | 71012 | 71013 | 72003 => Int,
        2001 | 2003 | 2006 | 2007 | 3000 | 3002 | 3004 | 3005 | 3007 | 3010 | 4002 | 4007 | 5005 | 6010..=6014 | 8006 | 9003 | 9004 | 9008..=9010 | 9012..=9014 | 13010..=13013 | 14003..=14006 | 15003
        | 16002 | 16004..=16012 | 16014 | 21000 | 21001 | 23002 | 23003 | 25002 | 26004..=26007 | 26010 | 26011 | 26015..=26017 | 26021 | 27003 | 27005 | 27006 | 28003 | 30002..=30009 | 40003..=40005
        | 50004 | 50005 | 50008 | 50010 | 50013..=50017 | 72005 | 72006 | 73004..=73006 | 74002 | 75002 | 75003 | 75005 => Float,
        2002 | 3003 | 3006 | 3009 | 4000 | 5006 | 6015 | 6016 | 13007 | 15002 | 20003 | 20004 | 22000 | 25007 | 26009 | 26023 | 27002 | 28002 | 50006 | 50007 | 50009 | 50011 | 50012 | 50018 | 72001 | 75004 => Byte,
        4001 | 4004..=4006 | 5000..=5004 | 18002..=18004 | 71006..=71009 => Floats(3),
        71010 => Floats(2),
        8003 | 8005 | 8009 => Floats(13),
        12002 => Floats(4),
        12003 => Floats(5),
        12004 => Floats(6),
        20005 => Floats(8),
        10002..=10004 => Counted(8),
        1001..=1005 | 1007..=1012 | 1015..=1017 | 7000 | 7001 | 8000 | 8001 | 9000 | 9001 | 9005 | 9006 | 10000 | 10001 | 11000 | 11001 | 12000 | 12001 | 13000 | 13001 | 14000 | 14001 | 15000 | 15001
        | 16000 | 16001 | 18000 | 18001 | 19000 | 19001 | 20000 | 20001 | 25000 | 25001 | 26000..=26003 | 27000 | 27001 | 28000 | 28001 | 29000 | 29001 | 30000 | 30001 | 40000 | 40001 | 40006..=40008
        | 50000..=50003 | 60000..=60005 | 60009 | 70000 | 70001 | 71000 | 71002 | 71005 | 71011 | 71014 | 71015 | 72000 | 72004 | 73000..=73003 | 74000 | 74001 | 75000 | 75001 => None,
        _ => return Option::None,
    })
}

/// The tokens of a parameter file, in order; where it stops making sense (a token not known),
/// what was read up to there and the unknown token.
pub fn tokens(d: &[u8]) -> (Vec<(u32, Value)>, Option<(usize, u32)>) {
    let mut out = Vec::new();
    let mut p = 0usize;
    let u32_at = |p: usize| d.get(p..p + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let f32_at = |p: usize| u32_at(p).map(f32::from_bits);
    while p + 4 <= d.len() {
        let t = u32_at(p).unwrap();
        let Some(l) = layout(t) else { return (out, Some((p, t))) };
        p += 4;
        let v = match l {
            Layout::None => Value::None,
            Layout::Int => {
                let Some(v) = u32_at(p) else { break };
                p += 4;
                Value::Int(v as i32)
            }
            Layout::Float => {
                let Some(v) = f32_at(p) else { break };
                p += 4;
                Value::Float(v)
            }
            Layout::Byte => {
                let Some(&v) = d.get(p) else { break };
                p += 1;
                Value::Byte(v)
            }
            Layout::Str => {
                let Some(n) = u32_at(p) else { break };
                let n = n as usize;
                let Some(s) = d.get(p + 4..p + 4 + n) else { break };
                p += 4 + n;
                Value::Str(String::from_utf8_lossy(s).into_owned())
            }
            Layout::Floats(n) => {
                let v: Option<Vec<f32>> = (0..n).map(|i| f32_at(p + i * 4)).collect();
                let Some(v) = v else { break };
                p += n * 4;
                Value::Floats(v)
            }
            Layout::Counted(k) => {
                let Some(n) = u32_at(p) else { break };
                let n = (n as usize).min(100_000);
                let v: Option<Vec<f32>> = (0..n * k).map(|i| f32_at(p + 4 + i * 4)).collect();
                let Some(v) = v else { break };
                p += 4 + n * k * 4;
                Value::Floats(v)
            }
        };
        out.push((t, v));
    }
    (out, None)
}

/// A SpeedTree curve (`BezierSpline min max variance { n  x y tx ty weight ... }`): a value
/// between `min` and `max` along a Bezier through its points (each with its tangent and the
/// tangent's length), give or take `variance`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Spline {
    pub min: f32,
    pub max: f32,
    pub variance: f32,
    pub points: Vec<[f32; 5]>,
}

impl Spline {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().strip_prefix("BezierSpline")?;
        let (head, rest) = s.split_once('{')?;
        let h: Vec<f32> = head.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let body: Vec<f32> = rest.trim_end_matches(|c: char| c == '}' || c.is_whitespace()).split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let n = *body.first()? as usize;
        let points = body[1..].chunks_exact(5).take(n).map(|c| [c[0], c[1], c[2], c[3], c[4]]).collect();
        Some(Self { min: *h.first()?, max: *h.get(1)?, variance: h.get(2).copied().unwrap_or(0.0), points })
    }

    /// The curve's height (0..1, as drawn) at `x` (0..1).
    pub fn curve(&self, x: f32) -> f32 {
        let p = &self.points;
        match p.len() {
            0 => return 1.0,
            1 => return p[0][1],
            _ => {}
        }
        let x = x.clamp(p[0][0], p[p.len() - 1][0]);
        let i = p.windows(2).position(|w| x <= w[1][0]).unwrap_or(p.len() - 2);
        let (a, b) = (p[i], p[i + 1]);
        let span = (b[0] - a[0]).max(1e-6);
        // (The tangents' lengths are of the span.)
        let (c0, c1) = ([a[0] + a[2] * a[4] * span, a[1] + a[3] * a[4] * span], [b[0] - b[2] * b[4] * span, b[1] - b[3] * b[4] * span]);
        let bez = |t: f32, p0: f32, p1: f32, p2: f32, p3: f32| {
            let u = 1.0 - t;
            u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
        };
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let m = (lo + hi) * 0.5;
            if bez(m, a[0], c0[0], c1[0], b[0]) < x {
                lo = m;
            } else {
                hi = m;
            }
        }
        bez((lo + hi) * 0.5, a[1], c0[1], c1[1], b[1])
    }

    /// Its value at `x`, `r` (-1..1) of its variance off.
    pub fn at(&self, x: f32, r: f32) -> f32 {
        self.min + (self.max - self.min) * self.curve(x) + self.variance * r
    }
}

/// A trunk or branch level (or the leaves', last): its shape along its parent and along itself,
/// and how its children grow on it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Level {
    pub disturbance: Spline,
    pub gravity: Spline,
    pub flexibility: Spline,
    pub flexibility_profile: Spline,
    /// The trunk's length (of the tree's size), a branch's (of its parent's), along its parent;
    /// the leaves': how far from the branch they're put.
    pub length: Spline,
    pub radius: Spline,
    /// Along itself: its thickness (of `radius`).
    pub radius_profile: Spline,
    /// Degrees off its parent (the trunk's: -90, upright).
    pub start_angle: Spline,
    pub cross_segments: i32,
    pub length_segments: i32,
    /// Where its children grow along it (0..1), and how many.
    pub first: f32,
    pub last: f32,
    pub frequency: f32,
    pub u_tiling: f32,
    pub v_tiling: f32,
    pub gravity_profile: Spline,
    /// Its children's spread along it (`Generation/Profile`), when given.
    pub frequency_profile: Option<Spline>,
    /// Whether its first children fork from it (`Fork`: their bias, angle and how many).
    pub fork: Option<(f32, f32, i32)>,
}

/// A leaf texture: its colour, where on it it hangs from, and its size.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LeafMap {
    pub color: [f32; 3],
    pub origin: [f32; 3],
    pub size: [f32; 3],
    pub name: String,
    pub blossom: bool,
}

/// What a tree is grown from.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SpeedTree {
    pub seed: i32,
    pub size: f32,
    pub size_variance: f32,
    pub bark: String,
    pub composite: String,
    /// The trunk, the branch levels, and last the leaves.
    pub levels: Vec<Level>,
    pub leaf_maps: Vec<LeafMap>,
    /// Each leaf map's picture in the composite map: four (u, v) corners.
    pub leaf_quads: Vec<[f32; 8]>,
    pub frond_quads: Vec<[f32; 8]>,
    /// The fronds: on which level (none when not enabled), how many blades each, and each
    /// frond map's size.
    pub frond_level: Option<i32>,
    pub frond_blades: i32,
    pub frond_sizes: Vec<f32>,
    /// Leaves turned and hung (`Hang(%)`, `Rotate`), each leaf map's.
    pub leaf_hang: Vec<f32>,
    pub leaf_rotate: Vec<f32>,
    /// The branches' and leaves' diffuse colours (`Lighting/Materials`).
    pub branch_color: [f32; 3],
    pub leaf_color: [f32; 3],
}

impl SpeedTree {
    pub fn parse(d: &[u8]) -> Option<Self> {
        let (toks, _) = tokens(d);
        if !matches!(toks.first(), Some((1000, Value::Str(s))) if s.starts_with("__IdvSpt")) {
            return None;
        }
        let mut t = SpeedTree::default();
        let mut level: Option<Level> = None;
        let mut leaf: Option<LeafMap> = None;
        let mut in_roots = false;
        let mut layer_block = 0u32;
        let mut prof_level = 0usize;
        let spline = |v: &Value| if let Value::Str(s) = v { Spline::parse(s).unwrap_or_default() } else { Spline::default() };
        let f = |v: &Value| if let Value::Float(x) = v { *x } else { 0.0 };
        let i = |v: &Value| if let Value::Int(x) = v { *x } else { 0 };
        let stem = |s: &str| {
            let l = s.to_ascii_lowercase();
            let l = l.rsplit(['\\', '/']).next().unwrap_or(&l).to_string();
            l.split(".tga").next().unwrap_or(&l).to_string()
        };
        for (k, v) in &toks {
            match *k {
                40000 => in_roots = true,
                40001 => in_roots = false,
                2000 => t.bark = if let Value::Str(s) = v { stem(s) } else { String::new() },
                2005 => t.seed = i(v),
                2006 => t.size = f(v),
                2007 => t.size_variance = f(v),
                1016 if !in_roots => level = Some(Level::default()),
                1017 => {
                    if let Some(l) = level.take()
                        && !in_roots
                    {
                        t.levels.push(l);
                    }
                }
                6000..=6017 if level.is_some() => {
                    let l = level.as_mut().unwrap();
                    match k {
                        6000 => l.disturbance = spline(v),
                        6001 => l.gravity = spline(v),
                        6002 => l.flexibility = spline(v),
                        6003 => l.flexibility_profile = spline(v),
                        6004 => l.length = spline(v),
                        6005 => l.radius = spline(v),
                        6006 => l.radius_profile = spline(v),
                        6007 => l.start_angle = spline(v),
                        6008 => l.cross_segments = i(v),
                        6009 => l.length_segments = i(v),
                        6010 => l.first = f(v),
                        6011 => l.last = f(v),
                        6012 => l.frequency = f(v),
                        6013 => l.u_tiling = f(v),
                        6014 => l.v_tiling = f(v),
                        6017 => l.gravity_profile = spline(v),
                        _ => {}
                    }
                }
                1007 => leaf = Some(LeafMap::default()),
                1008 => {
                    if let Some(l) = leaf.take() {
                        t.leaf_maps.push(l);
                    }
                }
                4000..=4007 if leaf.is_some() => {
                    let l = leaf.as_mut().unwrap();
                    let v3 = |v: &Value| if let Value::Floats(x) = v { [x[0], x[1], x[2]] } else { [0.0; 3] };
                    match k {
                        4000 => l.blossom = matches!(v, Value::Byte(1)),
                        4001 => l.color = v3(v),
                        4003 => l.name = if let Value::Str(s) = v { stem(s) } else { String::new() },
                        4004 => l.origin = v3(v),
                        4005 => l.size = v3(v),
                        _ => {}
                    }
                }
                10002 | 10004 => {
                    if let Value::Floats(x) = v {
                        let q = x.chunks_exact(8).map(|c| <[f32; 8]>::try_from(c).unwrap());
                        if *k == 10002 { t.leaf_quads.extend(q) } else { t.frond_quads.extend(q) }
                    }
                }
                13002 => t.frond_level = Some(i(v)),
                13004 => t.frond_blades = i(v),
                13007 => {
                    if matches!(v, Value::Byte(0)) {
                        t.frond_level = None;
                    }
                }
                14004 => t.frond_sizes.push(f(v)),
                20002 => t.composite = if let Value::Str(s) = v { stem(s) } else { String::new() },
                72005 => t.leaf_hang.push(f(v)),
                72006 => t.leaf_rotate.push(f(v)),
                8003 | 8005 => {
                    if let Value::Floats(x) = v {
                        let c = [x[0], x[1], x[2]];
                        if *k == 8003 { t.branch_color = c } else { t.leaf_color = c }
                    }
                }
                // (The branch levels' extras, in the levels' order: the generation profile is
                // the next level's spread on this one.)
                26000 => prof_level = 0,
                26002 => {}
                26003 => prof_level += 1,
                26009 if !in_roots => {
                    if let (Value::Byte(1), Some(l)) = (v, t.levels.get_mut(prof_level)) {
                        l.fork = Some((1.0, 0.0, 1));
                    }
                }
                26010..=26012 if !in_roots => {
                    if let Some((bias, angle, limit)) = t.levels.get_mut(prof_level).and_then(|l| l.fork.as_mut()) {
                        match k {
                            26010 => *bias = f(v),
                            26011 => *angle = f(v),
                            _ => *limit = i(v),
                        }
                    }
                }
                26019 if !in_roots => {
                    if let (Value::Str(s), Some(l)) = (v, t.levels.get_mut(prof_level)) {
                        l.frequency_profile = Spline::parse(s);
                    }
                }
                60005 => layer_block = 60005,
                60009 | 60006 => layer_block = 0,
                70002 if layer_block == 60005 && t.composite.is_empty() => t.composite = if let Value::Str(s) = v { stem(s) } else { String::new() },
                _ => {}
            }
        }
        Some(t)
    }
}

// ---- Growing the tree ----

use glam::Vec3;

/// A mesh: positions, normals, uvs and triangles.
#[derive(Clone, Debug, Default)]
pub struct Part {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// Leaf cards, as SpeedTree draws its leaves: each corner at its leaf's middle (`positions`),
/// with how far across the card it is (`corners`, x to the right and y up as seen: the card's
/// turned to face the view), its picture's uv, and the normal it's lit by (out from the crown).
#[derive(Clone, Debug, Default)]
pub struct Cards {
    pub positions: Vec<[f32; 3]>,
    pub corners: Vec<[f32; 2]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// A grown tree, in SpeedTree's units, standing on the origin, y up: its trunk and branches
/// (bark), its fronds and its leaves (both from the composite map), and its bounds.
#[derive(Clone, Debug, Default)]
pub struct Grown {
    pub bark: Part,
    pub fronds: Part,
    pub leaves: Cards,
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// How many of the bark's first vertices are the trunk's foot.
    pub trunk_ring: Option<usize>,
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn u(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn s(&mut self) -> f32 {
        self.u() * 2.0 - 1.0
    }
}

struct Branch {
    pts: Vec<Vec3>,
    radii: Vec<f32>,
    len: f32,
    level: usize,
}

impl Branch {
    /// Where along it `t` (0..1) is: the point, which way it runs there, and its radius.
    fn at(&self, t: f32) -> (Vec3, Vec3, f32) {
        let n = self.pts.len() - 1;
        let f = (t.clamp(0.0, 1.0) * n as f32).min(n as f32 - 1e-4);
        let i = f as usize;
        let k = f - i as f32;
        let (a, b) = (self.pts[i], self.pts[i + 1]);
        (a.lerp(b, k), (b - a).normalize_or(Vec3::Y), self.radii[i] + (self.radii[i + 1] - self.radii[i]) * k)
    }
}

/// Some direction at right angles to `d`.
fn perpendicular(d: Vec3) -> Vec3 {
    let o = if d.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    d.cross(o).normalize()
}

/// `d` turned `angle` (radians) about `axis`.
fn turn(d: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    glam::Quat::from_axis_angle(axis.normalize_or(Vec3::X), angle) * d
}

/// How hard gravity bends a branch over its length at full strength (a right angle).
const GRAVITY_BEND: f32 = std::f32::consts::FRAC_PI_2;
/// The most branches and leaves grown (SpeedTree's own levels of detail thin them so).
const BRANCH_BUDGET: f32 = 600.0;
/// Leaf cards' size, of their maps'.
const LEAF_SCALE: f32 = 0.75;

/// Grows the tree its parameters describe: the trunk upright, each branch level's branches
/// spread along their parents (between `first` and `last`, `frequency` of them on the longest,
/// fewer on shorter ones, turned round by the golden angle) at their start angles, as long as
/// their length curves have them (of the tree's size), bent by gravity and disturbed, tapering
/// by their radius profiles; the frond level's drawn as fronds, and the leaves put round the
/// last branch level as cards (no more than `leaf_budget` of them, the bigger for being fewer).
pub fn grow(t: &SpeedTree, leaf_budget: usize) -> Grown {
    let mut rng = Rng::new(t.seed as u64 ^ 0x5EED);
    let size = t.size.max(0.01);
    let has_leaves = !t.leaf_maps.is_empty() && !t.leaf_quads.is_empty() && t.levels.len() >= 2;
    let branch_levels = if has_leaves { t.levels.len() - 1 } else { t.levels.len() };
    let mut out = Grown::default();
    if branch_levels == 0 {
        return out;
    }
    // (The trunk: upright, as long as its length has it, its radius of the size.)
    let lv0 = &t.levels[0];
    let trunk_len = (size * lv0.length.at(0.0, rng.s() * 0.5)).max(size * 0.01);
    let trunk_r = (size * lv0.radius.at(0.0, 0.0)).abs().max(size * 0.002);
    let lean = perpendicular(Vec3::Y);
    let dir = turn(Vec3::Y, turn(lean, Vec3::Y, rng.u() * 6.283), rng.s() * 0.04);
    let mut levels: Vec<Vec<Branch>> = vec![vec![grow_branch(lv0, 0, Vec3::ZERO, dir, trunk_len, trunk_r, 0.0, &mut rng)]];
    // (How many of each level there'd be, to keep within the budget.)
    for l in 0..branch_levels - 1 {
        let parent = &t.levels[l];
        let child = &t.levels[l + 1];
        let max_len = levels[l].iter().map(|b| b.len).fold(1e-4, f32::max);
        let expected: f32 = levels[l].iter().map(|b| parent.frequency * (b.len / max_len)).sum();
        let thin = (BRANCH_BUDGET / expected.max(1.0)).min(1.0);
        let mut next = Vec::new();
        for b in &levels[l] {
            let n = (parent.frequency * (b.len / max_len) * thin).round().max(0.0) as usize;
            let base_r = b.radii.iter().copied().fold(0.0, f32::max);
            let mut roll = rng.u() * std::f32::consts::TAU;
            // (Forks: its first children split from it where its children stop, round it evenly,
            // as thick as it is there, turned less than the others are.)
            let forks = parent.fork.map_or(0, |f| f.2.max(0) as usize).min(n);
            for k in 0..forks {
                let (bias, fork_angle, _) = parent.fork.unwrap_or_default();
                let at = parent.last.clamp(0.0, 1.0);
                let (p, tan, r_at) = b.at(at);
                let side = turn(perpendicular(tan), tan, roll + k as f32 * std::f32::consts::TAU / forks as f32);
                let own = child.start_angle.at(0.5, rng.s());
                let angle = (own + (fork_angle - own) * (bias * 0.7).clamp(0.0, 1.0)).abs().max(12.0).to_radians();
                let dir = (tan * angle.cos() + side * angle.sin()).normalize_or(side);
                let len = size * child.length.max.max(child.length.min) * (0.85 + rng.u() * 0.15);
                next.push(grow_branch(child, l + 1, p, dir, len, r_at * 0.8, 0.5, &mut rng));
            }
            for k in forks..n {
                let at = parent.first + (parent.last - parent.first) * ((k as f32 + 0.5 + rng.s() * 0.4) / n as f32).clamp(0.0, 1.0);
                if let Some(p) = &parent.frequency_profile {
                    let span = (parent.last - parent.first).max(1e-4);
                    if rng.u() > p.curve(((at - parent.first) / span).clamp(0.0, 1.0)).clamp(0.05, 1.0) {
                        continue;
                    }
                }
                roll += 2.399_963 + rng.s() * 0.35;
                let (p, tan, r_at) = b.at(at);
                // (The child's curves run along where on its parent it can grow.)
                let x = ((at - parent.first) / (parent.last - parent.first).max(1e-4)).clamp(0.0, 1.0);
                let side = turn(perpendicular(tan), tan, roll);
                let angle = child.start_angle.at(x, rng.s()).to_radians();
                let dir = (tan * angle.cos() + side * angle.sin()).normalize_or(side);
                let len = size * child.length.at(x, rng.s());
                if len < size * 0.004 {
                    continue;
                }
                let r = (base_r * child.radius.at(x, rng.s()).abs()).min(r_at * 0.9).max(size * 0.0005);
                next.push(grow_branch(child, l + 1, p, dir, len, r, x, &mut rng));
            }
        }
        if next.is_empty() {
            break;
        }
        levels.push(next);
    }
    // (The tubes, and the fronds' blades.)
    let frond = t.frond_level.filter(|&f| f >= 1 && (f as usize) < levels.len()).map(|f| f as usize);
    let frond_quad = t.frond_quads.first().copied();
    for (l, bs) in levels.iter().enumerate() {
        for b in bs {
            match (frond, frond_quad) {
                (Some(f), Some(q)) if l >= f => blade(&mut out.fronds, b, q, t.frond_sizes.first().copied().unwrap_or(0.5)),
                _ => {
                    if l == 0 && out.bark.positions.is_empty() {
                        out.trunk_ring = Some(t.levels[0].cross_segments.clamp(3, 16) as usize + 1);
                    }
                    tube(&mut out.bark, b, &t.levels[b.level])
                }
            }
        }
    }
    // (The leaves, round the last branch level's.)
    if has_leaves && let Some(bs) = levels.get(branch_levels - 1).filter(|_| levels.len() == branch_levels) {
        let parent = &t.levels[branch_levels - 1];
        let place = &t.levels[branch_levels];
        let max_len = bs.iter().map(|b| b.len).fold(1e-4, f32::max);
        let expected: f32 = bs.iter().map(|b| parent.frequency * (b.len / max_len)).sum();
        let keep = (leaf_budget as f32 / expected.max(1.0)).min(1.0);
        let grow_by = keep.powf(-0.25).min(1.25);
        let (first, last) = (parent.first.min(parent.last), parent.last.max(parent.first + 0.3).min(1.0));
        for b in bs {
            let n = (parent.frequency * (b.len / max_len)).round() as usize;
            for k in 0..n {
                if rng.u() > keep {
                    continue;
                }
                let at = first + (last - first) * ((k as f32 + 0.5 + rng.s() * 0.5) / n.max(1) as f32).clamp(0.0, 1.0);
                let (p, tan, _) = b.at(at);
                let out_dir = turn(perpendicular(tan), tan, rng.u() * std::f32::consts::TAU);
                let centre = p + out_dir * size * place.length.at(at, rng.s());
                let m = (rng.u() * t.leaf_maps.len() as f32) as usize % t.leaf_maps.len();
                let map = &t.leaf_maps[m];
                let q = t.leaf_quads[m.min(t.leaf_quads.len() - 1)];
                // (A card's size is its leaf map's, a little smaller for SpeedTree's leaves' being
                // a little smaller than their maps.)
                let s = (1.0 + rng.s() * 0.15) * grow_by * size * LEAF_SCALE;
                card(&mut out.leaves, centre, q, map, s, rng.s() * 0.6);
            }
        }
        // (Lit as a rounded crown: each leaf facing out from the middle of them all.)
        let n = out.leaves.positions.len().max(1) as f32;
        let mid = out.leaves.positions.iter().fold(Vec3::ZERO, |a, p| a + Vec3::from(*p)) / n;
        for (p, nrm) in out.leaves.positions.iter().zip(out.leaves.normals.iter_mut()) {
            *nrm = ((Vec3::from(*p) - mid).normalize_or(Vec3::Y) + Vec3::Y * 0.6).normalize().to_array();
        }
    }
    let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in out.bark.positions.iter().chain(&out.fronds.positions).chain(&out.leaves.positions) {
        mn = mn.min(Vec3::from(*p));
        mx = mx.max(Vec3::from(*p));
    }
    // (The leaves reach their cards' size beyond their middles.)
    let reach = out.leaves.corners.iter().map(|c| c[0].abs().max(c[1].abs())).fold(0.0, f32::max);
    if !out.leaves.positions.is_empty() {
        mx += Vec3::splat(reach * 0.5);
        mn -= Vec3::new(reach * 0.5, 0.0, reach * 0.5);
    }
    if mn.x <= mx.x {
        (out.min, out.max) = (mn.to_array(), mx.to_array());
    }
    out
}

/// Fits a grown tree to its outline (`trees::envelope`, its reach out from the trunk in bands
/// of height, of its height): in each band its own reach drawn in or let out to the outline's,
/// so the tree up close has the crown its pictures from afar show.
pub fn fit(g: &mut Grown, env: &[f32]) {
    let trunk_ring = g.trunk_ring;
    let nb = env.len();
    let ht = g.max[1].max(1e-3);
    if nb < 2 || g.min[0] > g.max[0] {
        return;
    }
    let band = |y: f32| ((y / ht) * nb as f32 - 0.5).clamp(0.0, nb as f32 - 1.0);
    let reach_of = |g: &Grown| {
        let mut reach = vec![0.0f32; nb];
        let mut note = |p: [f32; 3], extra: f32| {
            let b = band(p[1]).round() as usize;
            reach[b] = reach[b].max((p[0] * p[0] + p[2] * p[2]).sqrt() + extra);
        };
        for p in g.bark.positions.iter().chain(&g.fronds.positions) {
            note(*p, 0.0);
        }
        for (p, c) in g.leaves.positions.iter().zip(&g.leaves.corners) {
            note(*p, c[0].abs());
        }
        reach
    };
    // (First the crown's foot raised or lowered to the outline's: where it first reaches out a
    // third as far as it does at its widest; the trunk below it stretched to suit.)
    let foot = |r: &[f32]| {
        let m = r.iter().copied().fold(0.0, f32::max);
        r.iter().position(|&v| v > m * 0.33).map(|b| b as f32 / nb as f32 * ht)
    };
    if let (Some(from), Some(to)) = (foot(&reach_of(g)), foot(env))
        && (from - to).abs() > ht * 0.03
        && to < ht * 0.9
    {
        let low = from < ht * 0.15;
        for p in g.bark.positions.iter_mut().chain(g.fronds.positions.iter_mut()).chain(g.leaves.positions.iter_mut()) {
            // (A crown down to the ground lifted whole, the trunk's foot let down to it again.)
            p[1] = if low || p[1] >= from { to + (p[1] - from) * (ht - to) / (ht - from) } else { p[1] * to / from };
            p[1] = p[1].max(0.0);
        }
        if low && let Some(sides) = trunk_ring {
            let lift = g.bark.positions[0][1];
            for (p, uv) in g.bark.positions.iter_mut().zip(g.bark.uvs.iter_mut()).take(sides) {
                p[1] = 0.0;
                uv[1] += lift * 0.05;
            }
        }
    }
    let reach = reach_of(g);
    let mut s: Vec<Option<f32>> = (0..nb).map(|b| (reach[b] > ht * 0.01 && env[b] > 0.0).then(|| (env[b] * ht / reach[b]).clamp(0.15, 2.5))).collect();
    // (Bands with nothing in them take their nearest neighbour's.)
    for b in 0..nb {
        if s[b].is_none() {
            s[b] = (1..nb).find_map(|d| s.get(b.wrapping_sub(d)).copied().flatten().or_else(|| s.get(b + d).copied().flatten()));
        }
    }
    let mut s: Vec<f32> = s.into_iter().map(|v| v.unwrap_or(1.0)).collect();
    for _ in 0..2 {
        s = (0..nb).map(|b| (s[b.saturating_sub(1)] + s[b] * 2.0 + s[(b + 1).min(nb - 1)]) * 0.25).collect();
    }
    let at = |y: f32| {
        let f = band(y);
        let i = (f as usize).min(nb - 2);
        s[i] + (s[i + 1] - s[i]) * (f - i as f32)
    };
    let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in g.bark.positions.iter_mut().chain(g.fronds.positions.iter_mut()).chain(g.leaves.positions.iter_mut()) {
        let k = at(p[1]);
        p[0] *= k;
        p[2] *= k;
        mn = mn.min(Vec3::from(*p));
        mx = mx.max(Vec3::from(*p));
    }
    let reach = g.leaves.corners.iter().map(|c| c[0].abs().max(c[1].abs())).fold(0.0, f32::max);
    if !g.leaves.positions.is_empty() {
        mx += Vec3::splat(reach * 0.5);
        mn -= Vec3::new(reach * 0.5, 0.0, reach * 0.5);
    }
    (g.min, g.max) = (mn.to_array(), mx.to_array());
}

/// A branch grown from `start` along `dir`: its segments, each turned by gravity (down, or up
/// when it's less than nothing) and disturbed at random, and its radius along it.
#[allow(clippy::too_many_arguments)]
fn grow_branch(lv: &Level, level: usize, start: Vec3, dir: Vec3, len: f32, radius: f32, on_parent: f32, rng: &mut Rng) -> Branch {
    let segs = lv.length_segments.clamp(2, 24) as usize;
    let gravity = lv.gravity.at(on_parent, rng.s());
    let step = len / segs as f32;
    // (The radius profile; a trunk's foot as broad as its broadest low down: its flares.)
    let prof = |t: f32| lv.radius_profile.at(t, 0.0).clamp(0.02, 2.0);
    let foot = if level == 0 { (0..=10).map(|i| prof(i as f32 * 0.025)).fold(0.0, f32::max) } else { 0.0 };
    let foot_to = if level == 0 { (0..=10).map(|i| i as f32 * 0.025).find(|&t| prof(t) >= foot - 1e-4).unwrap_or(0.0) } else { 0.0 };
    let r_at = |t: f32| radius * if t <= foot_to { foot } else { prof(t) };
    let mut d = dir.normalize_or(Vec3::Y);
    let mut p = start;
    let mut pts = vec![p];
    let mut radii = vec![r_at(0.0)];
    for i in 1..=segs {
        let t = (i as f32 - 0.5) / segs as f32;
        let bend = gravity * lv.gravity_profile.at(t, 0.0).clamp(0.0, 1.0) * GRAVITY_BEND / segs as f32;
        if level > 0 && bend.abs() > 1e-5 {
            let target = if bend > 0.0 { Vec3::NEG_Y } else { Vec3::Y };
            let axis = d.cross(target);
            if axis.length_squared() > 1e-8 {
                d = turn(d, axis, bend.abs().min(d.angle_between(target)));
            }
        }
        let amount = lv.disturbance.variance.abs().min(90.0).to_radians() * lv.disturbance.curve(t).clamp(0.0, 1.0) / (segs as f32).sqrt();
        if amount > 0.0 {
            d = turn(d, turn(perpendicular(d), d, rng.u() * std::f32::consts::TAU), amount * rng.s());
        }
        p += d * step;
        pts.push(p);
        radii.push(r_at(i as f32 / segs as f32).max(radius * 0.04));
    }
    Branch { pts, radii, len, level }
}

/// A branch's bark: rings round it, carried along it without twisting, the bark's picture
/// round it `u_tiling` times and up it as far as keeps it square.
fn tube(out: &mut Part, b: &Branch, lv: &Level) {
    let sides = lv.cross_segments.clamp(3, 16) as usize;
    let base = out.positions.len() as u32;
    let r0 = b.radii[0].max(1e-4);
    let u_tiles = lv.u_tiling.round().max(1.0);
    let mut side = perpendicular((b.pts[1] - b.pts[0]).normalize_or(Vec3::Y));
    let mut along = 0.0;
    let mut prev_tan = (b.pts[1] - b.pts[0]).normalize_or(Vec3::Y);
    for (i, (&p, &r)) in b.pts.iter().zip(&b.radii).enumerate() {
        let tan = if i + 1 < b.pts.len() { (b.pts[i + 1] - p).normalize_or(prev_tan) } else { prev_tan };
        let mid = (tan + prev_tan).normalize_or(tan);
        // (Carried round with the branch's turn.)
        let q = glam::Quat::from_rotation_arc(prev_tan, mid);
        side = (q * side - mid * (q * side).dot(mid)).normalize_or(perpendicular(mid));
        if i > 0 {
            along += (p - b.pts[i - 1]).length();
        }
        let up = mid.cross(side);
        for j in 0..=sides {
            let a = j as f32 / sides as f32 * std::f32::consts::TAU;
            let n = side * a.cos() + up * a.sin();
            out.positions.push((p + n * r).to_array());
            out.normals.push(n.to_array());
            out.uvs.push([j as f32 / sides as f32 * u_tiles, -along * u_tiles / (std::f32::consts::TAU * r0 * 4.0)]);
        }
        prev_tan = tan;
    }
    let ring = sides as u32 + 1;
    for i in 0..b.pts.len() as u32 - 1 {
        for j in 0..sides as u32 {
            let (a, c) = (base + i * ring + j, base + (i + 1) * ring + j);
            out.indices.extend_from_slice(&[a, c, a + 1, a + 1, c, c + 1]);
        }
    }
}

/// A frond: its picture along the branch, base to tip, on two blades crossed along it, as
/// wide as `size` of its length.
fn blade(out: &mut Part, b: &Branch, q: [f32; 8], size: f32) {
    let (us, vs) = ([q[0], q[2], q[4], q[6]], [q[1], q[3], q[5], q[7]]);
    let (u0, u1) = (us.iter().copied().fold(f32::MAX, f32::min), us.iter().copied().fold(f32::MIN, f32::max));
    let (v0, v1) = (vs.iter().copied().fold(f32::MAX, f32::min), vs.iter().copied().fold(f32::MIN, f32::max));
    let half = b.len * size.clamp(0.1, 2.0) * 0.5;
    let dir = (*b.pts.last().unwrap() - b.pts[0]).normalize_or(Vec3::Y);
    let flat = {
        let s = dir.cross(Vec3::Y);
        if s.length_squared() < 1e-6 { perpendicular(dir) } else { s.normalize() }
    };
    let n = b.pts.len();
    for across in [flat, dir.cross(flat).normalize_or(perpendicular(dir))] {
        let base = out.positions.len() as u32;
        let normal = dir.cross(across).normalize_or(Vec3::Y);
        let normal = if normal.y < 0.0 { -normal } else { normal };
        for (i, &p) in b.pts.iter().enumerate() {
            let t = i as f32 / (n - 1) as f32;
            // (Narrowing a little to the tip, as the picture does.)
            let w = half * (1.0 - 0.15 * t);
            let v = v1 + (v0 - v1) * t;
            for (s, u) in [(-1.0, u0), (1.0, u1)] {
                out.positions.push((p + across * w * s).to_array());
                out.normals.push(normal.to_array());
                out.uvs.push([u, v]);
            }
        }
        for i in 0..n as u32 - 1 {
            let (a, c) = (base + i * 2, base + (i + 1) * 2);
            out.indices.extend_from_slice(&[a, a + 1, c, a + 1, c + 1, c]);
        }
    }
}

/// A leaf card at `centre`: its picture's corners (from the composite map's quad), placed about
/// the leaf map's origin, `scale` times its size, turned `roll` in its own plane.
fn card(out: &mut Cards, centre: Vec3, q: [f32; 8], map: &LeafMap, scale: f32, roll: f32) {
    let (us, vs) = ([q[0], q[2], q[4], q[6]], [q[1], q[3], q[5], q[7]]);
    let (u0, u1) = (us.iter().copied().fold(f32::MAX, f32::min), us.iter().copied().fold(f32::MIN, f32::max));
    let (v0, v1) = (vs.iter().copied().fold(f32::MAX, f32::min), vs.iter().copied().fold(f32::MIN, f32::max));
    let (w, h) = (map.size[0].max(0.001) * scale, map.size[1].max(0.001) * scale);
    let (o, base) = (map.origin, out.positions.len() as u32);
    let (sn, cs) = roll.sin_cos();
    for k in 0..4 {
        let (u, v) = (us[k], vs[k]);
        let x = ((u - u0) / (u1 - u0).max(1e-6) - o[0]) * w;
        let y = (1.0 - (v - v0) / (v1 - v0).max(1e-6) - o[1]) * h;
        out.positions.push(centre.to_array());
        out.corners.push([x * cs - y * sn, x * sn + y * cs]);
        out.normals.push([0.0, 1.0, 0.0]);
        out.uvs.push([u, v]);
    }
    out.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splines_read_and_run_through_their_points() {
        let s = Spline::parse("BezierSpline 0.212 0.212 0.019\r\n{\r\n\t2\r\n\t0 1 0.707107 -0.707107 0.1\r\n\t1 0 0.707107 -0.707107 0.1\r\n}").unwrap();
        assert_eq!((s.min, s.max, s.variance, s.points.len()), (0.212, 0.212, 0.019, 2));
        let c = Spline::parse("BezierSpline 0 10 0 { 2  0 0 0.707107 0.707107 0.1  1 1 0.707107 0.707107 0.1 }").unwrap();
        assert!((c.curve(0.0)).abs() < 1e-3 && (c.curve(1.0) - 1.0).abs() < 1e-3);
        assert!((c.at(0.5, 0.0) - 5.0).abs() < 0.2, "a straight line's middle: {}", c.at(0.5, 0.0));
    }

    fn flat(min: f32, max: f32) -> Spline {
        Spline { min, max, variance: 0.0, points: vec![[0.0, 1.0, 0.707, -0.707, 0.1], [1.0, 1.0, 0.707, 0.707, 0.1]] }
    }

    /// A tree of a trunk, one level of branches, and leaves on them.
    fn sample() -> SpeedTree {
        let level = |len: f32, angle: f32, freq: f32| Level {
            length: flat(len, len),
            radius: flat(0.05, 0.05),
            radius_profile: flat(1.0, 1.0),
            start_angle: flat(angle, angle),
            gravity_profile: flat(1.0, 1.0),
            cross_segments: 6,
            length_segments: 4,
            first: 0.4,
            last: 1.0,
            frequency: freq,
            ..Default::default()
        };
        SpeedTree {
            seed: 7,
            size: 20.0,
            levels: vec![level(0.6, -90.0, 10.0), level(0.3, 45.0, 20.0), level(0.02, 90.0, 0.0)],
            leaf_maps: vec![LeafMap { size: [0.1, 0.1, 0.0], origin: [0.5, 0.5, 0.0], ..Default::default() }],
            leaf_quads: vec![[1.0, 1.0, 0.5, 1.0, 0.5, 0.5, 1.0, 0.5]],
            ..Default::default()
        }
    }

    #[test]
    fn a_tree_grows_up_with_branches_and_leaves() {
        let t = sample();
        let g = grow(&t, 100);
        // (The trunk's six sides and five rings, ten branches of five rings.)
        assert_eq!(g.bark.positions.len(), 7 * 5 * 11, "a trunk and ten branches");
        let leaves = g.leaves.positions.len() / 4;
        assert!((80..=100).contains(&leaves), "leaves kept to the budget: {leaves}");
        assert!(g.max[1] > 12.0 && g.min[1] > -1.0, "upright: {:?}..{:?}", g.min, g.max);
        // (The branches go out at 45 degrees from a trunk of 12, 6 long: about 4 out.)
        let reach = g.bark.positions.iter().map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt()).fold(0.0, f32::max);
        assert!((3.5..6.0).contains(&reach), "branches' reach {reach}");
        assert_eq!(g.leaves.corners.len(), g.leaves.positions.len());
    }

    #[test]
    fn a_grown_tree_is_fitted_to_its_outline() {
        let mut g = grow(&sample(), 100);
        let ht = g.max[1];
        // (An outline twice as wide everywhere but the trunk's bottom band.)
        let bands = 12;
        let reach = |g: &Grown, lo: f32, hi: f32| g.leaves.positions.iter().filter(|p| p[1] >= lo && p[1] < hi).map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt()).fold(0.0, f32::max);
        let before = reach(&g, ht * 0.6, ht);
        let env: Vec<f32> = (0..bands).map(|b| if b == 0 { 0.01 } else { before * 2.0 / ht }).collect();
        fit(&mut g, &env);
        let after = reach(&g, ht * 0.6, ht);
        assert!(after > before * 1.5, "let out to the outline: {before} -> {after}");
    }

    #[test]
    fn tokens_stop_at_what_isnt_known() {
        let mut d = Vec::new();
        d.extend(1000u32.to_le_bytes());
        d.extend(12u32.to_le_bytes());
        d.extend(b"__IdvSpt_02_");
        d.extend(2006u32.to_le_bytes());
        d.extend(8.5f32.to_le_bytes());
        d.extend(99999u32.to_le_bytes());
        let (t, stop) = tokens(&d);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1], (2006, Value::Float(8.5)));
        assert_eq!(stop, Some((28, 99999)));
    }
}
