//! Complates (`_XML`, 0x0333406C): the game's texture recipes in XML — parameters with
//! defaults plus the same render steps a compiled TXTC holds. Wall and floor patterns (CWAL)
//! ship only these and parameter overrides, so they're translated into a `Txtc` (patterns
//! become embedded fabrics) and run through the CPU compositor.

use std::collections::HashMap;

use crate::catalog::{CValue, Complate, T_COMPLATE_XML};
use crate::txtc::*;
use s3pkg::{PackageSet, ResourceKey, types};

const RT_A: u32 = 0x021E9CD2;
const RT_B: u32 = 0x021E9CD4;

/// One XML tag: its name, attributes, and whether it closes an element.
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    closing: bool,
}

impl Tag {
    fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// The tags of a (flat, well-formed) XML document in order.
fn tags(xml: &str) -> Vec<Tag> {
    let mut out = Vec::new();
    let b = xml.as_bytes();
    let mut i = 0;
    while let Some(o) = xml[i..].find('<') {
        let start = i + o + 1;
        if xml[start..].starts_with("!--") {
            i = xml[start..].find("-->").map_or(b.len(), |e| start + e + 3);
            continue;
        }
        // The tag runs to the next '>' outside quotes.
        let mut j = start;
        let mut quote = None;
        while j < b.len() {
            match (quote, b[j]) {
                (None, b'"' | b'\'') => quote = Some(b[j]),
                (Some(q), c) if c == q => quote = None,
                (None, b'>') => break,
                _ => {}
            }
            j += 1;
        }
        let body = &xml[start..j.min(b.len())];
        i = (j + 1).min(b.len());
        if body.starts_with('?') || body.starts_with('!') {
            continue;
        }
        let closing = body.starts_with('/');
        let body = body.trim_start_matches('/').trim_end_matches('/');
        let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
        let name = body[..name_end].to_string();
        let mut attrs = Vec::new();
        let mut rest = &body[name_end..];
        while let Some(eq) = rest.find('=') {
            let key = rest[..eq].trim().to_string();
            let after = rest[eq + 1..].trim_start();
            let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
            let Some(end) = after[1..].find(q) else { break };
            attrs.push((key, unescape(&after[1..1 + end])));
            rest = &after[1 + end + 1..];
        }
        out.push(Tag { name, attrs, closing });
    }
    out
}

/// A parsed complate: parameter defaults and the steps of each destination texture.
pub struct ComplateXml {
    pub name: String,
    params: HashMap<String, (String, String)>,
    destinations: Vec<(String, Vec<Vec<(String, String)>>)>,
}

impl ComplateXml {
    pub fn parse(xml: &str) -> Self {
        let mut name = String::new();
        let mut params = HashMap::new();
        let mut destinations: Vec<(String, Vec<Vec<(String, String)>>)> = Vec::new();
        for t in tags(xml) {
            if t.closing {
                continue;
            }
            match t.name.as_str() {
                "complate" => name = t.attr("name").unwrap_or("").to_string(),
                "param" => {
                    if let Some(n) = t.attr("name") {
                        params.insert(
                            n.to_ascii_lowercase(),
                            (t.attr("type").unwrap_or("").to_string(), t.attr("default").unwrap_or("").to_string()),
                        );
                    }
                }
                "destination" => destinations.push((t.attr("textureName").unwrap_or("").to_string(), Vec::new())),
                "step" => {
                    if let Some(d) = destinations.last_mut() {
                        d.1.push(t.attrs);
                    }
                }
                _ => {}
            }
        }
        Self { name, params, destinations }
    }

    /// The steps making the diffuse texture (the first destination).
    fn diffuse_steps(&self) -> Option<&Vec<Vec<(String, String)>>> {
        self.destinations
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("DiffuseMap"))
            .or_else(|| self.destinations.first())
            .map(|(_, s)| s)
    }
}

/// Reads a complate's XML by key.
pub fn load(pkgs: &PackageSet, key: &ResourceKey) -> Option<ComplateXml> {
    let d = pkgs.read(key).or_else(|| pkgs.read_ti(key.t, key.i))?;
    Some(ComplateXml::parse(&String::from_utf8_lossy(&d)))
}

/// The resource key of a complate or texture named by a path ("…\Wood\grainStraight_1").
fn named_key(t: u32, path: &str) -> ResourceKey {
    let stem = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
    ResourceKey::new(t, 0, s3pkg::fnv64(&stem.to_ascii_lowercase()))
}

fn floats(s: &str) -> Vec<f32> {
    s.split(',').filter_map(|x| x.trim().parse().ok()).collect()
}

fn blend(name: &str) -> i64 {
    match name.trim() {
        "Zero" => 0,
        "One" => 1,
        "SrcColor" => 2,
        "InvSrcColor" => 3,
        "SrcAlpha" => 4,
        "InvSrcAlpha" => 5,
        "DestAlpha" => 6,
        "InvDestAlpha" => 7,
        "DestColor" => 8,
        "InvDestColor" => 9,
        "SrcAlphaSat" => 10,
        _ => 1,
    }
}

fn color_write(s: &str) -> u64 {
    s.split([',', '|', ' '])
        .map(|w| match w.trim() {
            "Red" => 1,
            "Green" => 2,
            "Blue" => 4,
            "Alpha" => 8,
            "Color" => 7,
            "All" => 15,
            _ => 0,
        })
        .fold(0, |a, b| a | b)
}

fn argb_of(v: &[f32]) -> u32 {
    let c = |i: usize, d: f32| ((v.get(i).copied().unwrap_or(d).clamp(0.0, 1.0) * 255.0).round() as u32) & 255;
    (c(3, 1.0) << 24) | (c(0, 0.0) << 16) | (c(1, 0.0) << 8) | c(2, 0.0)
}

struct Builder {
    keys: Vec<ResourceKey>,
    fabrics: Vec<(u8, Txtc)>,
}

impl Builder {
    fn key_index(&mut self, k: ResourceKey) -> u8 {
        match self.keys.iter().position(|x| *x == k) {
            Some(i) => i as u8,
            None => {
                self.keys.push(k);
                (self.keys.len() - 1) as u8
            }
        }
    }
}

/// Builds the compositor program for a complate with an instance's overrides. `mkeys` is the
/// resource list the instance's texture and pattern indices refer to.
pub fn build(pkgs: &PackageSet, xml: &ComplateXml, inst: &Complate, mkeys: &[ResourceKey], depth: usize) -> Option<Txtc> {
    if depth > 3 {
        return None;
    }
    let value = |name: &str| -> String {
        inst.get(name).map(|v| v.text()).or_else(|| xml.params.get(&name.to_ascii_lowercase()).map(|p| p.1.clone())).unwrap_or_default()
    };
    // Substitutes "($Name)" references (repeatedly: defaults refer to other parameters).
    let subst = |s: &str| -> String {
        let mut out = s.to_string();
        for _ in 0..4 {
            let Some(a) = out.find("($") else { break };
            let Some(b) = out[a..].find(')') else { break };
            let name = out[a + 2..a + b].to_string();
            let v = match name.as_str() {
                "assetRoot" => "X:".to_string(),
                _ => value(&name),
            };
            out.replace_range(a..a + b + 1, &v);
        }
        out
    };
    let mut b = Builder { keys: Vec::new(), fabrics: Vec::new() };
    let mut steps = Vec::new();
    for attrs in xml.diffuse_steps()? {
        let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        if let Some(e) = get("enabled")
            && subst(e).trim().eq_ignore_ascii_case("false")
        {
            continue;
        }
        let kind = match get("type").unwrap_or("") {
            "ColorFill" => STEP_COLOR_FILL,
            "SetTarget" => STEP_SET_TARGET,
            "ChannelSelect" => STEP_CHANNEL_SELECT,
            "DrawFabric" => STEP_DRAW_FABRIC,
            "DrawImage" => STEP_DRAW_IMAGE,
            "HSVShift" => STEP_HSV_TO_RGB,
            _ => continue,
        };
        let mut step = Step::default();
        step.props.push((STEP_ID, Value::UInt(kind as u64)));
        for (k, raw) in attrs {
            let v = subst(raw);
            match k.as_str() {
                "color" => step.props.push((P_COLOR, Value::UInt(argb_of(&floats(&v)) as u64))),
                "colorWrite" => step.props.push((P_COLOR_WRITE, Value::UInt(color_write(&v)))),
                "renderTarget" => {
                    let rt = if v.ends_with('A') { RT_A } else { RT_B };
                    step.props.push((P_RENDER_TARGET, Value::UInt(rt as u64)));
                }
                "texture" => {
                    if let Some(rest) = v.strip_prefix("#tgi") {
                        let Some(k) = rest.parse::<usize>().ok().and_then(|i| mkeys.get(i)) else { continue };
                        let i = b.key_index(*k);
                        step.props.push((P_IMAGE_KEY, Value::Tgi(i)));
                    } else if v.starts_with("RenderTexture") {
                        let rt = if v.ends_with('A') { RT_A + 1 } else { RT_B + 1 };
                        step.props.push((P_IMAGE_SOURCE, Value::UInt(rt as u64)));
                    } else if !v.is_empty() {
                        let i = b.key_index(named_key(types::DDS, &v));
                        step.props.push((P_IMAGE_KEY, Value::Tgi(i)));
                    }
                }
                "select" => {
                    let f = floats(&v);
                    if f.len() == 4 {
                        step.props.push((P_CHANNEL_SELECT, Value::Vec4([f[0], f[1], f[2], f[3]])));
                    }
                }
                "enableBlending" => step.props.push((P_ENABLE_BLENDING, Value::Bool(v.eq_ignore_ascii_case("true")))),
                "srcBlend" => step.props.push((P_SRC_BLEND, Value::Int(blend(&v)))),
                "dstBlend" => step.props.push((P_DST_BLEND, Value::Int(blend(&v)))),
                "sourceRect" | "destinationRect" | "destRect" => {
                    let f = floats(&v);
                    if f.len() == 4 {
                        let p = if k == "sourceRect" { P_SRC_RECT } else { P_DST_RECT };
                        step.props.push((p, Value::Vec4([f[0], f[1], f[2], f[3]])));
                    }
                }
                "hsvShift" => {
                    let f = floats(&v);
                    step.props.push((P_HSV_SHIFT, Value::Vec4([f.first().copied().unwrap_or(0.0), f.get(1).copied().unwrap_or(0.0), f.get(2).copied().unwrap_or(0.0), 0.0])));
                }
                "pattern" => {
                    // "($daeFileName)Pattern A": the pattern filling slot "Pattern A".
                    let slot = raw.rsplit(')').next().unwrap_or(raw).trim();
                    let fab = match inst.block(slot) {
                        Some(sub) => mkeys.get(sub.xml as usize).filter(|k| k.t == T_COMPLATE_XML).and_then(|k| {
                            let x = load(pkgs, k)?;
                            build(pkgs, &x, sub, mkeys, depth + 1)
                        }),
                        None => {
                            let name = value(slot);
                            if name.is_empty() {
                                None
                            } else {
                                load(pkgs, &named_key(T_COMPLATE_XML, &name)).and_then(|x| build(pkgs, &x, &Complate::default(), mkeys, depth + 1))
                            }
                        }
                    };
                    if let Some(f) = fab {
                        let idx = 0x80 + b.fabrics.len() as u8;
                        b.fabrics.push((idx, f));
                        step.props.push((P_DEFAULT_FABRIC, Value::Tgi(idx)));
                        step.props.push((P_WIDTH, Value::UInt(256)));
                        step.props.push((P_HEIGHT, Value::UInt(256)));
                    }
                }
                _ => {}
            }
        }
        steps.push(step);
    }
    Some(Txtc { version: 8, fabrics: b.fabrics, steps, keys: b.keys })
}

/// Renders a complate instance to an RGBA image of the given size.
pub fn render(pkgs: &PackageSet, inst: &Complate, mkeys: &[ResourceKey], w: usize, h: usize) -> Option<crate::dds::Rgba> {
    let key = mkeys.get(inst.xml as usize).filter(|k| k.t == T_COMPLATE_XML)?;
    let xml = load(pkgs, key)?;
    let t = build(pkgs, &xml, inst, mkeys, 0)?;
    let mut c = crate::compositor::Compositor::new(pkgs);
    c.max_size = w.max(h);
    Some(c.run(&t, w, h))
}

/// A CAS design preset (`<preset><complate …><value …/><pattern …>…</pattern></complate>`,
/// as CAS parts and outfits keep them) as a complate instance and its resource list.
pub fn preset(xml: &str) -> Option<(Complate, Vec<ResourceKey>)> {
    let parse_key = |s: &str| -> Option<ResourceKey> {
        let mut it = s.strip_prefix("key:")?.split(':');
        let t = u32::from_str_radix(it.next()?, 16).ok()?;
        let g = u32::from_str_radix(it.next()?, 16).ok()?;
        let i = u64::from_str_radix(it.next()?, 16).ok()?;
        Some(ResourceKey::new(t, g, i))
    };
    let mut keys: Vec<ResourceKey> = Vec::new();
    let mut index = |k: ResourceKey| -> u8 {
        match keys.iter().position(|x| *x == k) {
            Some(i) => i as u8,
            None => {
                keys.push(k);
                (keys.len() - 1) as u8
            }
        }
    };
    // The open complate and patterns, innermost last.
    let mut stack: Vec<Complate> = Vec::new();
    let mut top = None;
    for t in tags(xml) {
        match (t.name.as_str(), t.closing) {
            ("complate" | "pattern", false) => {
                let xml = t.attr("reskey").and_then(parse_key).map_or(u8::MAX, &mut index);
                stack.push(Complate {
                    xml,
                    name: t.attr("name").unwrap_or("").to_string(),
                    pattern: t.attr("variable").unwrap_or("").to_string(),
                    ..Default::default()
                });
            }
            ("complate" | "pattern", true) => {
                let c = stack.pop()?;
                match stack.last_mut() {
                    Some(parent) => parent.blocks.push(c),
                    None => top = Some(c),
                }
            }
            ("value", false) => {
                let (Some(k), Some(v), Some(c)) = (t.attr("key"), t.attr("value"), stack.last_mut()) else { continue };
                let v = match parse_key(v) {
                    Some(rk) => CValue::Tgi(index(rk)),
                    None => CValue::Str(v.to_string()),
                };
                c.overrides.push((k.to_string(), v));
            }
            _ => {}
        }
    }
    Some((top?, keys))
}

/// Renders a CAS design preset; `layer` keeps the part's own layer (coverage in alpha) rather
/// than the composite over skin.
pub fn render_preset(pkgs: &PackageSet, xml: &str, max: usize, layer: bool) -> Option<crate::dds::Rgba> {
    let (inst, keys) = preset(xml)?;
    let x = load(pkgs, keys.get(inst.xml as usize)?)?;
    let t = build(pkgs, &x, &inst, &keys, 0)?;
    let mut c = crate::compositor::Compositor::new(pkgs);
    c.max_size = max;
    let (w, h) = c.output_size(&t);
    let (w, h) = (w.min(max), h.min(max));
    c.layer_mode = layer;
    Some(c.run(&t, w, h))
}
