//! Create a Style (in Create a Sim, as the game's): an item of clothing's colours changed
//! channel by channel. Each of the item's preset's solid colour channels (A to D) can take any
//! colour of the palette; the item is then rendered afresh from its preset with those colours
//! (the game's patterns through the compositor, from the installed game, opened the first time
//! it's needed) into the texture store, and worn so. The styles made are kept with the Sim's
//! outfit and in saves.

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

/// The installed game's packages, opened the first time a style is rendered.
static INSTALL: OnceLock<Option<Arc<s3pkg::PackageSet>>> = OnceLock::new();

/// Styles being rendered, for whom (by Sim id).
#[derive(Resource, Default)]
pub struct StyleRenders {
    tasks: Vec<(u64, CustomStyle, Task<bool>)>,
}

impl StyleRenders {
    pub fn busy(&self) -> bool {
        !self.tasks.is_empty()
    }

    /// Renders `style` for the Sim `sim` (from the game at `install`): done, it's announced as a
    /// `StyleReady`.
    pub fn request(&mut self, sim: u64, style: CustomStyle, install: std::path::PathBuf) {
        let s = style.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let Some(pkgs) = INSTALL.get_or_init(|| {
                let set = s3pkg::install::open_install(&install, |_| true);
                (set.len() > 0).then(|| Arc::new(set))
            }) else {
                return false;
            };
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

/// Renders finished are announced (or, failed, dropped with a word in the log).
pub fn poll_styles(mut renders: ResMut<StyleRenders>, mut ready: MessageWriter<StyleReady>) {
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
        app.init_resource::<StyleRenders>().add_message::<StyleReady>().add_systems(Update, poll_styles);
    }
}
