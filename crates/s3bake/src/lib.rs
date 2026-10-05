//! The asset bake: converts the installed game's packages into a local cache of GPU-ready
//! assets (meshes, BC-compressed DDS textures, terrain, placements, CAS parts, rigs, clips),
//! so the game never parses packages or composites textures at runtime.

pub mod bake;
pub mod building;
pub mod clips;
pub mod trees;
pub mod ddsw;
pub mod gamedata;
pub mod pack;
pub mod ponds;
pub mod premades;
pub mod sounds;
pub mod types;

pub use bake::{BakeRoot, bake_clips, bake_global, bake_music, bake_world, clips_ready};
pub use pack::{PackReader, read_value};
pub use gamedata::{GameDataBaked, Icons, bake_gamedata, gamedata_ready, load_gamedata};
pub use premades::{HouseholdBaked, PremadesBaked, ensure_premades, load_premades};
pub use sounds::{SoundBank, SoundDef, bake_sounds, sounds_ready};
pub use types::*;

/// Default location of the cache: `SIMS3_CACHE`, or `baked/` in the working directory.
pub fn default_root() -> BakeRoot {
    BakeRoot::new(std::env::var_os("SIMS3_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| "baked".into()))
}
