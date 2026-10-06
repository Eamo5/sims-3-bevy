//! Command-line baker: `s3bake [--data <install>] [--world <name>]... [--all-worlds]`.

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut data = PathBuf::from(std::env::var("SIMS3_DATA").unwrap_or_else(|_| String::from("S:/Games/Sims 3/The Sims 3")));
    let mut worlds = Vec::new();
    let mut all = false;
    let mut force = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--data" => {
                data = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--world" => {
                worlds.push(args[i + 1].clone());
                i += 1;
            }
            "--all-worlds" => all = true,
            "--force" => force = true,
            _ => {}
        }
        i += 1;
    }
    let root = s3bake::default_root();
    if let Some(i) = args.iter().position(|a| a == "--near") {
        // --near <world> <x> <z> <r>: world instances (and their catalogue objects) near a point.
        let w: s3bake::WorldBaked = s3bake::read_value(&root.world_dir(&args[i + 1]).join("world.bin")).expect("world");
        let (x, z, r): (f32, f32, f32) = (args[i + 2].parse().unwrap(), args[i + 3].parse().unwrap(), args[i + 4].parse().unwrap());
        let cat: Vec<s3bake::types::CatalogEntry> = s3bake::pack::read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        for inst in w.instances.iter().filter(|o| ((o.position[0] - x).powi(2) + (o.position[2] - z).powi(2)).sqrt() < r) {
            let owner = cat.iter().find(|c| c.models.contains(&inst.model)).map_or("?", |c| c.instance_name.as_str());
            println!("{:?} at {:?} lot {:?} ({owner})", inst.model, inst.position, inst.lot);
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--png") {
        // --png <dir> <name>...: the interface pictures so named, written out as PNG files.
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        let dir = std::path::Path::new(&args[i + 1]);
        std::fs::create_dir_all(dir).ok();
        for name in &args[i + 2..] {
            // (`layout:<name>` for an interface layout, 0x2F7D0008.)
            let (t, name, ext) = match name.strip_prefix("layout:") {
                Some(n) => (0x2F7D0008, n, "xml"),
                None => (0x2F7D0004, name.as_str(), "png"),
            };
            match pkgs.read_ti(t, s3pkg::fnv64(name)) {
                Some(d) => {
                    std::fs::write(dir.join(format!("{name}.{ext}")), &d).ok();
                    println!("{name}: {} bytes", d.len());
                }
                None => println!("{name}: not found"),
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--bones") {
        // --bones <rig> <name part>: a rig's bones so named, with their bind positions.
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        let rig = pkgs.read_ti(s3pkg::types::RIG, s3pkg::fnv64(&args[i + 1])).and_then(|d| s3formats::sim::Rig::parse(&d).ok()).expect("rig");
        let want = args.get(i + 2).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        for (k, b) in rig.bones.iter().enumerate() {
            if b.name.to_ascii_lowercase().contains(&want) {
                println!("{k:3} {} parent {:?} pos {:?}", b.name, b.parent, b.position);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "--hair-check") {
        // --hair-check: baked hair parts with no meshes, no layer, or a missing layer texture.
        let cas: s3bake::CasBaked = s3bake::read_value(&root.global_dir().join("cas.bin")).expect("cas.bin");
        let pack = s3bake::PackReader::open(&root.global_dir().join("cas.pack")).expect("cas.pack");
        let (mut n, mut bad) = (0, 0);
        for p in cas.parts.iter().filter(|p| p.baked && p.clothing_type == s3formats::sim::CT_HAIR) {
            n += 1;
            let meshes: Option<s3bake::CasPartMeshes> = pack.get(&p.key);
            let tris: usize = meshes.as_ref().map_or(0, |m| m.meshes.iter().map(|x| x.indices.len() / 3).sum());
            let layer_ok = p.layer.is_some_and(|l| root.tex_path(l).exists());
            if tris == 0 || !layer_ok {
                bad += 1;
                println!("{} ages {:#x}: {} tris, layer {:?} ok {}", p.name, p.age_gender, tris, p.layer, layer_ok);
            }
        }
        println!("{bad} of {n} baked hair parts look broken");
        return;
    }
    if args.iter().any(|a| a == "--geostates") {
        // --geostates: catalogue objects whose models have geometry states, with the states.
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        let cat: Vec<s3bake::types::CatalogEntry> = s3bake::pack::read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        let mut n = 0;
        for c in cat.iter().filter(|c| c.price >= 0) {
            for m in &c.models {
                let meshes = s3formats::model::load_model(&pkgs, &s3bake::types::rkey(*m)).unwrap_or_default();
                let states: Vec<String> = meshes
                    .iter()
                    .filter(|x| !x.states.is_empty())
                    .map(|x| format!("{} tris: {:?}", x.indices.len() / 3, x.states.iter().map(|(h, ix)| format!("{h:08X}/{}", ix.len() / 3)).collect::<Vec<_>>()))
                    .collect();
                if !states.is_empty() {
                    n += 1;
                    if n <= 60 {
                        println!("{} {:?}", c.instance_name, states);
                    }
                }
            }
        }
        println!("{n} models with geometry states");
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--model-name") {
        // --model-name <name>: the models (MODL, VPXY) named so (instance = fnv64 of the name).
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        let cat: Vec<s3bake::types::CatalogEntry> = s3bake::pack::read_value(&root.global_dir().join("catalog.bin")).unwrap_or_default();
        for c in cat.iter().filter(|c| c.instance_name.to_ascii_lowercase().contains(&args[i + 1].to_ascii_lowercase())) {
            println!("catalog {} {:?} models {:?}", c.instance_name, c.objd, c.models);
        }
        // Names from every name map.
        let want = args[i + 1].to_ascii_lowercase();
        for k in pkgs.keys_of_type(0x0166038C) {
            if let Some(d) = pkgs.read(k) {
                for (inst, n) in s3formats::audio::parse_name_map(&d) {
                    if n.to_ascii_lowercase().contains(&want) {
                        let types: Vec<String> = pkgs
                            .keys()
                            .filter(|x| x.i == inst || x.i == inst ^ (1 << 63) || (inst >> 32 == 0 && x.i as u32 == inst as u32))
                            .map(|x| format!("{:08X}:{:X}{}", x.t, x.g, if x.i == inst { "" } else { " (top bit)" }))
                            .collect();
                        println!("name {n} {inst:016X} types {types:?}");
                    }
                }
            }
        }
        // MODEL=<instance hex>: that model's meshes and their geometry states.
        if let Some(m) = std::env::var("MODEL").ok().and_then(|h| u64::from_str_radix(&h, 16).ok()) {
            let fnv32 = |s: &str| s.to_ascii_lowercase().bytes().fold(0x811C9DC5u32, |h, b| h.wrapping_mul(0x01000193) ^ b as u32);
            let names = ["full", "half", "empty", "foodfull", "foodhalf", "foodempty", "default", "burnt", "cookiesfull", "foodfullmesh"];
            if let Some(k) = pkgs.find_ti(s3pkg::types::MODL, m) {
                for mesh in s3formats::model::load_model(&pkgs, &k).unwrap_or_default() {
                    let st: Vec<String> = mesh
                        .states
                        .iter()
                        .map(|(h, ix)| format!("{} ({} tris)", names.iter().find(|n| fnv32(n) == *h).copied().unwrap_or("?"), ix.len() / 3))
                        .collect();
                    println!("mesh {:08X} {} tris, states {st:?}", mesh.name_hash, mesh.indices.len() / 3);
                }
            }
        }
        let inst = s3pkg::fnv64(&args[i + 1]);
        for k in pkgs.keys().filter(|k| k.i == inst || k.i == s3pkg::fnv32(&args[i + 1]) as u64) {
            println!("any type: {k:?}");
        }
        println!("SIMO resources: {}", pkgs.keys_of_type(0x025ED6F4).count());
        for t in [s3pkg::types::MODL, 0x736884F1, 0x01D10F34] {
            for k in pkgs.keys_of_type(t).filter(|k| k.i == inst) {
                let size = pkgs.read(k).map_or(0, |d| d.len());
                println!("{k:?} {size} bytes");
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--strings") {
        // --strings <text>: the English strings containing the text, with their keys.
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        let want = args[i + 1].to_ascii_lowercase();
        for (k, s) in s3formats::stbl::load_english(&pkgs) {
            if s.to_ascii_lowercase().contains(&want) {
                println!("{k:#018x} {s}");
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--fences") {
        // --fences <lot name part>: a lot's fence posts by style, and each style's CFEN references.
        let path = data.join("GameData/Shared/NonPackaged/Worlds/Sunset Valley.world");
        let pkg = s3pkg::Package::open(&path).expect("world");
        let world = s3formats::world::WorldData::load(&pkg).expect("world data");
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        for l in world.lots.iter().filter(|l| l.internal_name.contains(&args[i + 1])) {
            let read = |t: u32| pkg.find(&s3pkg::ResourceKey::new(t, 0, l.id)).and_then(|e| pkg.read(e).ok());
            let refs = read(0x05ED1226).and_then(|d| s3formats::objn::parse_refs(&d).ok()).unwrap_or_default();
            let Some(d) = read(0x913381F2) else { continue };
            let n = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
            let mut styles = std::collections::BTreeMap::new();
            for k in 0..n {
                let o = 8 + k * 14;
                let ri = u16::from_le_bytes(d[o + 12..o + 14].try_into().unwrap());
                *styles.entry(ri).or_insert(0) += 1;
            }
            println!("{}: {n} posts {styles:?}", l.internal_name);
            for ri in styles.keys() {
                let Some(k) = refs.get(ri) else { continue };
                println!("  style {ri}: {k:?}");
                if let Some(c) = pkgs.read(k).or_else(|| pkgs.read_ti(k.t, k.i)) {
                    std::fs::write(format!("cache/cfen_{ri}.bin"), &c).ok();
                    println!("    {} bytes", c.len());
                    // The models it refers to (VPXYs), with their bounds.
                    for o in (0..c.len().saturating_sub(16)).filter(|&o| c[o..o + 4] == 0x736884F1u32.to_le_bytes()) {
                        let g = u32::from_le_bytes(c[o + 4..o + 8].try_into().unwrap());
                        let inst = u64::from_le_bytes(c[o + 8..o + 16].try_into().unwrap());
                        let v = s3pkg::ResourceKey::new(0x736884F1, g, inst);
                        let models = pkgs.read(&v).or_else(|| pkgs.read_ti(v.t, v.i)).map(|d| s3formats::model::vpxy_models(&d)).unwrap_or_default();
                        for m in models.iter().take(1) {
                            let b = s3bake::bake::bake_model(&pkgs, m);
                            let (mn, mx) = b.parts.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(a, c2), p| {
                                ([a[0].min(p.bmin[0]), a[1].min(p.bmin[1]), a[2].min(p.bmin[2])], [c2[0].max(p.bmax[0]), c2[1].max(p.bmax[1]), c2[2].max(p.bmax[2])])
                            });
                            println!("    vpxy @{o} {inst:X} -> {m:?} parts {} bounds {mn:?}..{mx:?}", b.parts.len());
                        }
                    }
                }
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--graphlevels") {
        // --graphlevels <lot name part>: the wall and room graphs' edges per level (Sunset Valley).
        let path = data.join("GameData/Shared/NonPackaged/Worlds/Sunset Valley.world");
        let pkg = s3pkg::Package::open(&path).expect("world");
        let world = s3formats::world::WorldData::load(&pkg).expect("world data");
        for l in world.lots.iter().filter(|l| l.internal_name.contains(&args[i + 1])) {
            let b = s3formats::lot::LotBuildData::load(&pkg, l.id).unwrap_or_default();
            let count = |g: &s3formats::lot::WallGraph| {
                let mut m = std::collections::BTreeMap::new();
                for s in g.segments() {
                    *m.entry(s.2).or_insert(0) += 1;
                }
                m
            };
            println!("{}: walls {:?} rooms {:?}", l.internal_name, count(&b.walls), count(&b.rooms));
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--households") {
        // --households <world>: each premade household with its members' ages (CAS age bits).
        let p = s3bake::load_premades(&root, &args[i + 1]).expect("premades not baked");
        for h in &p.households {
            let ages: Vec<String> = h.members.iter().map(|m| format!("{}:{:#x}", m.first_name.rsplit(':').next().unwrap_or(""), m.age)).collect();
            println!("{:30} §{:<8} {}", h.name, h.funds, ages.join(" "));
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--lots") {
        let w: s3bake::WorldBaked = s3bake::read_value(&root.world_dir(&args[i + 1]).join("world.bin")).expect("world");
        for (k, l) in w.lots.iter().enumerate() {
            if !l.info.is_residential() || std::env::var("LOTOBJS").is_ok_and(|f| l.info.internal_name.contains(&f)) {
                let b = w.buildings.iter().find(|b| b.lot as usize == k);
                let mut kinds: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
                for o in b.map(|b| b.objects.as_slice()).unwrap_or(&[]) {
                    let s = o.script.rsplit('.').next().unwrap_or("").to_string();
                    *kinds.entry(s).or_default() += 1;
                }
                let top: Vec<String> = kinds.iter().filter(|(k, _)| !k.is_empty()).map(|(k, n)| format!("{k}x{n}")).collect();
                // LOTOBJS=<lot name part>: every object on it, with its catalogue name.
                if let Ok(f) = std::env::var("LOTOBJS")
                    && l.info.internal_name.contains(&f)
                {
                    let cat: std::collections::HashMap<s3bake::Key, String> =
                        s3bake::pack::read_value::<Vec<s3bake::types::CatalogEntry>>(&root.global_dir().join("catalog.bin"))
                            .map(|d| d.iter().map(|c| (c.objd, c.instance_name.clone())).collect())
                            .unwrap_or_default();
                    for o in b.map(|b| b.objects.as_slice()).unwrap_or(&[]) {
                        println!("    {:?} {} at {:?} {}", o.objd, cat.get(&o.objd).map_or("?", |s| s.as_str()), o.position, o.script);
                    }
                }
                println!(
                    "{k:3} {:40} {} [{}x{}] walls {} objs {}: {}",
                    l.info.internal_name,
                    l.display_name,
                    l.info.width,
                    l.info.depth,
                    b.map_or(0, |b| b.walls.len()),
                    b.map_or(0, |b| b.objects.len()),
                    top.join(" ")
                );
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--info") {
        let name = &args[i + 1];
        let w: s3bake::WorldBaked = s3bake::read_value(&root.world_dir(name).join("world.bin")).expect("world");
        let weights = lz4_flex::decompress_size_prepended(&w.weights_lz4).unwrap_or_default();
        let px = (w.weights_size * w.weights_size) as usize;
        let mut sums = vec![0u64; 16];
        for g in 0..4 {
            for i in 0..px {
                for c in 0..4 {
                    sums[g * 4 + c] += weights.get(g * px * 4 + i * 4 + c).copied().unwrap_or(0) as u64;
                }
            }
        }
        println!("{name}: layers {:?} data {} bytes, weights size {}, lots {}, instances {}, trees {}, sea {}", w.layer_dims, w.layer_data.len(), w.weights_size, w.lots.len(), w.instances.len(), w.trees.len(), w.sea_level);
        println!("layer coverage %: {:?}", sums.iter().map(|s| (*s as f64 / px as f64 / 2.55).round()).collect::<Vec<_>>());
        let tris: usize = w.roads.iter().map(|r| r.indices.len() / 3).sum();
        println!("roads: {} parts, {tris} triangles, {} without base texture", w.roads.len(), w.roads.iter().filter(|r| r.base.is_none()).count());
        let map = |m: &Option<s3bake::WorldMap>| m.as_ref().map(|m| format!("{}px {} mips {} {} bytes", m.size, m.mips, if m.bc3 { "BC3" } else { "BC1" }, m.data.len()));
        println!("overview: {:?}, lightmap: {:?}", map(&w.overview), map(&w.lightmap));
        let walls: usize = w.buildings.iter().map(|b| b.walls.len()).sum();
        let objs: usize = w.buildings.iter().map(|b| b.objects.len()).sum();
        println!("houses: {} with {walls} wall segments, {objs} objects", w.buildings.len());
        for b in w.buildings.iter().filter(|b| b.objects.iter().any(|o| o.level > 4)) {
            let lifts = b.objects.iter().filter(|o| o.script.to_ascii_lowercase().contains("elevator")).count();
            let top = b.objects.iter().map(|o| o.level).max().unwrap_or(0);
            println!("tall lot {:016X} {}: top level {top}, {lifts} elevator objects", w.lots[b.lot as usize].info.id, w.lots[b.lot as usize].display_name);
        }
        let furnished: Vec<String> = w.buildings.iter().filter(|b| b.is_furnished()).map(|b| w.lots[b.lot as usize].display_name.clone()).collect();
        println!("furnished: {} {:?}", furnished.len(), &furnished[..furnished.len().min(6)]);
        if let Some(b) = w.buildings.iter().find(|b| b.is_house() && !b.is_furnished()) {
            let mut scripts: Vec<&str> = b.objects.iter().map(|o| o.script.rsplit('.').next().unwrap_or("")).collect();
            scripts.sort();
            scripts.dedup();
            println!("unfurnished example {}: {:?}", w.lots[b.lot as usize].display_name, &scripts[..scripts.len().min(30)]);
        }
        let kinds: std::collections::HashSet<u64> = w.trees.iter().map(|t| t.kind).collect();
        println!("trees: {} of {} kinds, {} kinds with billboards", w.trees.len(), kinds.len(), w.tree_kinds.len());
        for k in w.tree_kinds.iter().take(5) {
            println!("  {:016X} h {:.1} r {:.1} views {:?}", k.kind, k.height, k.radius, k.views.iter().map(|v| v.map(|x| (x * 100.0).round() / 100.0)).collect::<Vec<_>>());
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--dump-sound") {
        // --dump-sound <name> <dir>: write a baked sound's samples as files.
        let bank: s3bake::SoundBank = s3bake::read_value(&root.global_dir().join("sounds.bin")).expect("sounds.bin");
        let pack = s3bake::PackReader::open(&root.global_dir().join("sounds.pack")).expect("sounds.pack");
        let dir = std::path::Path::new(&args[i + 2]);
        std::fs::create_dir_all(dir).unwrap();
        let def = bank.get(&args[i + 1]).expect("no such sound");
        println!("{def:?}");
        for s in &def.samples {
            let info = &bank.samples[s];
            let bytes: Vec<u8> = pack.get(&(s3formats::audio::T_SNR, 0, *s)).unwrap();
            let ext = if info.format == s3bake::sounds::FORMAT_MP3 { "mp3" } else { "wav" };
            std::fs::write(dir.join(format!("{s:016X}.{ext}")), bytes).unwrap();
            println!("{s:016X} {info:?}");
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--cas-part") {
        // --cas-part <name>: a baked CAS part's meshes.
        let cas: s3bake::CasBaked = s3bake::read_value(&root.global_dir().join("cas.bin")).expect("cas.bin");
        let pack = s3bake::PackReader::open(&root.global_dir().join("cas.pack")).expect("cas.pack");
        let want = args[i + 1].to_ascii_lowercase();
        for p in cas.parts.iter().filter(|p| p.name.to_ascii_lowercase().contains(&want)).take(12) {
            println!("{} {:?} type {} ages {:x} cat {:x} baked {} layer {:?}", p.name, p.key, p.clothing_type, p.age_gender, p.category, p.baked, p.layer);
            let m: Option<s3bake::CasPartMeshes> = pack.get(&p.key);
            for (k, mesh) in m.map(|m| m.meshes).unwrap_or_default().iter().enumerate() {
                let mags: Vec<String> = mesh
                    .morphs
                    .iter()
                    .map(|d| if d.is_empty() { "-".into() } else { format!("{:.3}", d.iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0f32, f32::max)) })
                    .collect();
                println!("  mesh {k} morphs (heavy/fit/thin max delta) {mags:?}");
                let mut mn = [f32::MAX; 3];
                let mut mx = [f32::MIN; 3];
                for v in &mesh.positions {
                    for a in 0..3 {
                        mn[a] = mn[a].min(v[a]);
                        mx[a] = mx[a].max(v[a]);
                    }
                }
                let (mut u0, mut u1) = ([f32::MAX; 2], [f32::MIN; 2]);
                for uv in &mesh.uvs {
                    for a in 0..2 {
                        u0[a] = u0[a].min(uv[a]);
                        u1[a] = u1[a].max(uv[a]);
                    }
                }
                // Correlation of height with v: negative means higher vertices sample nearer the
                // texture's top.
                let n = mesh.positions.len().max(1) as f32;
                let my = mesh.positions.iter().map(|p| p[1]).sum::<f32>() / n;
                let mv = mesh.uvs.iter().map(|u| u[1]).sum::<f32>() / n;
                let cov: f32 = mesh.positions.iter().zip(&mesh.uvs).map(|(p, u)| (p[1] - my) * (u[1] - mv)).sum::<f32>() / n;
                println!("  mesh {k}: shader {:08X} tex {:?} verts {} tris {} bounds {mn:?}..{mx:?} uv {u0:?}..{u1:?} cov(y,v) {cov:.5}", mesh.shader, mesh.texture, mesh.positions.len(), mesh.indices.len() / 3);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--list-clips") {
        // --list-clips <prefix>: every clip in the install starting with the prefix.
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        let want = args[i + 1].to_ascii_lowercase();
        let mut names: Vec<String> = pkgs
            .keys_of_type(s3pkg::types::CLIP)
            .filter_map(|k| s3formats::sim::clip_name(&pkgs.read(k)?))
            .filter(|n| n.to_ascii_lowercase().starts_with(&want))
            .collect();
        names.sort();
        names.dedup();
        for n in &names {
            println!("{n}");
        }
        eprintln!("{} clips", names.len());
        return;
    }
    if args.iter().any(|a| a == "--cas-stats") {
        // Baked CAS parts per age, gender and clothing type.
        let cas: s3bake::CasBaked = s3bake::read_value(&root.global_dir().join("cas.bin")).expect("cas.bin");
        let ages = [(0x1, "baby"), (0x2, "toddler"), (0x4, "child"), (0x8, "teen"), (0x10, "YA"), (0x20, "adult"), (0x40, "elder")];
        for (ab, an) in [(0x1, "baby"), (0x2, "toddler")] {
            let names: Vec<&str> = cas.parts.iter().filter(|p| p.age_gender & ab != 0 && p.age_gender & 0x7C == 0).map(|p| p.name.as_str()).take(30).collect();
            println!("{an} parts (any bake state): {names:?}");
        }
        let types = [(1, "hair"), (2, "scalp"), (3, "face"), (5, "body"), (6, "top"), (7, "bottom"), (8, "shoes")];
        for (ab, an) in ages {
            for (g, gn) in [(0x1000, "M"), (0x2000, "F")] {
                let counts: Vec<String> = types
                    .iter()
                    .map(|(t, tn)| {
                        let n = cas.parts.iter().filter(|p| p.baked && p.clothing_type == *t && p.age_gender & ab != 0 && p.age_gender & g != 0).count();
                        format!("{tn} {n}")
                    })
                    .collect();
                println!("{an:5} {gn}: {}", counts.join(", "));
            }
        }
        // Formal wear: baked with the everyday wardrobe, or not.
        for baked in [true, false] {
            let f: Vec<&str> = cas.parts.iter().filter(|p| p.category & 0x4 != 0 && p.category & 0x200000 != 0 && matches!(p.clothing_type, 4..=7) && p.baked == baked && p.age_gender & 0xF00 == 0).map(|p| p.name.as_str()).collect();
            println!("formal (baked {baked}): {} e.g. {:?}", f.len(), &f[..f.len().min(12)]);
        }
        // Adult women's hair, with the pack each comes from (group).
        let hair: Vec<String> = cas.parts.iter().filter(|p| p.baked && p.clothing_type == 1 && p.age_gender & 0x20 != 0 && p.age_gender & 0x2000 != 0).map(|p| format!("{} ({:x}, cat {:x})", p.name, p.key.1, p.category)).collect();
        println!("adult F hair: {hair:?}");
        // Facial hair and accessories by clothing type.
        let mut by_type = std::collections::BTreeMap::<u32, Vec<String>>::new();
        for p in cas.parts.iter().filter(|p| ["Beard", "Glasses", "Earring", "Necklace", "Hat", "Bracelet", "Ring", "Mustache", "Goatee", "Sideburn", "Makeup", "Lipstick", "EyeShadow", "Eyeliner", "Blush"].iter().any(|n| p.name.contains(n))) {
            by_type.entry(p.clothing_type).or_default().push(format!("{}{}", p.name, if p.baked { "" } else { "*" }));
        }
        for (t, v) in &by_type {
            println!("type {t}: {} parts, e.g. {:?}", v.len(), &v[..v.len().min(6)]);
        }
        // Service uniforms (the maid's, the repairman's, the mail carrier's).
        let svc: Vec<String> = cas.parts.iter().filter(|p| ["Maid", "Repair", "MailCarrier", "PizzaDelivery"].iter().any(|n| p.name.contains(n))).map(|p| format!("{} (type {}, baked {}, cat {:x}, ag {:x})", p.name, p.clothing_type, p.baked, p.category, p.age_gender)).collect();
        println!("service parts: {svc:#?}");
        let tones: Vec<String> = cas.tone.textures.iter().map(|(ag, t, _)| format!("{ag:x}/{t:x}")).collect();
        println!("skin textures: {tones:?}");
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--trees") {
        // --trees <world>: the world's tree species, their billboard atlas and views, and how
        // many trees of each.
        let root = s3bake::default_root();
        let w: s3bake::WorldBaked = s3bake::read_value(&root.world_dir(&args[i + 1]).join("world.bin")).expect("world");
        for k in &w.tree_kinds {
            let n = w.trees.iter().filter(|t| t.kind == k.kind).count();
            println!("{:016X} {n:5} trees round {} views {} atlas {:?} aspect {:.2} height {:.1} radius {:.1}", k.kind, k.round, k.views.len(), root.tex_path(k.billboard), k.atlas_aspect, k.height, k.radius);
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--catalog") {
        // --catalog <text>: baked catalog entries whose internal or shown name contains the text.
        let want = args.get(i + 1).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        let root = s3bake::default_root();
        let cat: Vec<s3bake::types::CatalogEntry> = s3bake::pack::read_value(&root.global_dir().join("catalog.bin")).expect("no catalog");
        let models = std::env::var("MODELS").is_ok().then(|| s3bake::PackReader::open(&root.global_dir().join("models.pack")).ok()).flatten();
        for c in cat.iter().filter(|c| c.instance_name.to_ascii_lowercase().contains(&want) || c.name.to_ascii_lowercase().contains(&want)) {
            println!("{:08X}:{:08X}:{:016X} {:30} {:30} §{} models {} script {}", c.objd.0, c.objd.1, c.objd.2, c.instance_name, c.name, c.price, c.models.len(), c.script);
            // MODELS=1: each model's parts (vertices, bounds, texture, blend mode).
            if models.is_some() {
                println!("    model keys {:X?}", c.models);
            }
            for m in models.iter().flat_map(|p| c.models.iter().filter_map(|k| p.get::<s3bake::BakedModel>(k))) {
                for p in &m.parts {
                    println!("    part {} verts, bounds {:?}..{:?}, tex {:?}, mode {}", p.positions.len(), p.bmin, p.bmax, p.texture, p.mode);
                }
            }
        }
        return;
    }
    if args.iter().any(|a| a == "--gamedata") {
        // The baked moodlets, traits and skills.
        let g = s3bake::load_gamedata(&s3bake::default_root()).expect("no gameplay data");
        println!("{} buffs, {} traits, {} skills", g.buffs.len(), g.traits.len(), g.skills.len());
        // The build catalogue's fences.
        println!("{} fences", g.fences.len());
        for f in &g.fences {
            println!("fence {:?} §{} straight {} diagonal {} post {}", f.name, f.price, f.straight.is_some(), f.diagonal.is_some(), f.post.is_some());
        }
        // The careers' uniforms.
        let outfits: Vec<s3bake::OutfitInfo> = s3bake::read_value(&s3bake::default_root().global_dir().join("outfits.bin")).unwrap_or_default();
        println!("{} outfits", outfits.len());
        let packs: Vec<s3bake::PackReader> =
            ["cas.pack", "outfits.pack"].iter().filter_map(|n| s3bake::PackReader::open(&s3bake::default_root().global_dir().join(n)).ok()).collect();
        let meshes = |k: &s3bake::Key| packs.iter().find_map(|p| p.get::<s3bake::CasPartMeshes>(k)).map_or(0, |m| m.meshes.len());
        for o in &outfits {
            let parts: Vec<String> = o
                .parts
                .iter()
                .map(|p| {
                    let (q, n) = (&p.part, meshes(&p.part.key));
                    format!("{} ({}, {n} meshes{}{})", q.name, q.clothing_type, if q.layer.is_some() { "" } else { ", no layer" }, if p.hat.is_some() { ", hat" } else { "" })
                })
                .collect();
            println!("outfit {}: {}", o.name, parts.join(", "));
        }
        // FX=<name part>: the effect slots of catalogue objects so named.
        if let Ok(f) = std::env::var("FX") {
            let cat: Vec<s3bake::types::CatalogEntry> = s3bake::pack::read_value(&s3bake::default_root().global_dir().join("catalog.bin")).unwrap_or_default();
            println!("{} objects with effect slots", g.fx_slots.len());
            for c in cat.iter().filter(|c| c.instance_name.to_ascii_lowercase().contains(&f.to_ascii_lowercase())) {
                if let Some((_, fx)) = g.fx_slots.iter().find(|(k, _)| *k == c.objd) {
                    println!("fx {} {:?}", c.instance_name, fx);
                }
            }
        }
        println!("{} plants", g.plants.len());
        for p in &g.plants {
            println!("plant {:?} [{}] {} {} bears {} §{} x{}-{} life {} water -{}/h weeds {} skill {}+{}", p.name, p.rarity, p.model, p.height, p.produce, p.price, p.harvest_min, p.harvest_max, p.lifetime, p.water_decay, p.weeds, p.skill_plant, p.skill_harvest);
            println!("    produce model {:?}", p.produce_model);
        }
        println!("{} opportunities", g.opportunities.len());
        let mut tokens = std::collections::BTreeSet::new();
        for o in &g.opportunities {
            for t in [&o.name, &o.desc, &o.completion, &o.failure, &o.interaction] {
                let mut s = t.as_str();
                while let Some(a) = s.find('{') {
                    let Some(b) = s[a..].find('}') else { break };
                    tokens.insert(s[a..a + b + 1].to_string());
                    s = &s[a + b + 1..];
                }
            }
        }
        println!("opportunity text tokens: {tokens:?}");
        for o in g.opportunities.iter().take(6) {
            println!("opp {} [{}] {:?} at {} {}..{} {}m days {} career {:?} skill {:?} {}-{} money {} perf {} raise {} skill+{} | {:?} | {:?}", o.guid, o.icon, o.name, o.rabbit_hole, o.open, o.close, o.minutes, o.days, o.career, o.skill, o.skill_min, o.skill_max, o.money, o.performance, o.raise, o.skill_reward, o.interaction, o.desc);
        }
        let b = &g.balloons;
        println!("balloons: {} idle, {} social, {} topic, {} random lists", b.idle.len(), b.social.len(), b.topic.len(), b.random.len());
        for k in ["MotiveHunger", "Chat", "Weather"] {
            println!("balloon list {k}: {:?}", b.list(k).map(|l| l.iter().map(|e| if e.icon.is_empty() { &e.refkey } else { &e.icon }).collect::<Vec<_>>()));
        }
        for b in &g.buffs {
            println!("buff {} = {:?} [{}] {} {} {}m: {}", b.hex, b.name, b.icon, b.axis, b.value, b.timeout, b.desc);
        }
        for t in &g.traits {
            println!("trait {} = {:?} [{} / {}] {}: {}", t.hex, t.name, t.icon, t.icon_small, t.category, t.desc);
        }
        for s in &g.skills {
            println!("skill {} = {:?} [{} / {} / {}] max {}: {}", s.hex, s.name, s.icon, s.wish_icon, s.object_icon, s.max_level, s.desc);
        }
        let (walls, floors) = g.patterns.iter().partition::<Vec<_>, _>(|p| !p.floor);
        println!("{} wallpapers, {} floors", walls.len(), floors.len());
        for p in walls.iter().take(5).chain(floors.iter().take(5)) {
            println!("pattern {:?} §{} floor {} tex {:?}", p.name, p.price, p.floor, p.texture);
        }
        for c in &g.collectibles {
            println!("collectible {:?} {} {:?} §{}-{} {} level {} model {}", c.kind, c.key, c.name, c.min_price, c.max_price, c.rarity, c.level, c.model);
        }
        for s in g.spawners.iter().take(12) {
            println!("spawner {} {:?} cap {} hours {:?}", s.class, s.items, s.capacity, s.hours);
        }
        for r in &g.roofs {
            println!("roof {:?} tex {:?} tile {:?}", r.name, r.texture, r.tile);
        }
        println!("writing: {} values; book titles: {:?}", g.writing.len(), g.book_titles.iter().map(|(g, t)| (g.as_str(), t.len(), t.first())).collect::<Vec<_>>());
        for r in &g.recipes {
            println!(
                "recipe {} {:?} lvl {} auto {} meals {:05b} veg {} §{} {:?} group {:?} single {:?} empty {:?}/{:?}",
                r.key, r.name, r.level, r.auto, r.meals, r.vegetarian, r.cost, r.ingredients, r.group, r.single, r.group_empty, r.single_empty
            );
        }
        for w in &g.lifetime_wishes {
            println!("lifetime wish {} {} [{}] {} score {}", w.id, w.check, w.icon, w.number, w.score);
        }
        for c in &g.careers {
            println!("career {} = {:?} [{}] part-time {}: {}", c.hex, c.name, c.icon, c.part_time, c.desc);
            for l in &c.levels {
                println!("   {} {:10} {:30} / {:30} §{}/h {}:00 +{}h days {:07b} skills {:?}", l.level, l.branch, l.title, l.title_female, l.hourly, l.start, l.hours, l.days, l.skills);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "--clip-sounds") {
        // Survey the sound cues of the baked animation clips and whether they resolve.
        let pkgs = s3pkg::install::open_install(&data, |_| true);
        let props: std::collections::HashSet<u64> = pkgs.keys_of_type(s3formats::audio::T_SOUND_PROPS).map(|k| k.i).collect();
        let mut cues = std::collections::BTreeMap::<String, (usize, String)>::new();
        for k in pkgs.keys_of_type(s3pkg::types::CLIP) {
            let Some(d) = pkgs.read(k) else { continue };
            let Some(name) = s3formats::sim::clip_name(&d) else { continue };
            if !s3bake::clips::wanted(&name) {
                continue;
            }
            for s in s3formats::sim::clip_sounds(&d) {
                let e = cues.entry(format!("{:?} {}", s.action, s.name)).or_insert((0, name.clone()));
                e.0 += 1;
            }
        }
        let mut resolved = 0;
        for (cue, (n, clip)) in &cues {
            let name = cue.split(' ').nth(1).unwrap();
            let ok = props.contains(&s3pkg::fnv64(name));
            resolved += ok as usize;
            let suffixes = ["_fa", "_fb", "_ma", "_ca", "_ea", "_ta", "_pa", "_cloth", "_wood", "_cment", "_leath", "_grass_rub", "_wood_rub", "_cpet_rub", "_lino_rub", "_wood_bare", "_wood_heel"];
            let alt: Vec<&str> = if ok { vec![] } else { suffixes.iter().copied().filter(|s| props.contains(&s3pkg::fnv64(&format!("{name}{s}")))).collect() };
            println!("{} {cue:60} x{n:<4} {clip} {alt:?}", if ok { "  " } else { "??" });
        }
        println!("{} cues, {resolved} resolve directly", cues.len());
        return;
    }
    let t0 = Instant::now();
    let progress = |s: &str| println!("[{:7.1}s] {s}", t0.elapsed().as_secs_f32());
    println!("Opening {}", data.display());
    let mut pkgs = s3pkg::install::open_install(&data, |_| true);
    println!("{} resources", pkgs.len());
    if pkgs.len() == 0 {
        eprintln!("no game packages found under {} (is the installation reachable?)", data.display());
        std::process::exit(1);
    }
    if force || root.global_manifest().is_none() {
        match s3bake::bake_global(&root, &pkgs, &data.to_string_lossy(), &progress) {
            Ok(m) => println!("global: {m:?}"),
            Err(e) => {
                eprintln!("global bake failed: {e}");
                std::process::exit(1);
            }
        }
    }
    println!("music tracks: {}", s3bake::bake_music(&root, &data));
    if !s3bake::clips_ready(&root) {
        match s3bake::bake_clips(&root, &pkgs, &progress) {
            Ok(n) => println!("animations: {n}"),
            Err(e) => eprintln!("animations failed: {e}"),
        }
    }
    if force || !s3bake::gamedata_ready(&root) {
        match s3bake::bake_gamedata(&root, &pkgs, std::path::Path::new(&data), &progress) {
            Ok(n) => println!("gameplay data: {n} icons"),
            Err(e) => eprintln!("gameplay data failed: {e}"),
        }
    }
    if force || !s3bake::sounds_ready(&root) {
        match s3bake::bake_sounds(&root, &pkgs, &progress) {
            Ok(n) => println!("sounds: {n}"),
            Err(e) => eprintln!("sounds failed: {e}"),
        }
    }
    let available = s3pkg::install::discover_worlds(&data);
    for path in available {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if !all && !worlds.iter().any(|w| name.to_lowercase().contains(&w.to_lowercase())) {
            continue;
        }
        match s3bake::ensure_premades(&root, &path, &name) {
            Ok(()) => {}
            Err(e) => eprintln!("{name}: premade households failed: {e}"),
        }
        if !force && root.world_ready(&name) {
            println!("{name}: already baked");
            continue;
        }
        if let Ok(p) = s3pkg::Package::open(&path) {
            pkgs.add(p);
        }
        match s3bake::bake_world(&root, &pkgs, &path, &name, &progress) {
            Ok(()) => println!("{name}: baked"),
            Err(e) => eprintln!("{name}: failed: {e}"),
        }
    }
    println!("done in {:.1}s", t0.elapsed().as_secs_f32());
}
