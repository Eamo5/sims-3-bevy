//! Runtime access to the baked asset cache (see the `s3bake` crate). The game reads only
//! these pre-converted files: no package parsing or texture compositing happens in play.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use s3bake::{BakeRoot, BakedModel, CasBaked, CasPartMeshes, CatalogEntry, Clip, Key, PackReader};

pub struct BakedData {
    pub root: BakeRoot,
    pub catalog: Vec<CatalogEntry>,
    pub catalog_index: HashMap<Key, usize>,
    pub models: PackReader,
    pub world_models: Option<PackReader>,
    /// The food recipes make (dishes and plates, full and emptied).
    pub food_models: Option<PackReader>,
    /// Objects with alternative geometry states, in their default one (over `models`).
    pub state_models: Option<PackReader>,
    pub cas: CasBaked,
    pub cas_pack: PackReader,
    pub clips: Option<PackReader>,
    /// Names of the baked clips (for picking variants).
    pub clip_names: Vec<String>,
}

impl BakedData {
    pub fn open(root: BakeRoot, world: Option<&str>) -> Result<Self, String> {
        let g = root.global_dir();
        let catalog: Vec<CatalogEntry> = s3bake::read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog: {e}"))?;
        let catalog_index = catalog.iter().enumerate().map(|(i, c)| (c.objd, i)).collect();
        let models = PackReader::open(&g.join("models.pack")).map_err(|e| format!("models: {e}"))?;
        let world_models = world.and_then(|w| PackReader::open(&root.world_dir(w).join("models.pack")).ok());
        let food_models = PackReader::open(&g.join("food.pack")).ok();
        let state_models = PackReader::open(&g.join("states.pack")).ok();
        let cas: CasBaked = s3bake::read_value(&g.join("cas.bin")).map_err(|e| format!("cas: {e}"))?;
        let cas_pack = PackReader::open(&g.join("cas.pack")).map_err(|e| format!("cas pack: {e}"))?;
        let clips = PackReader::open(&g.join("clips.pack")).ok();
        let clip_names: Vec<String> = s3bake::read_value(&g.join("clip_names.bin")).unwrap_or_default();
        Ok(Self { root, catalog, catalog_index, models, world_models, food_models, state_models, cas, cas_pack, clips, clip_names })
    }

    pub fn model(&self, k: &Key) -> Option<BakedModel> {
        self.state_models
            .as_ref()
            .and_then(|p| p.get(k))
            .or_else(|| self.world_models.as_ref().and_then(|p| p.get(k)))
            .or_else(|| self.models.get(k))
            .or_else(|| self.food_models.as_ref().and_then(|p| p.get(k)))
    }

    pub fn texture_bytes(&self, k: &Key) -> Option<Vec<u8>> {
        std::fs::read(self.root.tex_path(*k)).ok()
    }

    pub fn catalog_entry(&self, objd: &Key) -> Option<&CatalogEntry> {
        self.catalog_index.get(objd).map(|&i| &self.catalog[i])
    }

    /// Decodes a baked animation clip.
    pub fn clip(&self, name: &str) -> Option<Arc<Clip>> {
        let bytes: Vec<u8> = self.clips.as_ref()?.get(&s3bake::clips::clip_key(name))?;
        let raw = lz4_flex::decompress_size_prepended(&bytes).ok()?;
        postcard::from_bytes::<Clip>(&raw).ok().map(Arc::new)
    }

    pub fn cas_meshes(&self, k: &Key) -> Option<CasPartMeshes> {
        self.cas_pack.get(k)
    }
}

/// The opened cache, shared by gameplay systems.
#[derive(Resource, Clone)]
pub struct Baked(pub Arc<BakedData>);
