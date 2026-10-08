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

/// RGB (0..1) as hue, saturation and value (0..1).
pub fn rgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 1e-6 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if max <= 1e-6 { 0.0 } else { d / max }, max]
}

/// Hue, saturation and value (0..1; hue wraps) as RGB.
pub fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [f32; 3] {
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// A pattern's main colour from its base hue, saturation and value and the shift its preset
/// puts on them (the way patterned materials take their colours).
pub fn shifted(base: [f32; 3], shift: [f32; 3]) -> [f32; 3] {
    hsv_to_rgb([base[0] + shift[0], base[1] + shift[1], base[2] + shift[2]])
}

/// The shift that makes a pattern of `base` hue, saturation and value come out `colour`.
pub fn shift_for(base: [f32; 3], colour: [f32; 3]) -> [f32; 3] {
    let t = rgb_to_hsv(colour);
    // (The hue the shorter way round.)
    let mut dh = t[0] - base[0];
    if dh > 0.5 {
        dh -= 1.0;
    } else if dh < -0.5 {
        dh += 1.0;
    }
    [dh, t[1] - base[1], t[2] - base[2]]
}

/// The value of `key` in a pattern block of preset XML.
fn xml_value<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("key=\"{key}\" value=\"");
    let at = block.find(&pat)? + pat.len();
    block[at..].split('"').next()
}

/// A pattern block's base hue, saturation and value, and its shift (patterned materials).
fn hsv_of(block: &str) -> Option<([f32; 3], [f32; 3])> {
    let one = |k: &str| xml_value(block, k).and_then(|v| v.trim().parse::<f32>().ok());
    let base = [one("Base H Bg")?, one("Base S Bg")?, one("Base V Bg")?];
    let shift = floats(xml_value(block, "HSVShift Bg")?);
    (shift.len() >= 3).then(|| (base, [shift[0], shift[1], shift[2]]))
}

/// The pattern channels of a CAS preset (A to D, as 0 to 3) that are enabled and have a colour
/// to change: a solid colour's, or a patterned material's main colour (its base hue, saturation
/// and value as its shift leaves them). What Create a Style changes.
pub fn solid_channels(xml: &str) -> Vec<(u8, [f32; 3])> {
    let lower = xml.to_ascii_lowercase();
    (0..4u8)
        .filter_map(|ch| {
            let v = format!("pattern {}", (b'a' + ch) as char);
            if !lower.contains(&format!("key=\"{v} enabled\" value=\"true\"")) {
                return None;
            }
            let start = lower.find(&format!("variable=\"{v}\""))?;
            let end = start + lower[start..].find("</pattern>")?;
            let block = &xml[start..end];
            if let Some((base, shift)) = hsv_of(block) {
                return Some((ch, shifted(base, shift)));
            }
            if !lower[start..end].contains("solidcolor") {
                return None;
            }
            let vals = floats(xml_value(block, "Color")?);
            (vals.len() >= 3).then(|| (ch, [vals[0], vals[1], vals[2]]))
        })
        .collect()
}

/// A CAS preset with the colours of some of its solid channels changed (see `solid_channels`).
pub fn with_colours(xml: &str, colours: &[(u8, [f32; 3])]) -> String {
    let mut out = xml.to_string();
    for &(ch, [r, g, b]) in colours {
        let lower = out.to_ascii_lowercase();
        let v = format!("variable=\"pattern {}\"", (b'a' + ch) as char);
        let Some(start) = lower.find(&v) else { continue };
        let Some(len) = lower[start..].find("</pattern>") else { continue };
        let block = out[start..start + len].to_string();
        // (A patterned material: its shift set to bring its main colour out so.)
        let new_values: Vec<(&str, String)> = match hsv_of(&block) {
            Some((base, _)) => {
                let [dh, ds, dv] = shift_for(base, [r, g, b]);
                vec![("HSVShift Bg", format!("{dh:.4},{ds:.4},{dv:.4}")), ("H Bg", format!("{dh:.4}")), ("S Bg", format!("{ds:.4}")), ("V Bg", format!("{dv:.4}"))]
            }
            None => vec![("Color", format!("{r:.7},{g:.7},{b:.7},1.0"))],
        };
        for (key, value) in new_values {
            let pat = format!("key=\"{key}\" value=\"");
            // (The block's end afresh: values change length.)
            let Some(len) = out.to_ascii_lowercase()[start..].find("</pattern>") else { continue };
            let Some(at) = out[start..start + len].find(&pat) else { continue };
            let vstart = start + at + pat.len();
            let Some(vlen) = out[vstart..].find('"') else { continue };
            out.replace_range(vstart..vstart + vlen, &value);
        }
    }
    out
}

#[cfg(test)]
mod style_tests {
    use super::*;

    const XML: &str = r#"<preset><complate name="CasRgbMask"><value key="Pattern A Enabled" value="true" /><value key="Pattern B Enabled" value="true" /><value key="Pattern C Enabled" value="False" /><pattern name="solidColor_1" variable="Pattern A"><value key="Color" value="0.4196078,0.4078431,0.3921569,1.0" /><value key="filename" value="Materials\Miscellaneous\solidColor_1" /></pattern><pattern name="solidColor_1" variable="Pattern B"><value key="Color" value="0.2745098,0.2196078,0.2117647,1.0" /><value key="filename" value="Materials\Miscellaneous\solidColor_1" /></pattern><pattern name="solidColor_1" variable="Pattern C"><value key="Color" value="0.2,0.2,0.2,1.0" /></pattern></complate></preset>"#;

    #[test]
    fn channels_read_and_recoloured() {
        let ch = solid_channels(XML);
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].0, 0);
        assert!((ch[1].1[0] - 0.2745098).abs() < 1e-6);
        let x = with_colours(XML, &[(1, [1.0, 0.0, 0.5])]);
        let again = solid_channels(&x);
        assert_eq!(again[0].1, ch[0].1);
        assert_eq!(again[1].1, [1.0, 0.0, 0.5]);
    }

    const DENIM: &str = r#"<preset><complate name="CasRgbMask"><value key="Pattern A Enabled" value="true" /><pattern name="denimRough" variable="Pattern A"><value key="Base H Bg" value="0.6" /><value key="Base S Bg" value="0.7" /><value key="Base V Bg" value="0.5" /><value key="HSVShift Bg" value="-0.0345,-0.4571,-0.2700" /><value key="H Bg" value="-0.0345" /><value key="S Bg" value="-0.4571" /><value key="V Bg" value="-0.27" /><value key="rgbmask" value="denim_mask" /></pattern></complate></preset>"#;

    #[test]
    fn patterned_channels_recoloured() {
        let ch = solid_channels(DENIM);
        assert_eq!(ch.len(), 1);
        let x = with_colours(DENIM, &[(0, [0.8, 0.1, 0.1])]);
        let [r, g, b] = solid_channels(&x)[0].1;
        assert!((r - 0.8).abs() < 0.01 && (g - 0.1).abs() < 0.01 && (b - 0.1).abs() < 0.01, "{r} {g} {b}");
        // (The pattern's other values left as they were.)
        assert!(x.contains("denim_mask") && x.contains(r#"key="Base H Bg" value="0.6""#));
    }
}
