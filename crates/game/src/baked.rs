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
    pub cas: CasBaked,
    pub cas_pack: PackReader,
    pub clips: HashMap<String, Arc<Clip>>,
}

impl BakedData {
    pub fn open(root: BakeRoot, world: Option<&str>) -> Result<Self, String> {
        let g = root.global_dir();
        let catalog: Vec<CatalogEntry> = s3bake::read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog: {e}"))?;
        let catalog_index = catalog.iter().enumerate().map(|(i, c)| (c.objd, i)).collect();
        let models = PackReader::open(&g.join("models.pack")).map_err(|e| format!("models: {e}"))?;
        let world_models = world.and_then(|w| PackReader::open(&root.world_dir(w).join("models.pack")).ok());
        let cas: CasBaked = s3bake::read_value(&g.join("cas.bin")).map_err(|e| format!("cas: {e}"))?;
        let cas_pack = PackReader::open(&g.join("cas.pack")).map_err(|e| format!("cas pack: {e}"))?;
        let clips: Vec<(String, Clip)> = s3bake::read_value(&g.join("clips.bin")).unwrap_or_default();
        let clips = clips.into_iter().map(|(n, c)| (n, Arc::new(c))).collect();
        Ok(Self { root, catalog, catalog_index, models, world_models, cas, cas_pack, clips })
    }

    pub fn model(&self, k: &Key) -> Option<BakedModel> {
        self.world_models.as_ref().and_then(|p| p.get(k)).or_else(|| self.models.get(k))
    }

    pub fn texture_bytes(&self, k: &Key) -> Option<Vec<u8>> {
        std::fs::read(self.root.tex_path(*k)).ok()
    }

    pub fn catalog_entry(&self, objd: &Key) -> Option<&CatalogEntry> {
        self.catalog_index.get(objd).map(|&i| &self.catalog[i])
    }

    pub fn cas_meshes(&self, k: &Key) -> Option<CasPartMeshes> {
        self.cas_pack.get(k)
    }
}

/// The opened cache, shared by gameplay systems.
#[derive(Resource, Clone)]
pub struct Baked(pub Arc<BakedData>);
