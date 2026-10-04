//! The asset bake: converts the installed game's packages into a local cache of GPU-ready
//! assets (meshes, BC-compressed DDS textures, terrain, placements, CAS parts, rigs, clips),
//! so the game never parses packages or composites textures at runtime.

pub mod bake;
pub mod ddsw;
pub mod pack;
pub mod types;

pub use bake::{BakeRoot, CLIP_NAMES, bake_global, bake_world};
pub use pack::{PackReader, read_value};
pub use types::*;

/// Default location of the cache: `SIMS3_CACHE`, or `baked/` in the working directory.
pub fn default_root() -> BakeRoot {
    BakeRoot::new(std::env::var_os("SIMS3_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| "baked".into()))
}
