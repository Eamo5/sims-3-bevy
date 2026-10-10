//! The game's interface as its own UI framework draws it: the layouts (`0x025C95B6`, XML window
//! trees in `Game/Bin/UI/UI.package`), the interface images they draw (PNG `0x2F7D0004`) and
//! the fonts (OpenType `0x062E9EE0`) with the English style sheet's text styles (`0x025C90A6`).
//! Layouts are kept by instance (fnv64 of the lowercase layout name, as `HUDSimDisplay`);
//! images and fonts go to `ui.pack` as they are. See docs/formats/ui.md.

use std::collections::{BTreeSet, HashMap};

use s3pkg::{Package, PackageSet};
use serde::{Deserialize, Serialize};

use crate::bake::BakeRoot;
use crate::pack::{PackWriter, read_value, write_value};

pub const UI_VERSION: u32 = 17;
pub const T_LAYOUT: u32 = 0x025C95B6;
pub const T_FONT: u32 = 0x062E9EE0;
pub const T_IMAGE: u32 = 0x2F7D0004;
const T_STYLES: u32 = 0x025C90A6;
const T_XML: u32 = 0x0333406C;

/// Window flags: shown.
pub const WIN_VISIBLE: u32 = 0x1;
/// Window flags: takes clicks (a disabled button shows its disabled image).
pub const WIN_ENABLED: u32 = 0x2;
/// Window flags: clicks go through it.
pub const WIN_IGNORE_MOUSE: u32 = 0x10;
/// Window flags: its children are clipped to it.
pub const WIN_CLIP: u32 = 0x400;

/// Anchor bits of the layout procs.
pub const ANCHOR_TOP: u8 = 1;
pub const ANCHOR_BOTTOM: u8 = 2;
pub const ANCHOR_LEFT: u8 = 4;
pub const ANCHOR_RIGHT: u8 = 8;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UiBaked {
    pub version: u32,
    /// Every layout, by instance: its exported windows by export id (the first is the main
    /// one; item templates export several, as a moodlet's positive, negative and neutral cells,
    /// and nested windows can be exports too).
    pub layouts: Vec<(u64, Vec<(u32, UiWindow)>)>,
    /// The English text styles by id (a window's `TextFont`; 0 the default).
    pub styles: Vec<(u32, TextStyle)>,
    /// Buy mode's catalogue, and where each catalogue object goes in it (by OBJD key).
    pub buy: BuyCatalogBaked,
    pub buy_flags: Vec<(crate::types::Key, ObjBuy)>,
    /// The catalogue objects' descriptions (by OBJD key; those that have one).
    pub descriptions: Vec<(crate::types::Key, String)>,
    /// The loading screen's game tips (`GameTips` and the packs' `GameTipsEP<n>`, those whose
    /// words are installed).
    pub tips: Vec<String>,
}

/// A text style: its font (instance in `ui.pack`), size and line spacing in pixels.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TextStyle {
    pub name: String,
    pub font: u64,
    pub size: f32,
    pub line: f32,
}

/// How a window keeps its place as its parent's size changes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub enum UiPlace {
    /// Where its area says, from the parent's top-left.
    #[default]
    Fixed,
    /// `SimpleLayout`: each anchored edge's coordinate is measured from that edge of the parent
    /// (a negative right or bottom is in from the right or bottom); unanchored, from the top-left.
    Simple(u8),
    /// `HudLayout`: the area is in a design screen of these dimensions, and keeps its distance
    /// from the anchored edges of the real one.
    Hud(u8, [f32; 2]),
    /// `CenterInParentLayout`: centred across, this fraction of the free space down.
    Center(f32),
}

/// What a window draws.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum UiDrawable {
    /// `StdDrawable`: an image per state (normal, disabled, highlighted, pressed, then the same
    /// selected; 0 for none), stretched (scale 0), at its own size centred (1) or nine-sliced
    /// by its borders (2; fractions of the image), and the glow drawn round it when highlighted.
    Std { images: [u64; 8], scale: u8, borders: [f32; 4], glow: u64 },
    /// `ImageDrawable` / `IconDrawable`: one image, aligned (1 left/top, 2 right/bottom, 3
    /// centre), flags 1 scaled to fit and 2 keeping its shape, tinted per state for icons.
    Image { image: u64, flags: u32, halign: u8, valign: u8, tiling: u8, scale: f32, colors: Vec<u32> },
    /// `IconButtonMultiDrawable`: several, in order.
    Multi(Vec<UiDrawable>),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UiWindow {
    /// The class: `Window`, `Button`, `Text`, `Sims3IconButton`, `ItemGrid`...
    pub cls: String,
    pub id: u32,
    pub command: u32,
    pub flags: u32,
    /// Left, top, right, bottom (see [`UiPlace`]).
    pub area: [f32; 4],
    pub fill: u32,
    pub shade: u32,
    pub drawable: Option<UiDrawable>,
    /// The caption (in English) of a text or button.
    pub caption: String,
    /// Its string-table key, when it has one.
    pub caption_key: u64,
    pub font: u32,
    /// The text colour (per state, for buttons).
    pub colors: Vec<u32>,
    /// Text alignment: across 0 left, 1 centre, 2 right, 4 justified; down 0 top, 1 middle,
    /// 2 bottom, 3 middle.
    pub halign: u8,
    pub valign: u8,
    pub wrap: u32,
    pub tooltip: String,
    pub tooltip_key: u64,
    pub comment: String,
    pub place: UiPlace,
    pub button_type: u32,
    pub button_group: u32,
    /// A button's picture's place (`Alignment`: 1 at its left, as a check box's box).
    pub align: u8,
    pub icon: u64,
    /// An `ItemGrid`'s cells.
    pub grid: Option<UiGrid>,
    /// A `FillBarController`'s fill: direction (0 from the start, 1 from the middle, 2 from the
    /// end), and its colour (and its colour below the middle).
    pub fill_bar: Option<(u8, u32, u32)>,
    /// A slider's least, most and starting value, and its orientation.
    pub slider: Option<[f32; 4]>,
    /// A combo box's drop-down.
    pub combo: Option<UiCombo>,
    /// A scrollbar's orientation, minimum thumb size and seven skin pieces. Empty pieces
    /// retain their positions in the original `ScrollbarMultiDrawable`.
    pub scrollbar: Option<(bool, f32, Vec<Option<UiDrawable>>)>,
    pub children: Vec<UiWindow>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct UiGrid {
    /// Each cell's size.
    pub cell: [f32; 2],
    /// Left, top, right, bottom.
    pub padding: [f32; 4],
    /// Each cell's own padding (left, top, right, bottom: a cell's step is its size plus its
    /// padding's width and height).
    pub cell_padding: [f32; 4],
    pub columns: u32,
    pub rows: u32,
}

/// A combo box's drop-down: its background picture, its words' colours (normal, background,
/// highlighted, highlight background...), and how far below the box it drops.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UiCombo {
    pub pulldown: Option<UiDrawable>,
    pub colors: Vec<u32>,
    pub offset: f32,
}

impl UiWindow {
    pub fn visible(&self) -> bool {
        self.flags & WIN_VISIBLE != 0
    }
    /// The window (this or under it) with a control id.
    pub fn find(&self, id: u32) -> Option<&UiWindow> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }
    /// The window (this or under it) with a comment.
    pub fn find_comment(&self, comment: &str) -> Option<&UiWindow> {
        if self.comment == comment {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find_comment(comment))
    }
    /// Captions and tooltips in the game's words where its string tables have them (the layouts
    /// hold the designers' placeholders and string names, as `Ui/Tooltip/HUD/SimDisplay:Mood`).
    fn resolve(&mut self, strings: &HashMap<u64, String>) {
        if let Some(t) = strings.get(&self.caption_key).filter(|_| self.caption_key != 0) {
            self.caption = t.clone();
        }
        if let Some(t) = strings.get(&self.tooltip_key).filter(|_| self.tooltip_key != 0) {
            self.tooltip = t.clone();
        }
        for c in &mut self.children {
            c.resolve(strings);
        }
    }
    fn images(&self, out: &mut BTreeSet<u64>) {
        if self.icon != 0 { out.insert(self.icon); }
        fn of(d: &UiDrawable, out: &mut BTreeSet<u64>) {
            match d {
                UiDrawable::Std { images, glow, .. } => out.extend(images.iter().chain([glow]).copied().filter(|i| *i != 0)),
                UiDrawable::Image { image, .. } => {
                    if *image != 0 {
                        out.insert(*image);
                    }
                }
                UiDrawable::Multi(v) => v.iter().for_each(|d| of(d, out)),
            }
        }
        if let Some(d) = &self.drawable {
            of(d, out);
        }
        if let Some(d) = self.combo.as_ref().and_then(|c| c.pulldown.as_ref()) {
            of(d, out);
        }
        if let Some((_, _, parts)) = &self.scrollbar {
            for d in parts.iter().flatten() {
                of(d, out);
            }
        }
        for c in &self.children {
            c.images(out);
        }
    }
}

/// Pictures the interface's code loads by name (`ResourceKey.CreatePNGKey`), at fnv64 of the
/// name: the life stages' icons (the Simology panel's age bar).
pub const NAMED_IMAGES: &[&str] = &[
    "cas_basics_i_age_baby_r2",
    "cas_basics_i_age_toddler_r2",
    "cas_basics_i_age_child_r2",
    "cas_basics_i_age_teen_r2",
    "cas_basics_i_age_yadult_r2",
    "cas_basics_i_age_adult_r2",
    "cas_basics_i_age_elderly_r2",
    // The relationships panel's states (`GetLTRRelationshipImageKey`), the empty opportunity
    // slots (`mPlaceholderOppIcons`), the catalogue's All tab.
    "relationships_state_stranger",
    "relationships_state_aquaintance",
    "relationships_state_disliked",
    "relationships_state_friend_distant",
    "relationships_state_friend",
    "relationships_state_friend_good",
    "relationships_state_friend_best",
    "relationships_state_friend_old",
    "relationships_state_friend_bff",
    "relationships_state_date",
    "relationships_state_ex_spouse",
    "relationships_state_ex_romance",
    "relationships_state_enemy",
    "relationships_state_enemy_old",
    "relationships_state_partners",
    "relationships_state_fiancee",
    "relationships_state_spouse",
    "relationships_state_stranger_pet",
    "relationships_state_aquaintance_pet",
    "relationships_state_friend_pet",
    "relationships_state_friend_best_pet",
    "relationships_state_friend_bff_pet",
    "relationships_state_disliked_pet",
    "relationships_state_enemy_pet",
    "relationships_state_mate_pet",
    "opp_generic_career",
    "opp_generic_skill",
    "opp_generic",
    "glb_i_all_r2",
    // The towns' loading pictures (`LoadingScreenController`).
    "world_loading_twinbrook",
    "world_loading_bridgeport",
    "ep5_world_loading_screen",
    "world_loading_EP6World",
    "world_loading_EP7World",
    "ep10_world_loading_screen",
    "world_loading_university",
    "world_loading_future",
    "world_loading_beijing",
    "world_loading_paris",
    "world_loading_cairo",
];

/// The zodiac signs (`sign_<sign>_sm`).
pub const ZODIAC: [&str; 12] = ["Aries", "Taurus", "Gemini", "Cancer", "Leo", "Virgo", "Libra", "Scorpio", "Sagittarius", "Capricorn", "Aquarius", "Pisces"];

pub fn ui_ready(root: &BakeRoot) -> bool {
    let g = root.global_dir();
    g.join("ui.pack").exists() && read_value::<u32>(&g.join("ui.version")).is_ok_and(|v| v == UI_VERSION)
}

pub fn load_ui(root: &BakeRoot) -> Option<UiBaked> {
    read_value::<UiBaked>(&root.global_dir().join("ui.bin")).ok().filter(|u| u.version == UI_VERSION)
}

/// The interface package (not in the game's resource lists: the executable loads it itself).
pub fn ui_package(install_root: &std::path::Path) -> std::path::PathBuf {
    install_root.join("Game").join("Bin").join("UI").join("UI.package")
}

pub fn bake_ui(root: &BakeRoot, pkgs: &PackageSet, install_root: &std::path::Path, progress: &dyn Fn(&str)) -> Result<usize, String> {
    progress("Converting: the interface…");
    let pkg = Package::open(ui_package(install_root)).map_err(|e| format!("UI.package: {e}"))?;
    let mut out = UiBaked { version: UI_VERSION, ..Default::default() };
    for e in pkg.of_type(T_LAYOUT) {
        let Ok(d) = pkg.read(e) else { continue };
        let s = String::from_utf8_lossy(&d);
        let Some(doc) = parse_xml(&s) else { continue };
        // (Exports can be nested windows too: a notification's foregrounds and backgrounds.)
        let mut exports: Vec<(u32, UiWindow)> = Vec::new();
        fn exported(n: &XNode, top: bool, out: &mut Vec<(u32, UiWindow)>) {
            if n.name == "object"
                && (top || n.attr("id").is_some())
                && let Some(w) = window(n)
            {
                out.push((n.attr("id").map_or(1, num), w));
            }
            for c in &n.children {
                exported(c, false, out);
            }
        }
        for o in doc.children.iter().filter(|c| c.name == "object") {
            exported(o, true, &mut exports);
        }
        if !exports.is_empty() {
            out.layouts.push((e.key.i, exports));
        }
    }
    out.layouts.sort_by_key(|l| l.0);
    let strings: HashMap<u64, String> = read_value(&root.global_dir().join("strings.bin")).unwrap_or_default();
    for (_, list) in &mut out.layouts {
        for (_, w) in list {
            w.resolve(&strings);
        }
    }
    let g = root.global_dir();
    std::fs::create_dir_all(&g).map_err(|e| e.to_string())?;
    let mut pack = PackWriter::create(&g.join("ui.pack")).map_err(|e| e.to_string())?;
    // The fonts (all but the big Chinese, Japanese, Korean and Thai ones), by their names.
    let mut fonts: Vec<(FontNames, u64)> = Vec::new();
    for e in pkg.of_type(T_FONT) {
        let Ok(d) = pkg.read(e) else { continue };
        if d.len() > 1_000_000 {
            continue;
        }
        let Some(n) = font_names(&d) else { continue };
        pack.add((T_FONT, 0, e.key.i), &d).map_err(|e| e.to_string())?;
        fonts.push((n, e.key.i));
    }
    // The English style sheet: the one whose default style is in Helvetica Rounded.
    let sheet = pkg
        .of_type(T_STYLES)
        .filter_map(|e| pkg.read(e).ok())
        .map(|d| String::from_utf8_lossy(&d).into_owned())
        .find(|s| s.contains("HelveticaRounded"))
        .unwrap_or_default();
    for (id, st) in parse_styles(&sheet) {
        let font = pick_font(&fonts, &st.family, st.italic).unwrap_or(0);
        out.styles.push((id, TextStyle { name: st.name, font, size: st.size, line: st.line }));
    }
    // Every image the layouts draw, and those the interface's code loads by name.
    let mut images: BTreeSet<u64> = NAMED_IMAGES.iter().map(|n| s3pkg::fnv64(n)).collect();
    for z in ZODIAC {
        images.insert(s3pkg::fnv64(&format!("sign_{z}_sm")));
    }
    // The game tips.
    for name in std::iter::once("GameTips".to_string()).chain((1..=11).map(|n| format!("GameTipsEP{n}"))) {
        let Some(xml) = pkg.find(&s3pkg::ResourceKey::new(T_XML, 0, s3pkg::fnv64(&name))).and_then(|e| pkg.read(e).ok()) else { continue };
        let Some(doc) = parse_xml(&String::from_utf8_lossy(&xml)) else { continue };
        let mut tips = Vec::new();
        collect_tips(&doc, &mut tips);
        out.tips.extend(tips.iter().filter_map(|k| strings.get(&s3pkg::fnv64(k)).cloned()));
    }
    // Buy mode's catalogue (its icons with the rest), and each catalogue object's place in it.
    if let Some(xml) = pkg.find(&s3pkg::ResourceKey::new(T_XML, 0, s3pkg::fnv64("BuyCatalog"))).and_then(|e| pkg.read(e).ok()) {
        out.buy = buy_catalog(&String::from_utf8_lossy(&xml), &strings, &mut images);
    }
    let catalog: Vec<crate::types::CatalogEntry> = read_value(&g.join("catalog.bin")).unwrap_or_default();
    let objs = crate::bake::par_map(&catalog, |c| {
        let d = pkgs.read(&crate::types::rkey(c.objd))?;
        let o = s3formats::object::parse_objd(&d).ok()?;
        let f = o.buy;
        let desc = if c.price > 0 { strings.get(&o.desc_guid).cloned().unwrap_or_default() } else { String::new() };
        Some((c.objd, ObjBuy { room: f.room, function: f.function, sub: f.function_sub, sub2: f.function_sub2, room_sub: f.room_sub, sort: f.sort }, desc))
    });
    for (k, f, desc) in objs.into_iter().flatten() {
        out.buy_flags.push((k, f));
        if !desc.is_empty() {
            out.descriptions.push((k, desc));
        }
    }
    for (_, list) in &out.layouts {
        for (_, w) in list {
            w.images(&mut images);
        }
    }
    let mut n = 0;
    for (k, i) in images.iter().enumerate() {
        if k % 500 == 0 {
            progress(&format!("Converting: the interface ({k} of {} pictures)…", images.len()));
        }
        if let Some(png) = pkgs.read_ti(T_IMAGE, *i) {
            pack.add((T_IMAGE, 0, *i), &png).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    pack.finish().map_err(|e| e.to_string())?;
    write_value(&g.join("ui.bin"), &out).map_err(|e| e.to_string())?;
    write_value(&g.join("ui.version"), &UI_VERSION).map_err(|e| e.to_string())?;
    Ok(n)
}

fn collect_tips(n: &XNode, out: &mut Vec<String>) {
    if n.name == "Tip"
        && let Some(k) = n.attr("localizedName")
    {
        out.push(k.to_string());
    }
    for c in &n.children {
        collect_tips(c, out);
    }
}

// ---- Layouts ----

fn window(o: &XNode) -> Option<UiWindow> {
    let props = |n: &str| o.children.iter().find(|c| c.name == "prop" && c.attr("name") == Some(n));
    let val = |n: &str| props(n).and_then(|p| p.attr("value"));
    let area = val("Area")?;
    let mut w = UiWindow { cls: o.attr("cls").unwrap_or_default().to_string(), ..Default::default() };
    let a: Vec<f32> = area.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if a.len() == 4 {
        w.area = [a[0], a[1], a[2], a[3]];
    }
    w.id = val("ControlID").map_or(0, num);
    w.command = val("CommandID").map_or(0, num);
    w.flags = val("WindowFlags").map_or(0, num);
    w.fill = val("FillColor").map_or(0, num);
    w.shade = val("ShadeColor").map_or(0xffff_ffff, num);
    w.font = val("TextFont").map_or(0, num);
    w.comment = val("Comment").unwrap_or_default().to_string();
    if let Some(t) = props("TooltipText") {
        w.tooltip = t.attr("value").unwrap_or_default().to_string();
        w.tooltip_key = t.attr("key").map_or(0, image_key);
    }
    if let Some(c) = props("Caption") {
        w.caption = c.attr("value").unwrap_or_default().to_string();
        w.caption_key = c.attr("key").map_or(0, image_key);
    }
    // (A text's one colour; a button's caption colour per state.)
    if let Some(c) = val("TextColor") {
        w.colors = vec![num(c)];
    } else if let Some(p) = props("CaptionColors").or(props("Text Colors")) {
        w.colors = p.children.iter().filter(|v| v.name == "value").map(|v| num(&v.text)).collect();
    }
    w.halign = val("HorizontalAlign").or(val("CaptionHAlign")).map_or(0, num) as u8;
    w.valign = val("VerticalAlign").or(val("CaptionVAlign")).map_or(0, num) as u8;
    w.wrap = val("WordWrap").or(val("CaptionWrap")).or(val("Wrap Mode")).map_or(0, num);
    w.button_type = val("ButtonType").map_or(0, num);
    w.button_group = val("ButtonGroupID").map_or(0, num);
    w.align = val("Alignment").map_or(0, num) as u8;
    w.icon = props("Icon").and_then(|p| p.attr("key")).map_or(0, image_key);
    for name in ["FillDrawable", "ButtonDrawable", "SliderDrawable", "ComboBoxDrawable", "DialogDrawable"] {
        if let Some(d) = props(name).and_then(|p| p.child("object")).and_then(drawable) {
            w.drawable = Some(d);
        }
    }
    if w.cls == "ItemGrid" {
        let floats = |n: &str| -> Vec<f32> { val(n).unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect() };
        let (cell, pad, cell_pad) = (floats("CellArea"), floats("GridPadding"), floats("CellPadding"));
        w.grid = Some(UiGrid {
            cell: [cell.first().copied().unwrap_or(0.0), cell.get(1).copied().unwrap_or(0.0)],
            padding: [0, 1, 2, 3].map(|i| pad.get(i).copied().unwrap_or(0.0)),
            cell_padding: [0, 1, 2, 3].map(|i| cell_pad.get(i).copied().unwrap_or(0.0)),
            columns: val("VisibleCols").map_or(0, num),
            rows: val("VisibleRows").map_or(0, num),
        });
    }
    // A slider's range and value (`Slider`: whole steps from its least to its most), its
    // orientation (1 across); a combo box's drop-down picture and colours (`Sims3ComboBox`).
    if w.cls == "Slider" {
        let f = |n: &str| val(n).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
        w.slider = Some([f("MinValue"), f("MaxValue"), f("Value"), f("Orientation")]);
    }
    if w.cls.ends_with("ComboBox") {
        w.combo = Some(UiCombo {
            pulldown: props("PullDownBackgroundDrawable").and_then(|p| p.child("object")).and_then(drawable),
            colors: props("Colors").map(|p| p.children.iter().filter(|v| v.name == "value").map(|v| num(&v.text)).collect()).unwrap_or_default(),
            offset: val("PulldownVerticalOffset").and_then(|v| v.trim().parse().ok()).unwrap_or(0.0),
        });
    }
    if w.cls == "FillBarController" {
        w.fill_bar = Some((val("FillDirection").map_or(0, num) as u8, val("MainColor").map_or(0xffff_ffff, num), val("SecondaryColor").map_or(0xffff_ffff, num)));
    }
    if w.cls == "VerticalScrollbar" || w.cls == "HorizontalScrollbar" {
        if let Some(parts) = props("ScrollbarDrawable").and_then(|p| p.child("object")).and_then(|o| o.children.iter().find(|p| p.attr("name") == Some("Drawables"))) {
            w.scrollbar = Some((w.cls == "VerticalScrollbar", val("MinThumbSize").and_then(|v| v.parse().ok()).unwrap_or(16.0), parts.children.iter().map(drawable).collect()));
        }
    }
    if let Some(procs) = props("WinProcs") {
        for p in procs.children.iter().filter(|c| c.name == "object") {
            let pv = |n: &str| p.children.iter().find(|c| c.name == "prop" && c.attr("name") == Some(n)).and_then(|c| c.attr("value"));
            match p.attr("cls") {
                Some("SimpleLayout") => w.place = UiPlace::Simple(pv("Anchor").map_or(0, num) as u8),
                Some("HudLayout") => {
                    let d: Vec<f32> = pv("Dimensions").unwrap_or("1024,768").split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    w.place = UiPlace::Hud(pv("Anchor").map_or(0, num) as u8, [d.first().copied().unwrap_or(1024.0), d.get(1).copied().unwrap_or(768.0)]);
                }
                Some("CenterInParentLayout") => w.place = UiPlace::Center(pv("VerticalSpacing").and_then(|v| v.parse().ok()).unwrap_or(0.5)),
                _ => {}
            }
        }
    }
    if let Some(ch) = props("Children") {
        w.children = ch.children.iter().filter(|c| c.name == "object").filter_map(window).collect();
    }
    Some(w)
}

fn drawable(o: &XNode) -> Option<UiDrawable> {
    let prop = |n: &str| o.children.iter().find(|c| c.name == "prop" && c.attr("name") == Some(n));
    let val = |n: &str| prop(n).and_then(|p| p.attr("value"));
    match o.attr("cls")? {
        "StdDrawable" => {
            let mut images = [0u64; 8];
            if let Some(p) = prop("Image") {
                for (i, v) in p.children.iter().filter(|c| c.name == "value").take(8).enumerate() {
                    images[i] = v.attr("key").map_or(0, image_key);
                }
            }
            let mut borders = [0.0; 4];
            if let Some(s) = prop("ScaleArea").and_then(|p| p.child("struct")) {
                for (i, n) in ["Left", "Top", "Right", "Bottom"].iter().enumerate() {
                    borders[i] = s.children.iter().find(|c| c.attr("name") == Some(n)).and_then(|c| c.attr("value")).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                }
            }
            let glow = prop("GlowMask").and_then(|p| p.attr("key")).map_or(0, image_key);
            Some(UiDrawable::Std { images, scale: val("ScaleType").map_or(0, num) as u8, borders, glow })
        }
        "ImageDrawable" | "IconDrawable" => Some(UiDrawable::Image {
            image: prop("Image").and_then(|p| p.attr("key")).map_or(0, image_key),
            flags: val("ImageDrawableFlags").map_or(0, num),
            halign: val("AlignmentHorizontal").map_or(0, num) as u8,
            valign: val("AlignmentVertical").map_or(0, num) as u8,
            tiling: if o.attr("cls") == Some("ImageDrawable") { val("Tiling").map_or(0, num) as u8 } else { 0 },
            scale: val("Scale").and_then(|v| v.parse().ok()).unwrap_or(1.0),
            colors: prop("StateColors").map(|p| p.children.iter().filter(|v| v.name == "value").map(|v| num(&v.text)).collect()).unwrap_or_default(),
        }),
        "IconButtonMultiDrawable" | "SliderMultiDrawable" | "ComboBoxMultiDrawable" | "ScrollbarMultiDrawable" => {
            let list = prop("Drawables")?;
            Some(UiDrawable::Multi(list.children.iter().filter(|c| c.name == "object").filter_map(drawable).collect()))
        }
        _ => None,
    }
}

/// An image reference (`2f7d0004:00000000:<instance>`), or 0.
fn image_key(k: &str) -> u64 {
    k.rsplit(':').next().and_then(|i| u64::from_str_radix(i, 16).ok()).unwrap_or(0)
}

fn num(s: &str) -> u32 {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).unwrap_or(0),
        None => s.parse::<i64>().map(|v| v as u32).unwrap_or(0),
    }
}

// ---- Text styles ----

struct CssStyle {
    name: String,
    family: String,
    italic: bool,
    size: f32,
    line: f32,
}

/// The style sheet's styles (`Name(0xid) : Parent { font-family: ...; font-size: 8pt; ... }`),
/// each with what it inherits. Point sizes are taken at 96 dpi.
fn parse_styles(sheet: &str) -> Vec<(u32, CssStyle)> {
    // (Comments out, then style by style.)
    let clean: String = sheet.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    let mut raw: Vec<(String, u32, Option<String>, HashMap<String, String>)> = Vec::new();
    let mut rest = clean.as_str();
    while let Some(open) = rest.find('{') {
        let head = rest[..open].trim();
        let Some(close) = rest[open..].find('}') else { break };
        let body = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        // (The last statement of the head is the style's: `@font ...;` lines come before.)
        let head = head.rsplit(';').next().unwrap_or(head).trim();
        let Some(paren) = head.find('(') else { continue };
        let name = head[..paren].trim().to_string();
        let Some(end) = head[paren..].find(')') else { continue };
        let id = num(&head[paren + 1..paren + end]);
        let parent = head[paren + end + 1..].trim().strip_prefix(':').map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        let mut props = HashMap::new();
        for stmt in body.split(';') {
            if let Some((k, v)) = stmt.split_once(':') {
                props.insert(k.trim().to_ascii_lowercase(), v.trim().trim_matches('"').to_string());
            }
        }
        raw.push((name, id, parent, props));
    }
    let by_name: HashMap<&str, usize> = raw.iter().enumerate().map(|(i, r)| (r.0.as_str(), i)).collect();
    let get = |i: usize, key: &str| -> Option<String> {
        let mut at = Some(i);
        for _ in 0..8 {
            let r = &raw[at?];
            if let Some(v) = r.3.get(key) {
                return Some(v.clone());
            }
            at = r.2.as_deref().and_then(|p| by_name.get(p).copied());
        }
        None
    };
    let px = |v: &str| -> Option<f32> {
        let v = v.trim();
        if let Some(p) = v.strip_suffix("pt") {
            p.trim().parse::<f32>().ok().map(|p| p * 4.0 / 3.0)
        } else {
            v.trim_end_matches("px").trim().parse().ok()
        }
    };
    (0..raw.len())
        .map(|i| {
            let size = get(i, "font-size").and_then(|s| px(&s)).unwrap_or(11.0);
            let st = CssStyle {
                name: raw[i].0.clone(),
                family: get(i, "font-family").unwrap_or_default().split(',').next().unwrap_or("").trim().trim_matches('"').to_string(),
                italic: get(i, "font-style").is_some_and(|s| s.contains("italic")),
                size,
                line: get(i, "line-spacing").and_then(|s| px(&s)).unwrap_or(size * 1.3),
            };
            (raw[i].1, st)
        })
        .collect()
}

// ---- Fonts ----

/// A font's family, style, full and PostScript names (its `name` table).
#[derive(Debug, Default)]
struct FontNames {
    family: String,
    full: String,
    postscript: String,
}

fn font_names(d: &[u8]) -> Option<FontNames> {
    let be16 = |o: usize| d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let be32 = |o: usize| d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let tables = be16(4)?;
    let name = (0..tables).find_map(|t| (d.get(12 + 16 * t..16 + 16 * t)? == b"name").then(|| be32(12 + 16 * t + 8)).flatten())?;
    let count = be16(name + 2)?;
    let strings = name + be16(name + 4)?;
    let mut out = FontNames::default();
    for r in 0..count {
        let rec = name + 6 + 12 * r;
        let (platform, id, len, off) = (be16(rec)?, be16(rec + 6)?, be16(rec + 8)?, be16(rec + 10)?);
        let raw = d.get(strings + off..strings + off + len)?;
        let s = if platform == 0 || platform == 3 {
            String::from_utf16_lossy(&raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
        } else {
            raw.iter().map(|&b| b as char).collect()
        };
        let slot = match id {
            1 => &mut out.family,
            4 => &mut out.full,
            6 => &mut out.postscript,
            _ => continue,
        };
        if slot.is_empty() {
            *slot = s;
        }
    }
    (!out.family.is_empty()).then_some(out)
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase()
}

/// The font a style sheet's family name means (`HelveticaRounded LT Std Bd` is the font
/// `HelveticaRoundedLTStd-Bd`; `HelveticaNeueLT Std Med` the only Helvetica Neue there is),
/// its oblique when italic.
fn pick_font(fonts: &[(FontNames, u64)], family: &str, italic: bool) -> Option<u64> {
    let want = squash(family);
    let exact = fonts.iter().find(|(n, _)| squash(&n.postscript) == want || squash(&n.full) == want);
    let base = exact.or_else(|| {
        // (The longest family that starts the name, its plainest style.)
        let mut c: Vec<&(FontNames, u64)> = fonts.iter().filter(|(n, _)| want.starts_with(&squash(&n.family))).collect();
        c.sort_by_key(|(n, _)| (std::cmp::Reverse(squash(&n.family).len()), n.full.contains("Oblique") || n.full.contains("Italic"), n.full.len()));
        c.first().copied()
    })?;
    if italic && let Some(o) = fonts.iter().find(|(n, _)| n.full == format!("{} Oblique", base.0.full) || n.full == format!("{} Italic", base.0.full)) {
        return Some(o.1);
    }
    Some(base.1)
}

// ---- XML ----

/// A parsed XML element.
#[derive(Debug, Default)]
pub struct XNode {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XNode>,
    pub text: String,
}

impl XNode {
    pub fn attr(&self, n: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str())
    }
    pub fn child(&self, n: &str) -> Option<&XNode> {
        self.children.iter().find(|c| c.name == n)
    }
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// A small XML reader: elements, attributes and text (enough for the game's layouts).
pub fn parse_xml(s: &str) -> Option<XNode> {
    let mut stack: Vec<XNode> = vec![XNode::default()];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            let end = s[i..].find('<').map_or(b.len(), |e| i + e);
            let t = s[i..end].trim();
            if !t.is_empty() {
                stack.last_mut()?.text.push_str(&unescape(t));
            }
            i = end;
            continue;
        }
        if s[i..].starts_with("<?") {
            i += s[i..].find("?>")? + 2;
        } else if s[i..].starts_with("<!--") {
            i += s[i..].find("-->")? + 3;
        } else if s[i..].starts_with("<!") {
            i += s[i..].find('>')? + 1;
        } else if s[i..].starts_with("</") {
            i += s[i..].find('>')? + 1;
            let node = stack.pop()?;
            stack.last_mut()?.children.push(node);
        } else {
            // A start tag: its name and attributes (quoted values may hold '>').
            let mut j = i + 1;
            while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let mut node = XNode { name: s[i + 1..j].to_string(), ..Default::default() };
            let mut self_close = false;
            loop {
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= b.len() {
                    return None;
                }
                if b[j] == b'>' {
                    j += 1;
                    break;
                }
                if b[j] == b'/' {
                    self_close = true;
                    j += s[j..].find('>')? + 1;
                    break;
                }
                let k0 = j;
                while j < b.len() && b[j] != b'=' && !b[j].is_ascii_whitespace() {
                    j += 1;
                }
                let key = s[k0..j].to_string();
                while j < b.len() && b[j] != b'"' && b[j] != b'\'' {
                    j += 1;
                }
                let q = *b.get(j)?;
                let v0 = j + 1;
                let v1 = v0 + s[v0..].find(q as char)?;
                node.attrs.push((key, unescape(&s[v0..v1])));
                j = v1 + 1;
            }
            i = j;
            if self_close {
                stack.last_mut()?.children.push(node);
            } else {
                stack.push(node);
            }
        }
    }
    let mut root = stack.into_iter().next()?;
    (!root.children.is_empty()).then(|| root.children.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_flags_keep_expansion_subcategories_and_reject_invalid_bits() {
        let f = ObjBuy { function: 1 << 31, room: 1 << 3, sub: 1 << 63, sub2: 1 | 1 << 63, ..Default::default() };
        assert!(f.in_category(31));
        assert!(!f.in_category(32));
        assert!(f.in_room(3));
        assert!(!f.in_room(32));
        assert!(f.in_sub(63));
        assert!(f.in_sub(64));
        assert!(f.in_sub(127));
        assert!(!f.in_sub(65));
        assert!(!f.in_sub(128));
        assert!(!f.in_sub(255));
    }

    #[test]
    fn reads_scrollbar_skins_without_collapsing_empty_pieces() {
        let xml = r#"<object cls="VerticalScrollbar"><prop name="Area" value="0,0,16,200"/><prop name="MinThumbSize" value="30"/><prop name="ScrollbarDrawable"><object cls="ScrollbarMultiDrawable"><prop name="Drawables"><value type="object"/><object cls="StdDrawable"><prop name="Image"><value key="2f7d0004:00000000:0000000000000001"/></prop></object><value type="object"/><object cls="StdDrawable"><prop name="Image"><value key="2f7d0004:00000000:0000000000000002"/></prop></object></prop></object></prop></object>"#;
        let w = window(&parse_xml(xml).unwrap()).unwrap();
        let (vertical, minimum, parts) = w.scrollbar.as_ref().unwrap();
        assert!(*vertical);
        assert_eq!(*minimum, 30.0);
        assert_eq!(parts.len(), 4);
        assert!(parts[0].is_none());
        assert!(parts[1].is_some());
        assert!(parts[2].is_none());
        assert!(parts[3].is_some());
        let mut images = BTreeSet::new();
        w.images(&mut images);
        assert!(images.contains(&1));
        assert!(images.contains(&2));
    }

    #[test]
    fn reads_a_window() {
        let x = r#"<?xml version="1.0"?><graph class="Layout"><object cls="Window" clsid="0x4ec1b8d8">
            <prop name="WindowFlags" type="uint32" value="0x00002013" />
            <prop name="ControlID" type="uint32" value="0x06f5b840" />
            <prop name="Area" type="rectf" value="0,3,29,133" />
            <prop name="Comment" type="string" value="a &amp; b" />
            <prop name="CaptionColors" type="uint32" count="2"><value>0xff000000</value><value>0xffffffff</value></prop>
            <prop name="WinProcs" type="object" count="1"><object cls="SimpleLayout"><prop name="Anchor" type="uint8" value="6" /></object></prop>
            </object></graph>"#;
        let doc = parse_xml(x).unwrap();
        let w = window(doc.child("object").unwrap()).unwrap();
        assert_eq!(w.id, 0x06f5b840);
        assert_eq!(w.area, [0.0, 3.0, 29.0, 133.0]);
        assert_eq!(w.comment, "a & b");
        assert_eq!(w.colors, vec![0xff000000, 0xffffffff]);
        assert_eq!(w.place, UiPlace::Simple(ANCHOR_BOTTOM | ANCHOR_LEFT));
        assert!(w.visible());
    }

    #[test]
    fn reads_styles() {
        let css = "@font \"X\" ;\nDefaultStyle(0){\n font-family:\"HelveticaRounded LT Std Bd\";\n font-size:8.25pt;\n line-spacing: 14;\n}\n// c\nSims3Bold8pt(0x062eae0b): DefaultStyle{\n font-size:8pt;\n}\n";
        let s = parse_styles(css);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].0, 0x062eae0b);
        assert_eq!(s[1].1.family, "HelveticaRounded LT Std Bd");
        assert!((s[1].1.size - 8.0 * 4.0 / 3.0).abs() < 1e-4);
        assert_eq!(s[1].1.line, 14.0);
    }
}

// ---- The buy catalogue ----

/// Buy mode's catalogue (`BuyCatalog`, XML in UI.package): its categories and their
/// subcategories (each a bit of the objects' function flags, with an icon), the categories the
/// by-function catalogue shows, and the rooms (a bit of the objects' room flags) with their
/// buttons.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BuyCatalogBaked {
    pub categories: Vec<BuyCategory>,
    pub by_category: Vec<String>,
    pub rooms: Vec<BuyRoom>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BuyCategory {
    pub name: String,
    pub label: String,
    /// The bit of the objects' `functionCategoryFlags`.
    pub bit: u8,
    /// Its icon (an interface picture, fnv64 of its name).
    pub image: u64,
    pub subs: Vec<BuySub>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BuySub {
    pub name: String,
    pub label: String,
    /// The bit of the objects' function subcategory flags (64 and up in the second set).
    pub bit: u8,
    pub image: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BuyRoom {
    pub name: String,
    pub label: String,
    /// The bit of the objects' `roomCategoryFlags`.
    pub bit: u8,
    pub image: u64,
    /// The room picture's buttons: their window ids and the room subcategory bits they show.
    pub buttons: Vec<(String, String, u32, Vec<u8>)>,
}

/// An object's buy-mode categories (its OBJD's flags; see `s3formats::object::BuyFlags`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjBuy {
    pub room: u32,
    pub function: u32,
    pub sub: u64,
    pub sub2: u64,
    pub room_sub: u64,
    pub sort: u32,
}

impl ObjBuy {
    /// Whether it's in a function subcategory (by its bit).
    pub fn in_sub(&self, bit: u8) -> bool {
        if bit < 64 { self.sub & (1u64 << bit) != 0 } else { bit < 128 && self.sub2 & (1u64 << (bit - 64)) != 0 }
    }
    pub fn in_category(&self, bit: u8) -> bool {
        bit < 32 && self.function & (1u32 << bit) != 0
    }
    pub fn in_room(&self, bit: u8) -> bool {
        bit < 32 && self.room & (1u32 << bit) != 0
    }
}

fn buy_catalog(xml: &str, strings: &HashMap<u64, String>, images: &mut BTreeSet<u64>) -> BuyCatalogBaked {
    let mut out = BuyCatalogBaked::default();
    let Some(doc) = parse_xml(xml) else { return out };
    let label = |n: &XNode| -> String {
        let key = n.attr("localizedName").unwrap_or_default();
        strings.get(&s3pkg::fnv64(key)).cloned().unwrap_or_else(|| key.rsplit(':').next().unwrap_or(key).to_string())
    };
    let mut image = |n: &XNode| -> u64 {
        match n.attr("image").filter(|s| !s.is_empty()) {
            Some(i) => {
                let k = s3pkg::fnv64(i);
                images.insert(k);
                k
            }
            None => 0,
        }
    };
    let bit = |n: &XNode, a: &str| n.attr(a).and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(0);
    for cats in doc.children.iter().filter(|c| c.name == "Categories") {
        for c in cats.children.iter().filter(|c| c.name == "Category") {
            let subs = c.children.iter().filter(|s| s.name == "SubCategory").map(|s| BuySub { name: s.attr("name").unwrap_or_default().to_string(), label: label(s), bit: bit(s, "flagBit"), image: image(s) }).collect();
            out.categories.push(BuyCategory { name: c.attr("name").unwrap_or_default().to_string(), label: label(c), bit: bit(c, "flagBit"), image: image(c), subs });
        }
    }
    for cats in doc.children.iter().filter(|c| c.name == "Catalogs") {
        for c in cats.children.iter().filter(|c| c.name == "Catalog") {
            image(c);
            match c.attr("type") {
                Some("byCategory") => out.by_category = c.children.iter().filter(|i| i.name == "IncludedCategory").filter_map(|i| i.attr("name")).map(str::to_string).collect(),
                Some("byRoom") => {
                    for r in c.children.iter().filter(|r| r.name == "Room") {
                        let buttons = r
                            .children
                            .iter()
                            .filter(|b| b.name == "RoomButton")
                            .map(|b| {
                                let id = b.attr("buttonId").and_then(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok()).unwrap_or(0);
                                let bits = b.attr("flagBits").unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect();
                                (b.attr("name").unwrap_or_default().to_string(), label(b), id, bits)
                            })
                            .collect();
                        out.rooms.push(BuyRoom { name: r.attr("name").unwrap_or_default().to_string(), label: label(r), bit: bit(r, "flagBit"), image: image(r), buttons });
                    }
                }
                _ => {}
            }
        }
    }
    out
}
