//! Create a Style (in Create a Sim, as the game's): an item of clothing's colours changed
//! channel by channel. Each of the item's preset's solid colour channels (A to D) can take any
//! colour of the palette; the item is then rendered afresh from its preset with those colours
//! (the game's patterns through the compositor, from the installed game, opened the first time
//! it's needed) into the texture store, and worn so. The styles made are kept with the Sim's
//! outfit and in saves. Buy mode's is the same for an object in hand: its design's channels
//! recoloured (a patterned material's shifted to the colour, keeping its grain), rendered into
//! the texture store and placed in it, the placed object saved with that design.

use std::sync::{Arc, OnceLock};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use serde::{Deserialize, Serialize};

/// Texture-store type of the styles made in Create a Style.
pub const T_CAS_STYLE: u32 = 0x0CA5_5171;

/// An item's own style: its preset with some of its channels' colours changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomStyle {
    pub part: (u32, u32, u64),
    pub preset: u8,
    pub colours: Vec<(u8, [f32; 3])>,
}

impl CustomStyle {
    /// Where its rendering is kept in the texture store.
    pub fn texture(&self) -> s3bake::Key {
        let desc = format!("{:?}|{}|{:?}", self.part, self.preset, self.colours.iter().map(|(c, v)| (c, v.map(|x| (x * 255.0).round() as u8))).collect::<Vec<_>>());
        (T_CAS_STYLE, 0, s3pkg::fnv64(&desc))
    }

    /// Whether it's been rendered (and so can be worn).
    pub fn ready(&self) -> bool {
        s3bake::default_root().tex_path(self.texture()).exists()
    }
}

/// The palette of colours a channel can take (as the game's colour picker's commonest).
pub const PALETTE: [[f32; 3]; 30] = [
    [0.95, 0.95, 0.95],
    [0.75, 0.75, 0.75],
    [0.5, 0.5, 0.5],
    [0.3, 0.3, 0.3],
    [0.12, 0.12, 0.12],
    [0.03, 0.03, 0.03],
    [0.85, 0.12, 0.12],
    [0.95, 0.45, 0.1],
    [0.95, 0.8, 0.15],
    [0.55, 0.8, 0.2],
    [0.15, 0.6, 0.25],
    [0.1, 0.65, 0.6],
    [0.15, 0.55, 0.85],
    [0.15, 0.25, 0.7],
    [0.45, 0.2, 0.7],
    [0.75, 0.25, 0.65],
    [0.95, 0.55, 0.7],
    [0.55, 0.35, 0.2],
    [0.45, 0.06, 0.08],
    [0.55, 0.25, 0.05],
    [0.6, 0.5, 0.1],
    [0.3, 0.45, 0.1],
    [0.05, 0.3, 0.12],
    [0.05, 0.3, 0.3],
    [0.08, 0.25, 0.45],
    [0.06, 0.1, 0.3],
    [0.25, 0.1, 0.35],
    [0.4, 0.1, 0.3],
    [0.85, 0.75, 0.6],
    [0.95, 0.9, 0.75],
];

/// Texture-store type of the styles made for objects in Buy mode's Create a Style.
pub const T_OBJ_STYLE: u32 = 0x0B1E_5171;

/// An object's own style: one of its designs with some of its channels' colours changed.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectStyle {
    pub objd: s3bake::Key,
    pub design: u8,
    pub colours: Vec<(u8, [f32; 3])>,
}

impl ObjectStyle {
    /// Where its rendering is kept in the texture store (a placed object's design is saved by
    /// this key).
    pub fn texture(&self) -> s3bake::Key {
        let desc = format!("{:?}|{}|{:?}", self.objd, self.design, self.colours.iter().map(|(c, v)| (c, v.map(|x| (x * 255.0).round() as u8))).collect::<Vec<_>>());
        (T_OBJ_STYLE, 0, s3pkg::fnv64(&desc))
    }
}

/// Texture-store type of the styles made for walls and floors in Build mode's Create a Style.
pub const T_COVER_STYLE: u32 = 0x0C0E_5171;

/// A wall or floor pattern's own style: one of its swatches with some of its channels' colours
/// changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverStyle {
    pub cwal: u64,
    pub swatch: u8,
    pub floor: bool,
    pub colours: Vec<(u8, [f32; 3])>,
}

impl CoverStyle {
    /// Where its rendering is kept in the texture store (walls and floors painted in it are
    /// saved by this key).
    pub fn texture(&self) -> s3bake::Key {
        let desc = format!("{:X}|{}|{:?}", self.cwal, self.swatch, self.colours.iter().map(|(c, v)| (c, v.map(|x| (x * 255.0).round() as u8))).collect::<Vec<_>>());
        (T_COVER_STYLE, if self.floor { 4 } else { 3 }, s3pkg::fnv64(&desc))
    }

    /// Recipes travel with cached textures so sampling a saved custom covering restores its
    /// source catalogue pattern, preset and editable channels, rather than only its pixels.
    pub fn from_texture(key: s3bake::Key) -> Option<Self> {
        if key.0 != T_COVER_STYLE { return None; }
        let bytes = std::fs::read(s3bake::default_root().tex_path(key).with_extension("style.json")).ok()?;
        Self::decode_recipe(key, &bytes)
    }

    fn decode_recipe(key: s3bake::Key, bytes: &[u8]) -> Option<Self> {
        let style: Self = serde_json::from_slice(bytes).ok()?;
        (style.texture() == key && style.colours.iter().all(|(channel, rgb)| *channel < 4 && rgb.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))).then_some(style)
    }
}

#[cfg(test)]
mod cover_recipe_tests {
    use super::*;

    #[test]
    fn recipes_restore_source_preset_and_channels_only_for_their_own_texture() {
        let style = CoverStyle { cwal: 0x12345678, swatch: 2, floor: true, colours: vec![(0, [0.25, 0.5, 0.75]), (3, [1.0, 0.0, 0.0])] };
        let bytes = serde_json::to_vec(&style).unwrap();
        assert_eq!(CoverStyle::decode_recipe(style.texture(), &bytes), Some(style.clone()));
        let mut other = style.clone();
        other.cwal += 1;
        assert!(CoverStyle::decode_recipe(other.texture(), &bytes).is_none());
        other = style.clone();
        other.floor = false;
        assert!(CoverStyle::decode_recipe(other.texture(), &bytes).is_none());
        assert!(CoverStyle::decode_recipe(style.texture(), b"incomplete").is_none());
        let mut invalid = style;
        invalid.colours[0].1[0] = 2.0;
        assert!(CoverStyle::decode_recipe(invalid.texture(), &serde_json::to_vec(&invalid).unwrap()).is_none());
    }
}

/// The installed game's packages, opened the first time a style is rendered.
static INSTALL: OnceLock<Option<Arc<s3pkg::PackageSet>>> = OnceLock::new();

fn install(path: &std::path::Path) -> Option<&'static Arc<s3pkg::PackageSet>> {
    INSTALL
        .get_or_init(|| {
            let set = s3pkg::install::open_install(path, |_| true);
            (set.len() > 0).then(|| Arc::new(set))
        })
        .as_ref()
}

/// Styles being rendered, for whom (by Sim id).
#[derive(Resource, Default)]
pub struct StyleRenders {
    tasks: Vec<(u64, CustomStyle, Task<bool>)>,
    objects: Vec<(ObjectStyle, Task<bool>)>,
    covers: Vec<(CoverStyle, Task<bool>)>,
}

impl StyleRenders {
    pub fn busy(&self) -> bool {
        !self.tasks.is_empty() || !self.objects.is_empty() || !self.covers.is_empty()
    }

    /// Renders a wall or floor pattern's `style` (as its swatches are drawn): done, it's
    /// announced as a `CoverStyleReady`.
    pub fn request_cover(&mut self, style: CoverStyle, path: std::path::PathBuf) {
        let s = style.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let Some(pkgs) = install(&path) else { return false };
            let Some(p) = pkgs.read_ti(s3formats::catalog::T_CWAL, s.cwal).and_then(|d| s3formats::catalog::WallFloorPattern::parse(&d).ok()) else { return false };
            // (The swatch: the pattern's presets as offered, the same ones skipped.)
            let mut seen: Vec<String> = Vec::new();
            let Some(m) = p
                .materials
                .iter()
                .filter(|m| {
                    let sig = format!("{:?}", m.complate.blocks);
                    let new = !seen.contains(&sig);
                    seen.push(sig);
                    new
                })
                .nth(s.swatch as usize)
            else {
                return false;
            };
            let c = m.complate.with_colours(&s.colours);
            let (w, h) = if s.floor { (256, 256) } else { (256, 512) };
            let Some(img) = s3formats::complate::render(pkgs, &c, &m.keys, w, h) else { return false };
            let path = s3bake::default_root().tex_path(s.texture());
            let Ok(recipe) = serde_json::to_vec(&s) else { return false };
            std::fs::write(&path, s3bake::ddsw::encode_dds(&img)).is_ok()
                && std::fs::write(path.with_extension("style.json"), recipe).is_ok()
        });
        self.covers.push((style, task));
    }

    /// Renders an object's `style` (at `size`, as its designs are drawn): done, it's announced
    /// as an `ObjectStyleReady`.
    pub fn request_object(&mut self, style: ObjectStyle, size: (u16, u16), path: std::path::PathBuf) {
        let s = style.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let Some(pkgs) = install(&path) else { return false };
            let key = s3pkg::ResourceKey::new(s.objd.0, s.objd.1, s.objd.2);
            let Some(o) = pkgs.read(&key).and_then(|d| s3formats::object::parse_objd(&d).ok()) else { return false };
            let Some(p) = o.presets.get(s.design as usize) else { return false };
            let c = p.complate.with_colours(&s.colours);
            let (w, h) = (size.0.max(16) as usize, size.1.max(16) as usize);
            let Some(img) = s3formats::complate::render(pkgs, &c, &p.keys, w, h) else { return false };
            std::fs::write(s3bake::default_root().tex_path(s.texture()), s3bake::ddsw::encode_dds(&img)).is_ok()
        });
        self.objects.push((style, task));
    }

    /// Renders `style` for the Sim `sim` (from the game at `install`): done, it's announced as a
    /// `StyleReady`.
    pub fn request(&mut self, sim: u64, style: CustomStyle, install: std::path::PathBuf) {
        let s = style.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let Some(pkgs) = self::install(&install) else { return false };
            let key = s3pkg::ResourceKey::new(s.part.0, s.part.1, s.part.2);
            let Some(c) = pkgs.read(&key).and_then(|d| s3formats::sim::CasPart::parse(&d).ok()) else { return false };
            let Some(xml) = c.presets.get(s.preset as usize) else { return false };
            let xml = s3formats::complate::with_colours(xml, &s.colours);
            let Some(img) = s3formats::complate::render_preset(pkgs, &xml, 256, true) else { return false };
            std::fs::write(s3bake::default_root().tex_path(s.texture()), s3bake::ddsw::encode_dds(&img)).is_ok()
        });
        self.tasks.push((sim, style, task));
    }
}

/// A style rendered, for the Sim (by id) to wear.
#[derive(Message)]
pub struct StyleReady {
    pub sim: u64,
    pub style: CustomStyle,
}

impl StyleReady {
    pub fn style_sim(&self) -> u64 {
        self.sim
    }
}

/// An object's style rendered, for the object in hand to be in.
#[derive(Message)]
pub struct ObjectStyleReady(pub ObjectStyle);

/// A wall or floor pattern's style rendered, to paint with.
#[derive(Message)]
pub struct CoverStyleReady(pub CoverStyle);

/// Renders finished are announced (or, failed, dropped with a word in the log).
pub fn poll_styles(mut renders: ResMut<StyleRenders>, mut ready: MessageWriter<StyleReady>, mut objects: MessageWriter<ObjectStyleReady>, mut covers: MessageWriter<CoverStyleReady>) {
    let mut i = 0;
    while i < renders.covers.len() {
        let Some(ok) = block_on(poll_once(&mut renders.covers[i].1)) else {
            i += 1;
            continue;
        };
        let (style, _) = renders.covers.remove(i);
        if ok {
            covers.write(CoverStyleReady(style));
        } else {
            warn!("Create a Style: couldn't render the pattern {:X}", style.cwal);
        }
    }
    let mut i = 0;
    while i < renders.objects.len() {
        let Some(ok) = block_on(poll_once(&mut renders.objects[i].1)) else {
            i += 1;
            continue;
        };
        let (style, _) = renders.objects.remove(i);
        if ok {
            objects.write(ObjectStyleReady(style));
        } else {
            warn!("Create a Style: couldn't render the object {:?}", style.objd);
        }
    }
    let mut i = 0;
    while i < renders.tasks.len() {
        let Some(ok) = block_on(poll_once(&mut renders.tasks[i].2)) else {
            i += 1;
            continue;
        };
        let (sim, style, _) = renders.tasks.remove(i);
        if ok {
            ready.write(StyleReady { sim, style });
        } else {
            warn!("Create a Style: couldn't render {:?}", style.part);
        }
    }
}

pub struct StylePlugin;

impl Plugin for StylePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StyleRenders>().add_message::<StyleReady>().add_message::<ObjectStyleReady>().add_message::<CoverStyleReady>().add_systems(Update, poll_styles);
    }
}
