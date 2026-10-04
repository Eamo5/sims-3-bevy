//! Converts the installed game's data into the baked cache.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use s3formats::model::{self, MeshData, P_DIFFUSE_MAP};
use s3formats::object::{object_models, objd_objk, parse_objd};
use s3formats::sim::*;
use s3formats::world::WorldData;
use s3pkg::{Package, PackageSet, ResourceKey, types};

use crate::ddsw::{encode_dds, trim_dds};
use crate::pack::{PackReader, PackWriter, read_value, write_value};
use crate::types::*;

pub type Progress<'a> = &'a (dyn Fn(&str) + Sync);

/// Largest texture edge kept for objects (keeps the cache compact and loads fast).
pub const OBJECT_TEX_MAX: u32 = 512;

/// Animation clips the game uses (baked up front).
pub const CLIP_NAMES: &[&str] = &[
    "a_female_walk",
    "a_male_walk",
    "a_idle_neutral_loop_1",
    "a_idle_neutral_loop_2",
    "a_idle_neutral_loop_3",
    "a_idle_friendly_loop_1",
    "a_idle_friendly_loop_2",
    "a_dance_beg_posAHeadBob_x",
    "a2o_chairLiving_sit_breathe_loop_x",
    "a2o_sitTemplate_sit_loopBreathe",
    "a2o_bed_sleep_back_loop_x",
    "a2o_treadmill_jog_loop_x",
    "a2o_fridge_openDoor_x",
    "a2o_stove_clean_loop_x",
    "a2o_shower_takeShower_loop1_x",
    "a2o_sink_brushTeeth_Loop1_x",
    "a2o_computer_game_loop1_counter_x",
    "a2o_computer_chess_type_loop_counter_x",
    "a2o_book_readBook_standing_loopRead_x",
    "a2o_mirror_full_checkSelfOut_loop1_x",
    "a2o_holographicEasel_loopMed_1_x",
    "a2o_guitar_play_high_loop1_x",
    "a2o_chessTable_loop1_x",
];

pub struct BakeRoot {
    pub dir: PathBuf,
}

impl BakeRoot {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
    pub fn global_dir(&self) -> PathBuf {
        self.dir.join("global")
    }
    pub fn textures_dir(&self) -> PathBuf {
        self.dir.join("textures")
    }
    pub fn world_dir(&self, world: &str) -> PathBuf {
        self.dir.join("worlds").join(world)
    }
    pub fn tex_path(&self, k: Key) -> PathBuf {
        self.textures_dir().join(format!("{:08X}_{:08X}_{:016X}.dds", k.0, k.1, k.2))
    }
    pub fn global_manifest(&self) -> Option<GlobalManifest> {
        let m: GlobalManifest = serde_json::from_slice(&std::fs::read(self.global_dir().join("manifest.json")).ok()?).ok()?;
        (m.version == BAKE_VERSION).then_some(m)
    }
    pub fn world_ready(&self, world: &str) -> bool {
        let p = self.world_dir(world).join("world.bin");
        std::fs::read(&p).ok().is_some_and(|d| {
            postcard::take_from_bytes::<u32>(&d).is_ok_and(|(v, _)| v == WORLD_VERSION)
        }) && self.world_dir(world).join("models.pack").exists()
    }
}

/// Runs `f` over `items` on all CPU cores.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(24);
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items.chunks(chunk).map(|c| s.spawn(|| c.iter().map(&f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

fn bounds_ok(m: &MeshData) -> bool {
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for p in &m.positions {
        for a in 0..3 {
            mn[a] = mn[a].min(p[a]);
            mx[a] = mx[a].max(p[a]);
        }
    }
    (0..3).all(|a| (mn[a] - m.bounds_min[a]).abs() < 0.05 && (mx[a] - m.bounds_max[a]).abs() < 0.05)
}

const P_IMPOSTER_TEXTURE: u32 = 0xBDCF71C5;

/// Decodes a MODL into ready-to-upload parts.
pub fn bake_model(pkgs: &PackageSet, modl: &ResourceKey) -> BakedModel {
    let meshes = model::load_model(pkgs, modl).unwrap_or_default();
    let mut parts = Vec::new();
    for m in &meshes {
        if m.indices.is_empty() || !bounds_ok(m) {
            continue;
        }
        let mat = &m.material;
        let imposter = mat.shader == model::SHADER_LOT_IMPOSTER;
        let tex = if imposter {
            mat.texture(P_IMPOSTER_TEXTURE).or_else(|| mat.texture(P_DIFFUSE_MAP))
        } else {
            mat.texture(P_DIFFUSE_MAP)
        };
        let mode = if mat.is_alpha_blended() {
            2
        } else if mat.is_alpha_tested() {
            1
        } else {
            0
        };
        parts.push(BakedPart {
            positions: m.positions.clone(),
            normals: m.normals.clone(),
            uvs: m.uvs.clone(),
            indices: m.indices.clone(),
            texture: tex.map(|k| key_of(&k)),
            mode,
            unlit: imposter,
            bmin: m.bounds_min,
            bmax: m.bounds_max,
        });
    }
    BakedModel { parts }
}

/// Produces a GPU-ready DDS for a texture reference (DDS: trimmed; TXTC: composited + BC-encoded).
pub fn bake_texture(pkgs: &PackageSet, key: Key, max: u32, layer_mode: bool) -> Option<Vec<u8>> {
    let rk = rkey(key);
    let data = pkgs.read(&rk).or_else(|| pkgs.read_ti(rk.t, rk.i))?;
    match rk.t {
        types::DDS => Some(trim_dds(&data, max)),
        types::TXTC => {
            let t = s3formats::txtc::Txtc::parse(&data).ok()?;
            let mut c = s3formats::compositor::Compositor::new(pkgs);
            c.max_size = max as usize;
            c.layer_mode = layer_mode;
            let (w, h) = c.output_size(&t);
            let (w, h) = (w.min(max as usize), h.min(max as usize));
            c.layer_mode = layer_mode;
            Some(encode_dds(&c.run(&t, w, h)))
        }
        _ => None,
    }
}

fn bake_textures(root: &BakeRoot, pkgs: &PackageSet, keys: &[(Key, bool)], max: u32, label: &str, progress: Progress) -> usize {
    std::fs::create_dir_all(root.textures_dir()).ok();
    let todo: Vec<(Key, bool)> = keys.iter().copied().filter(|(k, _)| !root.tex_path(*k).exists()).collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = todo.len();
    let ok = par_map(&todo, |(k, layer)| {
        let r = bake_texture(pkgs, *k, max, *layer).map(|d| std::fs::write(root.tex_path(*k), d).is_ok()).unwrap_or(false);
        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n % 100 == 0 || n == total {
            progress(&format!("{label}: textures {n}/{total}"));
        }
        r
    });
    ok.into_iter().filter(|b| *b).count() + (keys.len() - todo.len())
}

fn write_models(path: &Path, pkgs: &PackageSet, keys: &[Key], label: &str, progress: Progress) -> Result<(usize, Vec<(Key, bool)>), String> {
    let mut w = PackWriter::create(path).map_err(|e| e.to_string())?;
    let mut tex = HashSet::new();
    let mut n = 0;
    for (ci, chunk) in keys.chunks(400).enumerate() {
        progress(&format!("{label}: models {}/{}", (ci * 400 + chunk.len()).min(keys.len()), keys.len()));
        let baked = par_map(chunk, |k| (*k, bake_model(pkgs, &rkey(*k))));
        for (k, m) in baked {
            for p in &m.parts {
                if let Some(t) = p.texture {
                    tex.insert(t);
                }
            }
            w.add(k, &m).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok((n, tex.into_iter().map(|k| (k, false)).collect()))
}

// ---------------------------------------------------------------------------------------------
// Global bake: catalog, object models and textures, CAS, rigs, clips, strings

fn human(age_gender: u32) -> bool {
    matches!((age_gender >> 8) & 0xF, 0 | 1)
}

/// Whether a CAS part is part of the random everyday wardrobe (or is a face/scalp).
pub fn cas_bake_wanted(name: &str, ct: u32, age_gender: u32, category: u32) -> bool {
    if !human(age_gender) || category & CAT_HIDDEN != 0 || age_gender & (AGE_CHILD | AGE_TEEN | AGE_YOUNG_ADULT | AGE_ADULT | AGE_ELDER) == 0 {
        return false;
    }
    match ct {
        CT_FACE => name.ends_with("Face"),
        CT_SCALP => name.ends_with("Scalp"),
        CT_HAIR | CT_BODY | CT_TOP | CT_BOTTOM | CT_SHOES => {
            category & CAT_EVERYDAY != 0
                && category & CAT_VALID_RANDOM != 0
                && category & 0x400000 == 0
                && !name.contains("Nude")
                && !name.to_ascii_lowercase().contains("hat")
        }
        _ => false,
    }
}

fn skin_mesh(g: &Geom, rig: &Rig) -> SkinMesh {
    let palette: Vec<u16> = g.bone_hashes.iter().map(|h| rig.index_of(*h).unwrap_or(0) as u16).collect();
    let mut joints = Vec::with_capacity(g.positions.len());
    let mut weights = Vec::with_capacity(g.positions.len());
    for (bi, bw) in g.bone_indices.iter().zip(&g.weights) {
        let mut j = [0u16; 4];
        let mut w = *bw;
        for k in 0..4 {
            j[k] = palette.get(bi[k] as usize).copied().unwrap_or(0);
            if bi[k] as usize >= palette.len() {
                w[k] = 0.0;
            }
        }
        let s: f32 = w.iter().sum();
        if s > 1e-4 {
            for x in &mut w {
                *x /= s;
            }
        } else {
            w = [1.0, 0.0, 0.0, 0.0];
            j = [1, 0, 0, 0];
        }
        joints.push(j);
        weights.push(w);
    }
    let texture = match g.params.get(&P_DIFFUSE_MAP) {
        Some(s3formats::model::ParamValue::Texture(k)) => Some(key_of(k)),
        _ => None,
    };
    SkinMesh {
        positions: g.positions.clone(),
        normals: g.normals.clone(),
        uvs: g.uvs.clone(),
        indices: g.indices.clone(),
        joints,
        weights,
        shader: g.shader,
        texture,
    }
}

pub fn bake_global(root: &BakeRoot, pkgs: &PackageSet, install_root: &str, progress: Progress) -> Result<GlobalManifest, String> {
    let gdir = root.global_dir();
    std::fs::create_dir_all(&gdir).map_err(|e| e.to_string())?;

    progress("Converting: string tables…");
    let strings = s3formats::stbl::load_english(pkgs);
    write_value(&gdir.join("strings.bin"), &strings).map_err(|e| e.to_string())?;

    progress("Converting: buy catalog…");
    let mut objds: Vec<ResourceKey> = pkgs.keys_of_type(types::OBJD).copied().collect();
    objds.sort();
    let catalog: Vec<CatalogEntry> = par_map(&objds, |k| {
        let d = pkgs.read(k)?;
        let info = parse_objd(&d).ok()?;
        let script = objd_objk(pkgs, &d).and_then(|o| o.script_class).unwrap_or_default();
        let models: Vec<Key> = object_models(pkgs, k).iter().map(key_of).collect();
        Some(CatalogEntry {
            objd: key_of(k),
            name: strings.get(&info.name_guid).cloned().unwrap_or_else(|| info.instance_name.clone()),
            price: if info.show_in_catalog { info.price as i32 } else { -1 },
            script,
            instance_name: info.instance_name,
            models,
        })
    })
    .into_iter()
    .flatten()
    .collect();
    write_value(&gdir.join("catalog.bin"), &catalog).map_err(|e| e.to_string())?;

    let mut model_keys: Vec<Key> = catalog.iter().flat_map(|c| c.models.iter().copied()).collect::<HashSet<_>>().into_iter().collect();
    model_keys.sort();
    let (n_models, tex) = write_models(&gdir.join("models.pack"), pkgs, &model_keys, "Converting objects", progress)?;
    let n_tex = bake_textures(root, pkgs, &tex, OBJECT_TEX_MAX, "Converting objects", progress);

    // CAS: index every part, bake meshes and layers for the wardrobe sims are dressed from.
    progress("Converting: Create-a-Sim parts…");
    let rig_of = |name: &str| pkgs.read_ti(types::RIG, s3pkg::fnv64(name)).and_then(|d| Rig::parse(&d).ok());
    let adult_rig = rig_of("auRig");
    let child_rig = rig_of("cuRig");
    let casp_keys: Vec<ResourceKey> = pkgs.keys_of_type(types::CASP).copied().collect();
    let parsed: Vec<(ResourceKey, CasPart)> =
        par_map(&casp_keys, |k| Some((*k, CasPart::parse(&pkgs.read(k)?).ok()?))).into_iter().flatten().collect();
    let mut infos = Vec::with_capacity(parsed.len());
    let mut cas_tex: Vec<(Key, bool)> = Vec::new();
    let mut cpack = PackWriter::create(&gdir.join("cas.pack")).map_err(|e| e.to_string())?;
    let wanted: Vec<&(ResourceKey, CasPart)> =
        parsed.iter().filter(|(_, c)| cas_bake_wanted(&c.name, c.clothing_type, c.age_gender, c.category)).collect();
    let rig = adult_rig.clone().ok_or("adult rig (auRig) not found")?;
    let meshes: HashMap<Key, CasPartMeshes> = par_map(&wanted, |(k, c)| {
        let geoms: Vec<SkinMesh> = c
            .lod0_geoms(pkgs)
            .iter()
            .filter_map(|g| Geom::parse(&pkgs.read(g).or_else(|| pkgs.read_ti(g.t, g.i))?).ok())
            .map(|g| skin_mesh(&g, &rig))
            .collect();
        (key_of(k), CasPartMeshes { meshes: geoms })
    })
    .into_iter()
    .collect();
    for (k, c) in &parsed {
        let key = key_of(k);
        let baked = meshes.get(&key).is_some_and(|m| !m.meshes.is_empty());
        let layer = if baked { c.diffuse.first().map(key_of) } else { None };
        if let Some(m) = meshes.get(&key).filter(|_| baked) {
            cpack.add(key, m).map_err(|e| e.to_string())?;
            for sm in &m.meshes {
                if let Some(t) = sm.texture {
                    cas_tex.push((t, false));
                }
            }
            if let Some(l) = layer {
                cas_tex.push((l, true));
            }
        }
        infos.push(CasPartInfo {
            key,
            name: c.name.clone(),
            clothing_type: c.clothing_type,
            age_gender: c.age_gender,
            category: c.category,
            baked,
            layer,
        });
    }
    cpack.finish().map_err(|e| e.to_string())?;

    // Skin tone detail textures and ramp.
    let mut tone = ToneBaked::default();
    let mut tones: Vec<ResourceKey> = pkgs.keys_of_type(T_TONE).copied().collect();
    tones.sort();
    if let Some(t) = tones.iter().find_map(|k| SkinTone::parse(&pkgs.read(k)?).ok()) {
        for tt in &t.textures {
            if let Some(dl) = tt.detail_light {
                tone.textures.push((tt.age_gender, tt.type_flags, key_of(&dl)));
                cas_tex.push((key_of(&dl), false));
            }
        }
        if let Some(ramp) = t.ramp.and_then(|k| s3formats::dds::decode(&pkgs.read(&k)?, 64)) {
            for i in 0..32 {
                let v = 0.03 + i as f32 / 31.0 * 0.94;
                let c = ramp.sample(0.5, v);
                tone.ramp.push([c[0], c[1], c[2]]);
            }
        }
    }
    cas_tex.sort();
    cas_tex.dedup();
    let n_cas_tex = bake_textures(root, pkgs, &cas_tex, 512, "Converting Sims", progress);
    let cas = CasBaked { parts: infos, tone, adult_rig, child_rig };
    write_value(&gdir.join("cas.bin"), &cas).map_err(|e| e.to_string())?;

    progress("Converting: animations…");
    let clips: Vec<(String, Clip)> = CLIP_NAMES
        .iter()
        .filter_map(|n| {
            let inst = s3pkg::fnv64(n) & 0x7FFF_FFFF_FFFF_FFFF;
            let c = Clip::parse(&pkgs.read_ti(types::CLIP, inst)?).ok()?;
            Some((n.to_string(), c))
        })
        .collect();
    write_value(&gdir.join("clips.bin"), &clips).map_err(|e| e.to_string())?;

    let manifest = GlobalManifest {
        version: BAKE_VERSION,
        install_root: install_root.to_string(),
        catalog_entries: catalog.len(),
        models: n_models,
        textures: n_tex + n_cas_tex,
        cas_parts: meshes.len(),
        clips: clips.len(),
    };
    std::fs::write(gdir.join("manifest.json"), serde_json::to_vec_pretty(&manifest).unwrap()).map_err(|e| e.to_string())?;
    Ok(manifest)
}

// ---------------------------------------------------------------------------------------------
// World bake: terrain, lots, placed objects, imposters, trees

fn dds_info(d: &[u8]) -> Option<(u32, u32, u32, [u8; 4], usize)> {
    if d.len() < 128 || &d[0..4] != b"DDS " {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let fourcc: [u8; 4] = d[84..88].try_into().unwrap();
    Some((u(16), u(12), u(28).max(1), fourcc, if &fourcc == b"DX10" { 148 } else { 128 }))
}

/// Terrain paint layers as one BC3 texture array (BC1 layers are promoted to BC3).
fn layer_array(dds: &[Option<Vec<u8>>]) -> Option<((u32, u32, u32, u32), Vec<u8>)> {
    let promote = |d: &Vec<u8>| -> Option<Vec<u8>> {
        let (w, h, m, fcc, off) = dds_info(d)?;
        let _ = (w, h, m);
        match &fcc {
            b"DXT5" => Some(d[off..].to_vec()),
            b"DXT1" => {
                let mut out = Vec::with_capacity((d.len() - off) * 2);
                for b in d[off..].chunks_exact(8) {
                    out.extend_from_slice(&[0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
                    out.extend_from_slice(b);
                }
                Some(out)
            }
            _ => None,
        }
    };
    let (fw, fh, fm, _, _) = dds.iter().flatten().find_map(|d| dds_info(d))?;
    let first = dds.iter().flatten().find_map(|d| promote(d).filter(|_| dds_info(d).is_some_and(|i| i.0 == fw && i.1 == fh && i.2 == fm)))?;
    let mut data = Vec::with_capacity(first.len() * dds.len());
    for d in dds {
        let layer = d
            .as_ref()
            .filter(|d| dds_info(d).is_some_and(|i| i.0 == fw && i.1 == fh && i.2 == fm))
            .and_then(promote)
            .filter(|l| l.len() == first.len());
        data.extend_from_slice(layer.as_deref().unwrap_or(&first));
    }
    Some(((fw, fh, fm, dds.len() as u32), data))
}

pub fn bake_world(root: &BakeRoot, pkgs: &PackageSet, world_path: &Path, name: &str, progress: Progress) -> Result<(), String> {
    // Build into a scratch folder and swap it in at the end, so a failed bake never
    // destroys a working cache.
    let final_dir = root.world_dir(name);
    let wdir = final_dir.with_extension("baking");
    let _ = std::fs::remove_dir_all(&wdir);
    std::fs::create_dir_all(&wdir).map_err(|e| e.to_string())?;
    progress(&format!("Converting {name}: terrain…"));
    let pkg = Package::open(world_path).map_err(|e| e.to_string())?;
    let world = WorldData::load(&pkg)?;

    let layer_dds: Vec<Option<Vec<u8>>> = world
        .paint
        .as_ref()
        .map(|p| p.layers.iter().map(|l| pkgs.read(&l.texture).or_else(|| pkgs.read_ti(l.texture.t, l.texture.i))).collect())
        .unwrap_or_default();
    let (layer_dims, layer_data) = layer_array(&layer_dds).unwrap_or(((0, 0, 0, 0), Vec::new()));
    let layer_avg: Vec<[f32; 3]> = layer_dds
        .iter()
        .map(|d| d.as_deref().and_then(|d| s3formats::dds::decode(d, 16)).map(|img| average_linear(&img)).unwrap_or([0.2, 0.25, 0.12]))
        .collect();

    let size = (world.heightmap.width - 1) as u32;
    let px = (size * size) as usize;
    let mut weights = vec![0u8; px * 16];
    if let Some(blend) = &world.blend
        && blend.width as u32 == size
    {
        for (li, m) in blend.layers.iter().enumerate().take(16) {
            let (g, c) = (li / 4, li % 4);
            for (i, v) in m.iter().enumerate() {
                weights[g * px * 4 + i * 4 + c] = *v;
            }
        }
    }
    let weights_lz4 = lz4_flex::compress_prepend_size(&weights);

    let sea_level = pkg
        .of_type(0xB074ACE6)
        .next()
        .and_then(|e| pkg.read(e).ok())
        .and_then(|d| d.get(2..6).map(|b| f32::from_le_bytes(b.try_into().unwrap())))
        .filter(|v| v.is_finite() && *v > 0.0 && *v < 500.0)
        .unwrap_or(28.07);

    let strings: HashMap<u64, String> = read_value(&root.global_dir().join("strings.bin")).unwrap_or_default();
    let lookup = |k: &str| strings.get(&s3pkg::fnv64(k)).cloned();
    let lots: Vec<LotBaked> = world
        .lots
        .iter()
        .map(|l| {
            let name = l.name_key().and_then(lookup);
            let addr = l.address_key().and_then(lookup);
            let display_name = match (name, addr) {
                (Some(n), Some(a)) if n != a => format!("{n} — {a}"),
                (Some(n), _) => n,
                (None, Some(a)) => a,
                _ => l.internal_name.clone(),
            };
            LotBaked { info: l.clone(), display_name }
        })
        .collect();

    progress(&format!("Converting {name}: placed objects…"));
    let placed = s3formats::objn::load_world_objects(&pkg);
    let lot_ids: HashMap<u64, usize> = world.lots.iter().enumerate().map(|(i, l)| (l.id, i)).collect();
    let mut instances = Vec::new();
    let mut trees = Vec::new();
    let mut owners: Vec<&u64> = placed.keys().collect();
    owners.sort();
    for owner in owners {
        let objs = &placed[owner];
        let on_lot = lot_ids.contains_key(owner);
        for o in objs {
            for t in &o.trees {
                let m = &t.matrix;
                // Row-major 4x4 with the position in row 3; rotation in the upper 3x3.
                let r = [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]];
                trees.push(TreeBaked {
                    position: [m[12], m[13], m[14]],
                    rotation: mat3_to_quat(r),
                    scale: t.scale.max(0.2),
                    kind: o.speedtree.map(|k| k.i).unwrap_or(0),
                });
            }
            if on_lot {
                continue;
            }
            let Some(p) = o.position else { continue };
            let models: Vec<ResourceKey> = if let Some(m) = o.model {
                vec![m]
            } else if let Some(v) = o.vpxy.filter(|v| v.i != 0) {
                pkgs.read(&v).or_else(|| pkgs.read_ti(v.t, v.i)).map(|d| model::vpxy_models(&d)).unwrap_or_default()
            } else {
                Vec::new()
            };
            for m in models {
                instances.push(InstanceBaked { model: key_of(&m), position: p, rotation: o.rotation, lot: None });
            }
        }
    }
    for (i, lot) in world.lots.iter().enumerate() {
        let key = ResourceKey::new(types::MODL, 0x00B0C507, lot.id);
        if pkgs.get_entry(&key).is_some() {
            let (s, c) = (lot.rotation * 0.5).sin_cos();
            instances.push(InstanceBaked { model: key_of(&key), position: lot.corner, rotation: [0.0, s, 0.0, c], lot: Some(i as u32) });
        }
    }

    let mut model_keys: Vec<Key> = instances.iter().map(|i| i.model).collect::<HashSet<_>>().into_iter().collect();
    model_keys.sort();
    let (_, tex) = write_models(&wdir.join("models.pack"), pkgs, &model_keys, &format!("Converting {name}"), progress)?;
    bake_textures(root, pkgs, &tex, OBJECT_TEX_MAX, &format!("Converting {name}"), progress);

    progress(&format!("Converting {name}: roads…"));
    let sectors = (world.heightmap.width.saturating_sub(1) / SECTOR) as u64;
    let half_range = world.heightmap.scale * 32768.0;
    let mut roads = Vec::new();
    let mut road_tex = HashSet::new();
    for e in pkg.of_type(types::MODL).filter(|e| e.key.g == 2) {
        let (sx, sz) = (e.key.i % sectors.max(1), e.key.i / sectors.max(1));
        let origin = [(sx as usize * SECTOR + SECTOR / 2) as f32, half_range, (sz as usize * SECTOR + SECTOR / 2) as f32];
        for m in model::load_model(pkgs, &e.key).unwrap_or_default() {
            let tex = |p: u32| m.material.texture(p).filter(|k| pkgs.get_entry(k).is_some() || pkgs.find_ti(k.t, k.i).is_some()).map(|k| key_of(&k));
            let part = RoadPart {
                positions: m.positions.iter().map(|p| [p[0] + origin[0], p[1] + origin[1], p[2] + origin[2]]).collect(),
                normals: m.normals.clone(),
                uvs1: if m.uvs1.len() == m.uvs.len() { m.uvs1.clone() } else { m.uvs.clone() },
                uvs: m.uvs,
                indices: m.indices,
                base: tex(P_ROAD_BASE),
                overlay: tex(P_ROAD_OVERLAY),
                opacity: tex(P_ROAD_OPACITY),
            };
            road_tex.extend([part.base, part.overlay, part.opacity].into_iter().flatten().map(|k| (k, false)));
            roads.push(part);
        }
    }
    let road_tex: Vec<(Key, bool)> = road_tex.into_iter().collect();
    bake_textures(root, pkgs, &road_tex, OBJECT_TEX_MAX, &format!("Converting {name}"), progress);
    progress(&format!("Converting {name}: lot pictures…"));
    let thumbs: Vec<&s3formats::world::LotInfo> = world.lots.iter().filter(|l| !root.tex_path(lot_thumbnail_key(l.id)).exists()).collect();
    par_map(&thumbs, |l| {
        let key = ResourceKey::new(T_LOT_THUMB, 0, l.id);
        let img = pkg.find(&key).and_then(|e| pkg.read(e).ok()).and_then(|d| decode_png(&d));
        if let Some(img) = img {
            let _ = std::fs::write(root.tex_path(lot_thumbnail_key(l.id)), encode_dds(&img));
        }
    });
    progress(&format!("Converting {name}: houses…"));
    let buildings: Vec<LotBuildingBaked> = world
        .lots
        .iter()
        .enumerate()
        .filter_map(|(i, l)| crate::building::bake_building(&pkg, i, l, placed.get(&l.id).map(|v| v.as_slice()).unwrap_or(&[])))
        .collect();
    let styles: Vec<(Key, bool)> = BUILD_STYLES.iter().map(|k| (*k, false)).collect();
    bake_textures(root, pkgs, &styles, 256, &format!("Converting {name}"), progress);
    let overview = stitch_sectors(&pkg, sectors, 2);
    let lightmap = stitch_sectors(&pkg, sectors, 7);

    let baked = WorldBaked {
        version: WORLD_VERSION,
        name: name.to_string(),
        heightmap: world.heightmap.clone(),
        sea_level,
        layer_dims,
        layer_data,
        layer_avg,
        weights_size: size,
        weights_lz4,
        lots,
        instances,
        trees,
        roads,
        overview,
        lightmap,
        buildings,
    };
    write_value(&wdir.join("world.bin"), &baked).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&wdir, &final_dir).map_err(|e| e.to_string())?;
    Ok(())
}

fn average_linear(img: &s3formats::dds::Rgba) -> [f32; 3] {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let mut sum = [0.0f64; 3];
    let n = (img.data.len() / 4).max(1) as f64;
    for p in img.data.chunks_exact(4) {
        for k in 0..3 {
            sum[k] += lin(p[k]) as f64;
        }
    }
    sum.map(|v| (v / n) as f32)
}

const SECTOR: usize = 256;
/// The game's 256 px picture of each lot (PNG).
const T_LOT_THUMB: u32 = 0xD84E7FC6;

/// Decodes a PNG into RGBA8.
fn decode_png(d: &[u8]) -> Option<s3formats::dds::Rgba> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(d));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let px = &buf[..info.buffer_size()];
    let data: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    Some(s3formats::dds::Rgba { width: w, height: h, data })
}
const P_ROAD_BASE: u32 = 0x53521204;
const P_ROAD_OVERLAY: u32 = 0x28392DC6;
const P_ROAD_OPACITY: u32 = 0x6BDFD546;

/// Stitches the world's per-sector DXT maps of one `kind` (2: terrain colour, 7: lights/shadows)
/// into a single world-sized block-compressed texture without re-encoding.
fn stitch_sectors(pkg: &Package, n: u64, kind: u64) -> Option<WorldMap> {
    if n == 0 {
        return None;
    }
    let mut tiles = Vec::new();
    for sz in 0..n {
        for sx in 0..n {
            let key = ResourceKey::new(types::DDS, 1, (kind << 48) | (sz << 24) | (sx << 8));
            tiles.push(pkg.read(pkg.find(&key)?).ok()?);
        }
    }
    let head = |d: &[u8]| -> Option<(u32, u32, bool)> {
        let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        (d.len() > 128 && &d[0..4] == b"DDS ").then_some(())?;
        let bc3 = match &d[84..88] {
            b"DXT1" => false,
            b"DXT5" => true,
            _ => return None,
        };
        (u(12) == u(16)).then_some((u(16), u(28).max(1), bc3))
    };
    let (s0, _, bc3) = head(&tiles[0])?;
    let mut mips = u32::MAX;
    for t in &tiles {
        let (s, m, b) = head(t)?;
        if s != s0 || b != bc3 {
            return None;
        }
        mips = mips.min(m);
    }
    // Keep levels whose sector tiles are still whole blocks.
    let mips = mips.min((s0 / 4).ilog2() + 1);
    let bb = if bc3 { 16 } else { 8 };
    let n = n as usize;
    let mut data = Vec::new();
    let mut offs = vec![128usize; tiles.len()];
    for level in 0..mips {
        let b = ((s0 >> level) / 4) as usize;
        let row = n * b * bb;
        let start = data.len();
        data.resize(start + row * n * b, 0);
        for (ti, t) in tiles.iter().enumerate() {
            let (sx, sz) = (ti % n, ti / n);
            for by in 0..b {
                let src = offs[ti] + by * b * bb;
                let dst = start + (sz * b + by) * row + sx * b * bb;
                data[dst..dst + b * bb].copy_from_slice(t.get(src..src + b * bb)?);
            }
            offs[ti] += b * b * bb;
        }
    }
    Some(WorldMap { size: s0 * n as u32, mips, bc3, data })
}

fn mat3_to_quat(m: [[f32; 3]; 3]) -> [f32; 4] {
    // m[row][col] with rows as the basis vectors (row-major, v * M convention).
    let (m00, m01, m02) = (m[0][0], m[1][0], m[2][0]);
    let (m10, m11, m12) = (m[0][1], m[1][1], m[2][1]);
    let (m20, m21, m22) = (m[0][2], m[1][2], m[2][2]);
    let tr = m00 + m11 + m22;
    let (x, y, z, w);
    if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        w = 0.25 * s;
        x = (m21 - m12) / s;
        y = (m02 - m20) / s;
        z = (m10 - m01) / s;
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        w = (m21 - m12) / s;
        x = 0.25 * s;
        y = (m01 + m10) / s;
        z = (m02 + m20) / s;
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        w = (m02 - m20) / s;
        x = (m01 + m10) / s;
        y = 0.25 * s;
        z = (m12 + m21) / s;
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        w = (m10 - m01) / s;
        x = (m02 + m20) / s;
        y = (m12 + m21) / s;
        z = 0.25 * s;
    }
    let l = (x * x + y * y + z * z + w * w).sqrt().max(1e-6);
    [x / l, y / l, z / l, w / l]
}

/// Opens a baked model pack (for runtime).
pub fn open_pack(path: &Path) -> Option<PackReader> {
    PackReader::open(path).ok()
}
