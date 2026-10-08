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
    /// The produce gardens bear.
    pub produce_models: Option<PackReader>,
    /// The build catalogue's fences' pieces.
    pub fence_models: Option<PackReader>,
    pub cas: CasBaked,
    pub cas_pack: PackReader,
    /// The careers' uniforms, and the meshes of their parts the wardrobe doesn't have.
    pub outfits: Vec<s3bake::OutfitInfo>,
    /// The wardrobe's colourways (each part's presets' swatch colours).
    pub colourways: Vec<s3bake::gamedata::CasColourways>,
    /// The face sliders' bone adjustments by age-and-sex prefix.
    pub face_bones: Vec<s3bake::gamedata::FaceBones>,
    /// The eye colour overlay (the iris) and Create a Sim's eye colours.
    pub eye_colors: s3bake::gamedata::EyeColors,
    /// The catalogue objects' designs, by OBJD key.
    pub designs: std::collections::HashMap<Key, s3bake::gamedata::ObjectDesigns>,
    /// The pictures Sims paint, and the canvases (their catalogue objects drawn at their sizes).
    pub paintings: s3bake::gamedata::PaintingsBaked,
    pub canvas_models: Option<PackReader>,
    pub outfit_pack: Option<PackReader>,
    pub clips: Option<PackReader>,
    /// Names of the baked clips (for picking variants).
    pub clip_names: Vec<String>,
    /// Models with moving parts: their rigs and skinning (by MODL key).
    pub skins: Option<PackReader>,
}

impl BakedData {
    pub fn open(root: BakeRoot, world: Option<&str>) -> Result<Self, String> {
        let g = root.global_dir();
        let catalog: Vec<CatalogEntry> = s3bake::read_value(&g.join("catalog.bin")).map_err(|e| format!("catalog: {e}"))?;
        let catalog_index: HashMap<Key, usize> = catalog.iter().enumerate().map(|(i, c)| (c.objd, i)).collect();
        let models = PackReader::open(&g.join("models.pack")).map_err(|e| format!("models: {e}"))?;
        let world_models = world.and_then(|w| PackReader::open(&root.world_dir(w).join("models.pack")).ok());
        let food_models = PackReader::open(&g.join("food.pack")).ok();
        let state_models = PackReader::open(&g.join("states.pack")).ok();
        let produce_models = PackReader::open(&g.join("produce.pack")).ok();
        let fence_models = PackReader::open(&g.join("fences.pack")).ok();
        let mut cas: CasBaked = s3bake::read_value(&g.join("cas.bin")).map_err(|e| format!("cas: {e}"))?;
        // (With the formal wear baked alongside the careers' uniforms.)
        cas.parts.extend(s3bake::read_value::<Vec<s3bake::CasPartInfo>>(&g.join("wardrobe.bin")).unwrap_or_default());
        let cas_pack = PackReader::open(&g.join("cas.pack")).map_err(|e| format!("cas pack: {e}"))?;
        let outfits = s3bake::read_value(&g.join("outfits.bin")).unwrap_or_default();
        let colourways = s3bake::read_value(&g.join("cas_presets.bin")).unwrap_or_default();
        let face_bones = s3bake::read_value(&g.join("face_bones.bin")).unwrap_or_default();
        let eye_colors = s3bake::read_value(&g.join("eye_colors.bin")).unwrap_or_default();
        let mut designs: HashMap<Key, s3bake::gamedata::ObjectDesigns> =
            s3bake::read_value::<Vec<s3bake::gamedata::ObjectDesigns>>(&g.join("object_designs.bin")).unwrap_or_default().into_iter().map(|d| (d.objd, d)).collect();
        // A finished painting is its canvas at its size, its picture a design on its face.
        let paintings: s3bake::gamedata::PaintingsBaked = s3bake::read_value(&g.join("paintings.bin")).unwrap_or_default();
        let canvas_models = PackReader::open(&g.join("canvases.pack")).ok();
        let mut catalog = catalog;
        if canvas_models.is_some() {
            for c in &paintings.canvases {
                if let Some(&i) = catalog_index.get(&c.objd) {
                    catalog[i].models = vec![c.model];
                }
                designs.insert(c.objd, s3bake::gamedata::ObjectDesigns { objd: c.objd, count: 0, texture: paintings.face, ..Default::default() });
            }
        }
        let outfit_pack = PackReader::open(&g.join("outfits.pack")).ok();
        let clips = PackReader::open(&g.join("clips.pack")).ok();
        let clip_names: Vec<String> = s3bake::read_value(&g.join("clip_names.bin")).unwrap_or_default();
        let skins = PackReader::open(&g.join("skins.pack")).ok();
        Ok(Self {
            root,
            catalog,
            catalog_index,
            models,
            world_models,
            food_models,
            state_models,
            produce_models,
            fence_models,
            cas,
            cas_pack,
            outfits,
            colourways,
            outfit_pack,
            clips,
            clip_names,
            face_bones,
            eye_colors,
            designs,
            paintings,
            canvas_models,
            skins,
        })
    }

    /// A model's rig and skinning, when it has moving parts.
    pub fn skin(&self, k: &Key) -> Option<s3bake::gamedata::ObjectSkin> {
        self.skins.as_ref().and_then(|p| p.get(k))
    }

    pub fn model(&self, k: &Key) -> Option<BakedModel> {
        self.state_models
            .as_ref()
            .and_then(|p| p.get(k))
            .or_else(|| self.world_models.as_ref().and_then(|p| p.get(k)))
            .or_else(|| self.models.get(k))
            .or_else(|| self.food_models.as_ref().and_then(|p| p.get(k)))
            .or_else(|| self.produce_models.as_ref().and_then(|p| p.get(k)))
            .or_else(|| self.fence_models.as_ref().and_then(|p| p.get(k)))
            .or_else(|| self.canvas_models.as_ref().and_then(|p| p.get(k)))
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
        self.cas_pack.get(k).or_else(|| self.outfit_pack.as_ref()?.get(k))
    }
}

/// The opened cache, shared by gameplay systems.
#[derive(Resource, Clone)]
pub struct Baked(pub Arc<BakedData>);
