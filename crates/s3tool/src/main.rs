use s3pkg::{Package, types};
use std::collections::BTreeMap;

fn parse_hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: s3tool <types|list|dump|hex|dumpall> <package> [...]");
        return;
    }
    if args[1] == "xmlnames" {
        // xmlnames <package> [filter] [outdir]: the named XML tables (0x0333406C) of a package.
        let pkg = Package::open(&args[2]).expect("open package");
        let mut names: std::collections::HashMap<u64, String> = std::collections::HashMap::new();
        for e in pkg.entries.iter().filter(|e| e.key.t == 0x0166038C) {
            if let Ok(d) = pkg.read(e) {
                names.extend(s3formats::audio::parse_name_map(&d));
            }
        }
        let filter = args.get(3).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        let mut found: Vec<(String, usize)> = pkg
            .entries
            .iter()
            .filter(|e| e.key.t == 0x0333406C)
            .filter_map(|e| names.get(&e.key.i).map(|n| (n.clone(), e.mem_size as usize)))
            .filter(|(n, _)| n.to_ascii_lowercase().contains(&filter))
            .collect();
        found.sort();
        for (n, size) in &found {
            println!("{n} ({size} bytes)");
        }
        if let Some(dir) = args.get(4) {
            std::fs::create_dir_all(dir).ok();
            for e in pkg.entries.iter().filter(|e| e.key.t == 0x0333406C) {
                if let Some(n) = names.get(&e.key.i).filter(|n| n.to_ascii_lowercase().contains(&filter))
                    && let Ok(d) = pkg.read(e)
                {
                    std::fs::write(std::path::Path::new(dir).join(format!("{n}.xml")), d).ok();
                }
            }
        }
        return;
    }
    if args[1] == "roofs" {
        // roofs <root>: the build catalogue's roof patterns (0xF1EDBD86) and their textures.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let mut keys: Vec<_> = set.keys_of_type(0xF1EDBD86).copied().collect();
        keys.sort();
        keys.dedup_by_key(|k| k.i);
        for k in keys {
            let Some(d) = set.read(&k) else { continue };
            let name: String = {
                let n = u32::from_le_bytes([d[0x20], d[0x21], d[0x22], d[0x23]]) as usize;
                let _ = n;
                let raw: Vec<u16> = d[0x21..].chunks(2).take_while(|c| c.len() == 2 && !(c[0] == 0 && c[1] == 0)).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&raw)
            };
            let tgis = s3formats::model::tgi_table_at(&d, 4).unwrap_or_default();
            println!("{:016X} {name} tgis {:?}", k.i, tgis.iter().map(|t| format!("{:08X}:{:08X}:{:016X}", t.t, t.g, t.i)).collect::<Vec<_>>());
            for t in tgis.iter().filter(|t| t.t == types::VPXY) {
                if let Some(vd) = set.read(t).or_else(|| set.read_ti(t.t, t.i))
                    && let Ok(rcol) = s3formats::rcol::Rcol::parse(&vd)
                {
                    let tags: Vec<String> = (0..rcol.chunks.len()).map(|i| rcol.chunk_tag(i).map(|t| String::from_utf8_lossy(&t).to_string()).unwrap_or_default()).collect();
                    let ext: Vec<String> = rcol.external.iter().map(|k| format!("{:08X}:{:08X}:{:016X}", k.t, k.g, k.i)).collect();
                    println!("  vpxy {:016X} chunks {:?} external {:?}", t.i, tags, ext);
                    if let Some(ci) = rcol.find_tag(b"VPXY") && let Some(c) = rcol.chunk_data(ci) {
                        let keys = s3formats::model::tgi_table_at(c, 8).unwrap_or_default();
                        println!("    keys {:?}", keys.iter().map(|k| format!("{:08X}:{:08X}:{:016X}", k.t, k.g, k.i)).collect::<Vec<_>>());
                        for mk in keys.iter().filter(|k| k.t == 0x01D0E75D) {
                            if let Some(m) = s3formats::model::load_matd_resource(&set, mk) {
                                let mut ps: Vec<String> = m.params.iter().map(|(k, v)| match v {
                                    s3formats::model::ParamValue::Texture(t) => format!("{k:08X}=tex {:08X}:{:08X}:{:016X}", t.t, t.g, t.i),
                                    s3formats::model::ParamValue::Float(f) => format!("{k:08X}={f:?}"),
                                    other => format!("{k:08X}={other:?}"),
                                }).collect();
                                ps.sort();
                                println!("    matd shader {:08X} {}", m.shader, ps.join(" "));
                            }
                        }
                    }
                }
                let models = set.read(t).or_else(|| set.read_ti(t.t, t.i)).map(|d| s3formats::model::vpxy_models(&d)).unwrap_or_default();
                for mk in models {
                    for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                        let tex = m.material.texture(s3formats::model::P_DIFFUSE_MAP);
                        println!("    mesh {} verts shader {:08X} diffuse {:?}", m.positions.len(), m.material.shader, tex.map(|t| format!("{:08X}:{:08X}:{:016X}", t.t, t.g, t.i)));
                    }
                }
            }
        }
        return;
    }
    if args[1] == "objects" {
        // objects <root> [max]: decode the models of many OBJDs and validate their bounds.
        let root = std::path::Path::new(&args[2]);
        let max: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(50);
        let set = s3pkg::install::open_install(root, |_| true);
        let mut keys: Vec<_> = set.keys_of_type(types::OBJD).copied().collect();
        keys.sort();
        let (mut ok, mut bad, mut nomodel) = (0, 0, 0);
        for k in keys.iter().take(max) {
            let models = s3formats::object::object_models(&set, k);
            if models.is_empty() {
                nomodel += 1;
                continue;
            }
            for mk in &models {
                match s3formats::model::load_model(&set, mk) {
                    Some(meshes) => {
                        for m in &meshes {
                            let mut mn = [f32::MAX; 3];
                            let mut mx = [f32::MIN; 3];
                            for p in &m.positions {
                                for a in 0..3 {
                                    mn[a] = mn[a].min(p[a]);
                                    mx[a] = mx[a].max(p[a]);
                                }
                            }
                            let err = (0..3)
                                .map(|a| (mn[a] - m.bounds_min[a]).abs().max((mx[a] - m.bounds_max[a]).abs()))
                                .fold(0.0f32, f32::max);
                            let tex = m.material.texture(s3formats::model::P_DIFFUSE_MAP);
                            if err > 0.01 {
                                bad += 1;
                                println!(
                                    "BOUNDS {k} {mk} mesh {:08X} verts={} tris={} err={err:.4} decoded={mn:?}..{mx:?} stored={:?}..{:?}",
                                    m.name_hash,
                                    m.positions.len(),
                                    m.indices.len() / 3,
                                    m.bounds_min,
                                    m.bounds_max
                                );
                            } else {
                                ok += 1;
                            }
                            if args.get(4).is_some() {
                                println!(
                                    "{k} {mk} mesh {:08X} shader={:08X} verts={} tris={} diffuse={:?}",
                                    m.name_hash,
                                    m.material.shader,
                                    m.positions.len(),
                                    m.indices.len() / 3,
                                    tex
                                );
                            }
                        }
                    }
                    None => {
                        println!("FAILED to load {mk} for {k}");
                        bad += 1;
                    }
                }
            }
        }
        println!("ok meshes={ok} bad={bad} objects without model={nomodel}");
        return;
    }
    if args[1] == "winding" {
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let mut keys: Vec<_> = set.keys_of_type(types::OBJD).copied().collect();
        keys.sort();
        let (mut pos, mut neg) = (0usize, 0usize);
        for k in keys.iter().take(400) {
            for mk in s3formats::object::object_models(&set, k) {
                for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                    for t in m.indices.chunks_exact(3) {
                        let p = |i: u32| m.positions[i as usize];
                        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                        let cr = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                        let n = m.normals[t[0] as usize];
                        let d = cr[0] * n[0] + cr[1] * n[1] + cr[2] * n[2];
                        if d > 0.0 { pos += 1 } else if d < 0.0 { neg += 1 }
                    }
                }
            }
        }
        println!("winding: dot>0 {pos}  dot<0 {neg}");
        return;
    }
    if args[1] == "catalog" {
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let strings = s3formats::stbl::load_english(&set);
        eprintln!("{} strings", strings.len());
        let mut keys: Vec<_> = set.keys_of_type(types::OBJD).copied().collect();
        keys.sort();
        for k in keys {
            let Some(d) = set.read(&k) else { continue };
            match s3formats::object::parse_objd(&d) {
                Ok(info) => {
                    let objk = s3formats::object::objd_objk(&set, &d);
                    let script = objk.as_ref().and_then(|o| o.script_class.clone()).unwrap_or_default();
                    let disp = strings.get(&info.name_guid).cloned().unwrap_or_default();
                    println!("{k}	{}	{:.0}	{}	{}	{}	{}", info.show_in_catalog as u8, info.price, info.name, disp, script, info.instance_name);
                }
                Err(_) => println!("{k}	PARSE_ERROR"),
            }
        }
        return;
    }
    if args[1] == "worldmodl" {
        // worldmodl <root> <world>: list the world's MODL resources with decoded bounds.
        let root = std::path::Path::new(&args[2]);
        let mut set = s3pkg::install::open_install(root, |_| true);
        set.add(Package::open(&args[3]).unwrap());
        let w = Package::open(&args[3]).unwrap();
        let mut n = 0;
        for e in w.of_type(types::MODL) {
            let meshes = s3formats::model::load_model(&set, &e.key).unwrap_or_default();
            let mut mn = [f32::MAX; 3];
            let mut mx = [f32::MIN; 3];
            let mut tris = 0;
            let mut shaders = std::collections::BTreeSet::new();
            for m in &meshes {
                tris += m.indices.len() / 3;
                shaders.insert(format!("{:08X}", m.material.shader));
                for p in &m.positions { for a in 0..3 { mn[a] = mn[a].min(p[a]); mx[a] = mx[a].max(p[a]); } }
            }
            if n < 40 || e.key.g == 0x00B0C507 && n < 60 {
                println!("{} meshes={} tris={tris} bounds={:?}..{:?} shaders={:?}", e.key, meshes.len(), mn.map(|v| v.round()), mx.map(|v| v.round()), shaders);
            }
            n += 1;
        }
        return;
    }
    if args[1] == "modlgroup" {
        // modlgroup <root> <world> <group>: every MODL of a group with bounds, shaders and textures.
        let root = std::path::Path::new(&args[2]);
        let mut set = s3pkg::install::open_install(root, |_| true);
        set.add(Package::open(&args[3]).unwrap());
        let w = Package::open(&args[3]).unwrap();
        let g = parse_hex(&args[4]) as u32;
        for e in w.of_type(types::MODL).filter(|e| e.key.g == g) {
            for m in s3formats::model::load_model(&set, &e.key).unwrap_or_default() {
                let mut mn = [f32::MAX; 3];
                let mut mx = [f32::MIN; 3];
                for p in &m.positions { for a in 0..3 { mn[a] = mn[a].min(p[a]); mx[a] = mx[a].max(p[a]); } }
                let tex: Vec<String> = m.material.params.iter().filter_map(|(p, v)| match v {
                    s3formats::model::ParamValue::Texture(k) => Some(format!("{p:08X}={:016X}{}", k.i, if set.read(k).or_else(|| set.read_ti(k.t, k.i)).is_some() { "" } else { "(missing)" })),
                    _ => None,
                }).collect();
                let rng = |u: &[[f32; 2]]| u.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, t| [a[0].min(t[0]), a[1].min(t[1]), a[2].max(t[0]), a[3].max(t[1])]);
                println!("{} shader={:08X} verts={} tris={} uv0={:?} uv1={:?} {:?}..{:?} {:?}", e.key, m.material.shader, m.positions.len(), m.indices.len() / 3, rng(&m.uvs), rng(&m.uvs1), mn.map(|v| v.round()), mx.map(|v| v.round()), tex);
                if args.get(5).is_some() { for (p, v) in &m.material.params { if !matches!(v, s3formats::model::ParamValue::Texture(_)) { println!("    {p:08X} {v:?}"); } } }
            }
        }
        return;
    }
    if args[1] == "objbounds" {
        // objbounds <root> <objd instance hex>: model bounds of an object's meshes.
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let k = s3pkg::ResourceKey::new(types::OBJD, 0, parse_hex(&args[3]));
        for mk in s3formats::object::object_models(&set, &k) {
            for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                println!("{mk} mesh {:08X} verts {} bounds {:?}..{:?}", m.name_hash, m.positions.len(), m.bounds_min, m.bounds_max);
            }
        }
        return;
    }
    if args[1] == "lotbuild" {
        // lotbuild <world> <lot id hex> <out.txt>: walls, roofs and floor cells per level.
        let w = Package::open(&args[2]).unwrap();
        let id = parse_hex(&args[3]);
        let b = s3formats::lot::LotBuildData::load(&w, id).expect("no build data");
        let mut out = String::new();
        for (a, bb, l, e) in b.walls.segments() {
            out += &format!("W {} {} {} {} {l} {} {}
", a[0], a[1], bb[0], bb[1], e.left, e.right);
        }
        for r in &b.roofs {
            out += &format!("R {} {} {} {} {} {} {}
", r.a[0], r.a[1], r.b[0], r.b[1], r.level, r.slope, r.style);
        }
        let levels: std::collections::BTreeSet<u32> = b.rooms.vertices.values().map(|v| v.level).collect();
        for &lv in &levels {
            let edges: Vec<([f32; 2], [f32; 2])> = b.rooms.segments().filter(|s| s.2 == lv || (lv == 1 && s.2 == 0)).map(|s| (s.0, s.1)).collect();
            for c in s3formats::lot::enclosed_floor(&edges, b.rooms.width - 1, b.rooms.depth - 1) {
                out += &format!("F {lv} {} {} {} {}
", c.x, c.z, c.mask, c.region);
            }
        }
        std::fs::write(&args[4], out).unwrap();
        println!("walls {} roofs {} levels {:?}", b.walls.edges.len(), b.roofs.len(), levels);
        return;
    }
    if args[1] == "names" {
        // names <root> <substring>: search every package name map (NMAP) for matching names.
        let root = std::path::Path::new(&args[2]);
        let pat = args[3].to_lowercase();
        for path in s3pkg::install::discover_packages(root) {
            let Ok(p) = Package::open(&path.path) else { continue };
            for e in p.of_type(0x0166038C) {
                let Ok(d) = p.read(e) else { continue };
                if d.len() < 8 {
                    continue;
                }
                let mut r = 8usize;
                let n = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
                for _ in 0..n {
                    if r + 12 > d.len() { break; }
                    let inst = u64::from_le_bytes(d[r..r + 8].try_into().unwrap());
                    let l = u32::from_le_bytes(d[r + 8..r + 12].try_into().unwrap()) as usize;
                    let name = String::from_utf8_lossy(&d[r + 12..(r + 12 + l).min(d.len())]).into_owned();
                    r += 12 + l;
                    if name.to_lowercase().contains(&pat) {
                        let types: Vec<String> = p.entries.iter().filter(|x| x.key.i == inst).map(|x| format!("{:08X}:{:08X}", x.key.t, x.key.g)).collect();
                        println!("{:016X} {name} {:?} [{}]", inst, types, path.path.file_name().unwrap().to_string_lossy());
                    }
                }
            }
        }
        return;
    }
    if args[1] == "roadgraph" {
        // roadgraph <world>: the road graph's counts and first curves.
        let w = Package::open(&args[2]).unwrap();
        let e = w.of_type(s3formats::world::T_ROAD_GRAPH).next().expect("no road graph");
        let g = s3formats::world::RoadGraph::parse(&w.read(e).unwrap()).expect("parse");
        println!("{} road intersections, {} road curves, {} walkways", g.road_intersections.len(), g.road_curves.len(), g.walk_curves.len());
        for c in g.road_curves.iter().take(6) {
            println!("{c:?}");
        }
        for i in g.road_intersections.iter().take(4) {
            println!("int {i:?}");
        }
        return;
    }
    if args[1] == "lots" {
        // lots <world> [x z radius]: lots with id, name, corner and size.
        let w = Package::open(&args[2]).unwrap();
        let near: Option<(f32, f32, f32)> = (args.len() >= 6).then(|| (args[3].parse().unwrap(), args[4].parse().unwrap(), args[5].parse().unwrap()));
        for e in w.of_type(0xD063545B) {
            let Ok(l) = s3formats::world::LotInfo::parse(e.key.i, &w.read(e).unwrap()) else { continue };
            if let Some((x, z, r)) = near {
                let (dx, dz) = (l.corner[0] - x, l.corner[2] - z);
                if (dx * dx + dz * dz).sqrt() > r { continue; }
            }
            println!("{:016X} {} corner {:?} rot {:.2} {}x{}", l.id, l.internal_name, l.corner.map(|v| v.round()), l.rotation, l.width, l.depth);
        }
        return;
    }
    if args[1] == "lotlevels" {
        // lotlevels <root> <world>: per lot with a building, its wall levels and, per floor-grid
        // level, the most used floor design (to see how grid levels map to storeys).
        use s3formats::lotdesign as ld;
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let w = Package::open(&args[3]).unwrap();
        for e in w.of_type(0xD063545B) {
            let Ok(l) = s3formats::world::LotInfo::parse(e.key.i, &w.read(e).unwrap()) else { continue };
            let Some(b) = s3formats::lot::LotBuildData::load(&w, l.id) else { continue };
            if b.walls.edges.is_empty() {
                continue;
            }
            let levels: std::collections::BTreeSet<i64> = b.walls.segments().map(|s| s.2 as i32 as i64).collect();
            let rd = |t: u32, g: u32| w.find(&s3pkg::ResourceKey::new(t, g, l.id)).and_then(|e| w.read(e).ok());
            let refs = s3formats::objn::parse_refs(&rd(0x05ED1226, 0).unwrap_or_default()).unwrap_or_default();
            let designs = ld::parse_designs(&rd(ld::T_DESIGNS, 0).unwrap_or_default());
            let fpal = rd(ld::T_FLOOR_PALETTE, ld::G_FLOOR_PALETTE).and_then(|d| ld::parse_floor_palette(&d).ok()).unwrap_or_default();
            let Some(gd) = rd(ld::T_GRID, ld::G_FLOOR_GRID) else { continue };
            let Ok(g) = ld::Grid::parse(&gd) else { continue };
            let mut per = Vec::new();
            for lv in 0..g.levels {
                let mut count: std::collections::HashMap<u16, usize> = std::collections::HashMap::new();
                for z in 0..g.depth {
                    for x in 0..g.width {
                        for v in g.quad(lv, x, z).unwrap_or([0; 4]) {
                            if v != 0 {
                                *count.entry(v).or_default() += 1;
                            }
                        }
                    }
                }
                let top = count.iter().max_by_key(|(_, n)| **n).map(|(id, n)| {
                    let name = fpal.get(&(*id as u32)).and_then(|(_, comp)| designs.get(comp)).and_then(|d| d.complate.get("daeFileName").map(|v| v.text())).unwrap_or_default();
                    format!("{name}x{n}")
                });
                per.push(format!("L{lv}:{}", top.unwrap_or_default()));
            }
            let _ = &refs;
            let _ = &set;
            println!("{:016X} {:24} walls {:?} | {}", l.id, l.internal_name, levels, per.join(" "));
        }
        return;
    }
    if args[1] == "lotobjs" {
        // lotobjs <world> <lot id hex>: the lot's placed objects with scripts and positions.
        let w = Package::open(&args[2]).unwrap();
        let id = parse_hex(&args[3]);
        let all = s3formats::objn::load_world_objects(&w);
        let lot = w.of_type(0xD063545B).find(|e| e.key.i == id).and_then(|e| s3formats::world::LotInfo::parse(id, &w.read(e).ok()?).ok());
        if let Some(l) = &lot { println!("lot corner {:?} rot {} size {}x{}", l.corner, l.rotation, l.width, l.depth); }
        // (DESIGNS=<install root>: each object's own design, its complate and patterns; with
        // DESIGN_PNG=<dir>, drawn too.)
        let install = std::env::var("DESIGNS").ok().map(|root| {
            let mut set = s3pkg::install::open_install(std::path::Path::new(&root), |_| true);
            set.add(Package::open(&args[2]).unwrap());
            set
        });
        for (n, o) in all.get(&id).map(|v| v.as_slice()).unwrap_or(&[]).iter().enumerate() {
            println!("{:?} pos {:?} rot {:?} script {:?} model {:?}", o.catalog.map(|c| c.to_string()), o.position.map(|p| p.map(|v| (v * 100.0).round() / 100.0)), o.rotation.map(|v| (v * 1000.0).round() / 1000.0), o.script, o.model.map(|m| m.to_string()));
            if let (Some(set), Some((c, keys))) = (&install, &o.design) {
                let blocks: Vec<String> = c.blocks.iter().map(|b| format!("{} ({})", b.name, b.pattern)).collect();
                println!("    design {} xml {:?} keys {} blocks {blocks:?}", c.name, keys.get(c.xml as usize).map(|k| k.to_string()), keys.len());
                // (CANON=1: the design as compared, and whether it names a legacy pattern.)
                if std::env::var("CANON").is_ok() {
                    let canon = c.canonical(keys);
                    println!("    canon old={} {}", canon.contains("=OLD\\"), canon.chars().take(600).collect::<String>());
                }
                if let Ok(dir) = std::env::var("DESIGN_PNG")
                    && let Some(img) = s3formats::complate::render(set, c, keys, 256, 256)
                {
                    let name = o.catalog.and_then(|k| set.read(&k)).and_then(|d| s3formats::object::parse_objd(&d).ok()).map(|x| x.instance_name).unwrap_or_default();
                    let f = std::fs::File::create(std::path::Path::new(&dir).join(format!("{n:03}_{name}.png"))).unwrap();
                    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width as u32, img.height as u32);
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    enc.write_header().unwrap().write_image_data(&img.data).unwrap();
                }
            }
        }
        return;
    }
    if args[1] == "hm" {
        // hm <world> x z [x z ...]: terrain height samples.
        let w = Package::open(&args[2]).unwrap();
        let d = w.read(w.of_type(0x2AD195F2).next().unwrap()).unwrap();
        let hm = s3formats::world::Heightmap::parse(&d).unwrap();
        for c in args[3..].chunks(2) {
            let (x, z): (f32, f32) = (c[0].parse().unwrap(), c[1].parse().unwrap());
            println!("({x}, {z}) -> {}", hm.sample(x, z));
        }
        return;
    }
    if args[1] == "modlverts" {
        // modlverts <root> <world> <modl key> <mesh index> [n]
        let root = std::path::Path::new(&args[2]);
        let mut set = s3pkg::install::open_install(root, |_| true);
        set.add(Package::open(&args[3]).unwrap());
        let parts: Vec<&str> = args[4].split(':').collect();
        let k = s3pkg::ResourceKey::new(parse_hex(parts[0]) as u32, parse_hex(parts[1]) as u32, parse_hex(parts[2]));
        let meshes = s3formats::model::load_model(&set, &k).unwrap_or_default();
        let m = &meshes[args[5].parse::<usize>().unwrap()];
        let n: usize = args.get(6).map(|s| s.parse().unwrap()).unwrap_or(40);
        for i in 0..m.positions.len().min(n) {
            println!("{i:4} pos {:?} uv0 {:?} uv1 {:?} n {:?}", m.positions[i], m.uvs[i], m.uvs1.get(i), m.normals[i]);
        }
        println!("tris: {:?}", &m.indices[..m.indices.len().min(30)]);
        return;
    }
    if args[1] == "slots" {
        // slots <root> <objd instance hex>: an object's routing, container and effect slots.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let k = s3pkg::ResourceKey::new(types::OBJD, 0, parse_hex(&args[3]));
        let objd = set.read(&k).expect("no OBJD");
        let objk = s3formats::object::objd_objk(&set, &objd).expect("no OBJK");
        for k in s3formats::object::objd_keys(&objd).unwrap_or_default() {
            println!("OBJD refers to {k}");
            if k.t == types::OBJK
                && let Some(d) = set.read(&k).or_else(|| set.read_ti(k.t, k.i))
            {
                for t in s3formats::model::tgi_table_at(&d, 4).unwrap_or_default() {
                    println!("  OBJK refers to {t}");
                }
            }
        }
        let mk = objk.model_key.expect("no model");
        let v = set.read(&mk).or_else(|| set.read_ti(mk.t, mk.i)).expect("no VPXY");
        let rk = s3formats::model::vpxy_keys(&v).into_iter().find(|k| k.t == s3formats::model::T_RSLT).expect("no RSLT");
        let s = set.read(&rk).or_else(|| set.read_ti(rk.t, rk.i)).and_then(|d| s3formats::model::parse_rslt(&d)).expect("RSLT");
        for (kind, list) in [("routing", &s.routing), ("container", &s.containers), ("effect", &s.effects)] {
            for x in list {
                println!("{kind:9} name {:08X} bone {:08X} pos {:?} rot {:?}", x.name, x.bone, x.pos, x.rot);
            }
        }
        return;
    }
    if args[1] == "skin" {
        // skin <root> <objd instance hex>: each mesh's joints and blend data, and the rig.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let k = s3pkg::ResourceKey::new(types::OBJD, 0, parse_hex(&args[3]));
        let objd = set.read(&k).expect("no OBJD");
        let objk = s3formats::object::objd_objk(&set, &objd).expect("no OBJK");
        let mk = objk.model_key.expect("no model");
        let v = set.read(&mk).or_else(|| set.read_ti(mk.t, mk.i)).expect("no VPXY");
        let rig = s3formats::model::vpxy_keys(&v)
            .into_iter()
            .find(|k| k.t == 0x8EAF13DE)
            .and_then(|k| set.read(&k).or_else(|| set.read_ti(k.t, k.i)))
            .and_then(|d| s3formats::sim::Rig::parse(&d).ok());
        let name = |h: u32| rig.as_ref().and_then(|r| r.bones.iter().find(|b| s3pkg::fnv32(&b.name) == h).map(|b| b.name.clone())).unwrap_or(format!("{h:08X}"));
        if let Some(r) = &rig {
            for (i, b) in r.bones.iter().enumerate() {
                println!("bone [{i}] {} <- {} pos {:?} rot {:?}", b.name, b.parent, b.position, b.rotation);
            }
        }
        for mk in s3formats::object::object_models(&set, &k) {
            println!("model {mk}");
            for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                println!("  mesh {:08X} verts {} joints {:?}", m.name_hash, m.positions.len(), m.joints.iter().map(|j| name(*j)).collect::<Vec<_>>());
                let mut hist: std::collections::BTreeMap<[u8; 4], usize> = Default::default();
                for b in &m.blend_indices {
                    *hist.entry(*b).or_default() += 1;
                }
                println!("    indices {:?}", hist.iter().take(12).collect::<Vec<_>>());
                for (i, (b, w)) in m.blend_indices.iter().zip(&m.blend_weights).enumerate().step_by((m.blend_indices.len() / 6).max(1)).take(6) {
                    println!("    v{i} {:?} {b:?} {w:?}", m.positions[i]);
                }
            }
        }
        return;
    }
    if args[1] == "objmats" {
        // objmats <root> <objd instance hex>: each mesh's shader and material parameters.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let k = s3pkg::ResourceKey::new(types::OBJD, 0, parse_hex(&args[3]));
        for mk in s3formats::object::object_models(&set, &k) {
            println!("model {mk}");
            for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                println!("  mesh {:08X} shader {:08X} verts {} blended {}", m.name_hash, m.material.shader, m.positions.len(), m.material.is_alpha_blended());
                for (p, v) in &m.material.params {
                    println!("    {p:08X} {v:?}");
                }
            }
        }
        return;
    }
    if args[1] == "grepres" {
        // grepres <root> <text> [type hex]: resources of any (or that) type holding the text, as
        // ASCII or UTF-16; the first few of each type, and counts.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let want = args[3].as_bytes().to_vec();
        let wide: Vec<u8> = args[3].encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        let only = args.get(4).map(|t| u32::from_str_radix(t, 16).unwrap());
        let mut hits = std::collections::BTreeMap::<u32, usize>::new();
        let types: std::collections::BTreeSet<u32> = set.keys().map(|k| k.t).filter(|t| only.is_none_or(|o| o == *t) && *t != types::CLIP).collect();
        for t in types {
            for k in set.keys_of_type(t).copied().collect::<Vec<_>>() {
                let Some(d) = set.read(&k) else { continue };
                let at = d.windows(want.len()).position(|w| w == want.as_slice()).or_else(|| d.windows(wide.len()).position(|w| w == wide.as_slice()));
                if let Some(at) = at {
                    let n = hits.entry(t).or_default();
                    if *n < 4 {
                        println!("{k} @ {at}");
                    }
                    *n += 1;
                }
            }
        }
        println!("{hits:X?}");
        return;
    }
    if args[1] == "xmlfind" {
        // xmlfind <root> <text> [outdir]: XML resources (0x0333406C) containing the text, with
        // their names; with an outdir, writes them there. EXTRA=<package> adds a package.
        let mut set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        if let Ok(x) = std::env::var("EXTRA") {
            set.add(Package::open(&x).unwrap());
        }
        let mut names = std::collections::HashMap::new();
        for k in set.keys_of_type(0x0166038C).copied().collect::<Vec<_>>() {
            if let Some(d) = set.read(&k) {
                for (i, n) in s3formats::audio::parse_name_map(&d) {
                    names.insert(i, n);
                }
            }
        }
        let want = args[3].as_bytes();
        let out = args.get(4).map(std::path::PathBuf::from);
        if let Some(o) = &out {
            std::fs::create_dir_all(o).unwrap();
        }
        for k in set.keys_of_type(0x0333406C).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            if d.windows(want.len()).any(|w| w == want) {
                let nm = names.get(&k.i).cloned().unwrap_or_else(|| format!("{:016X}", k.i));
                println!("{k} {nm} {} bytes", d.len());
                if let Some(o) = &out {
                    std::fs::write(o.join(format!("{nm}.xml")), &d).unwrap();
                }
            }
        }
        return;
    }
    if args[1] == "imgnames" {
        // imgnames <root> [filter] [outdir]: named UI images (PNG 0x2F7D0004) from the install's
        // name maps; with an outdir, writes the matching ones there.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let mut names = std::collections::HashMap::new();
        for k in set.keys_of_type(0x0166038C).copied().collect::<Vec<_>>() {
            if let Some(d) = set.read(&k) {
                for (i, n) in s3formats::audio::parse_name_map(&d) {
                    names.insert(i, n);
                }
            }
        }
        let filter = args.get(3).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        let out = args.get(4).map(std::path::PathBuf::from);
        if let Some(o) = &out {
            std::fs::create_dir_all(o).unwrap();
        }
        let mut n = 0;
        let mut rows: Vec<(String, s3pkg::ResourceKey)> = set
            .keys_of_type(0x2F7D0004)
            .filter_map(|k| Some((names.get(&k.i)?.clone(), *k)))
            .filter(|(nm, _)| nm.to_ascii_lowercase().contains(&filter))
            .collect();
        rows.sort();
        for (nm, k) in rows {
            n += 1;
            if let Some(o) = &out {
                if let Some(d) = set.read(&k) {
                    std::fs::write(o.join(format!("{nm}.png")), d).unwrap();
                }
            } else {
                println!("{nm} {k}");
            }
        }
        eprintln!("{n} images ({} names)", names.len());
        return;
    }
    if args[1] == "impuv" {
        // impuv <root> <world> <modl key> <texture key>: average texture colour under the
        // model's triangles, grouped by how upward-facing and how high they are.
        let root = std::path::Path::new(&args[2]);
        let mut set = s3pkg::install::open_install(root, |_| true);
        set.add(Package::open(&args[3]).unwrap());
        let key = |s: &str| {
            let p: Vec<&str> = s.split(':').collect();
            s3pkg::ResourceKey::new(parse_hex(p[0]) as u32, parse_hex(p[1]) as u32, parse_hex(p[2]))
        };
        let tex = s3formats::dds::decode(&set.read(&key(&args[5])).unwrap(), 1024).unwrap();
        for m in s3formats::model::load_model(&set, &key(&args[4])).unwrap_or_default() {
            println!("uv sets: {} {}; tex {}x{}", m.uvs.len(), m.uvs1.len(), tex.width, tex.height);
            let mut groups = BTreeMap::<String, (usize, [f32; 3], [f32; 4])>::new();
            let mut areas = BTreeMap::<String, f32>::new();
            for t in m.indices.chunks_exact(3) {
                let v = |i: u32| m.positions[i as usize];
                let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                let ny = n[1] / len;
                let lo = a[1].min(b[1]).min(c[1]);
                let class = if lo < 0.4 && ny.abs() > 0.6 { "ground" } else if lo > 2.4 && ny.abs() > 0.25 { "roof" } else { "walls" };
                let uv = |i: u32| m.uvs[i as usize];
                let (ua, ub, uc) = (uv(t[0]), uv(t[1]), uv(t[2]));
                let cu = (ua[0] + ub[0] + uc[0]) / 3.0;
                let cv = (ua[1] + ub[1] + uc[1]) / 3.0;
                // Area-weighted: sample a grid of points over the triangle, weight by 3D area.
                let area = len * 0.5;
                let mut px = [0.0f32; 3];
                let mut k = 0.0;
                for i in 0..=4 {
                    for j in 0..=(4 - i) {
                        let (wa, wb) = (i as f32 / 4.0, j as f32 / 4.0);
                        let wc = 1.0 - wa - wb;
                        let p = tex.sample(ua[0] * wa + ub[0] * wb + uc[0] * wc, ua[1] * wa + ub[1] * wb + uc[1] * wc);
                        for c in 0..3 {
                            px[c] += p[c];
                        }
                        k += 1.0;
                    }
                }
                if std::env::var_os("UVDUMP").is_some() {
                    println!("TRI {class}{} {} {} {} {} {} {} {:.2}", if ny > 0.0 { "+" } else { "-" }, ua[0], ua[1], ub[0], ub[1], uc[0], uc[1], a[1].max(b[1]).max(c[1]));
                }
                let e = groups.entry(format!("{class} ny{}", if ny > 0.0 { "+" } else { "-" })).or_insert((0, [0.0; 3], [f32::MAX, f32::MAX, f32::MIN, f32::MIN]));
                e.0 += 1;
                for c in 0..3 {
                    e.1[c] += px[c] / k * area;
                }
                *areas.entry(format!("{class} ny{}", if ny > 0.0 { "+" } else { "-" })).or_insert(0.0) += area;
                e.2 = [e.2[0].min(cu), e.2[1].min(cv), e.2[2].max(cu), e.2[3].max(cv)];
            }
            for (k, (n, c, r)) in groups {
                let a = areas[&k].max(1e-6);
                println!("  {k}: {n} tris ({a:.0} m2), mean rgb {:.0} {:.0} {:.0}, uv range {:.3},{:.3}..{:.3},{:.3}", c[0] * 255.0 / a, c[1] * 255.0 / a, c[2] * 255.0 / a, r[0], r[1], r[2], r[3]);
            }
        }
        return;
    }
    if args[1] == "matparams" {
        // matparams <root> <world> <modl key>
        let root = std::path::Path::new(&args[2]);
        let mut set = s3pkg::install::open_install(root, |_| true);
        set.add(Package::open(&args[3]).unwrap());
        let parts: Vec<&str> = args[4].split(':').collect();
        let k = s3pkg::ResourceKey::new(parse_hex(parts[0]) as u32, parse_hex(parts[1]) as u32, parse_hex(parts[2]));
        for m in s3formats::model::load_model(&set, &k).unwrap_or_default() {
            println!("mesh {:08X} shader {:08X}", m.name_hash, m.material.shader);
            for (p, v) in &m.material.params { println!("  {p:08X} {v:?}"); }
        }
        return;
    }
    if args[1] == "cas" {
        // cas <root>: summarize CAS parts and rig
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let rig = s3formats::sim::Rig::parse(&set.read_ti(types::RIG, s3pkg::fnv64("auRig")).unwrap()).unwrap();
        println!("rig {} bones={}", rig.name, rig.bones.len());
        let mut keys: Vec<_> = set.keys_of_type(types::CASP).copied().collect();
        keys.sort();
        let (mut ok, mut bad) = (0, 0);
        let filter = args.get(3).cloned().unwrap_or_default().to_lowercase();
        for k in keys {
            let d = set.read(&k).unwrap();
            match s3formats::sim::CasPart::parse(&d) {
                Ok(c) => {
                    ok += 1;
                    if !filter.is_empty() && c.name.to_lowercase().contains(&filter) {
                        let geoms = c.lod0_geoms(&set);
                        println!("{k} {} type={} dt={:X} ag={:08X} cat={:08X} vpxy={} diffuse={:?} geoms={:?} presets={}", c.name, c.clothing_type, c.data_type, c.age_gender, c.category, c.vpxy.len(), c.diffuse, geoms, c.presets.len());
                        // (PRESETS=1: the part's presets too.)
                        if std::env::var("PRESETS").is_ok() {
                            for p in &c.presets {
                                println!("    preset {p}");
                            }
                        }
                        for g in geoms.iter().take(3) {
                            match s3formats::sim::Geom::parse(&set.read(g).or_else(|| set.read_ti(g.t, g.i)).unwrap_or_default()) {
                                Ok(geo) => println!(
                                    "    geom {g}: verts={} tris={} bones={} shader={:08X} params={} diffuse={:?}",
                                    geo.positions.len(),
                                    geo.indices.len() / 3,
                                    geo.bone_hashes.len(),
                                    geo.shader,
                                    geo.params.len(),
                                    geo.params.get(&s3formats::model::P_DIFFUSE_MAP)
                                ),
                                Err(e) => println!("    geom {g}: ERR {e}"),
                            }
                        }
                    }
                }
                Err(_) => bad += 1,
            }
        }
        println!("casp ok={ok} bad={bad}");
        return;
    }
    if args[1] == "tones" {
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        for k in set.keys_of_type(s3formats::sim::T_TONE).copied().collect::<Vec<_>>() {
            let t = s3formats::sim::SkinTone::parse(&set.read(&k).unwrap()).unwrap();
            println!("{k} ramp={:?}", t.ramp);
            for x in &t.textures { if x.age_gender & std::env::var("TONE_AGES").ok().and_then(|v| u32::from_str_radix(&v, 16).ok()).unwrap_or(0x30) != 0 { println!("   ag={:08X} type={} light={:?} dark={:?}", x.age_gender, x.type_flags, x.detail_light, x.detail_dark); } }
        }
        return;
    }
    if args[1] == "clips" {
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let filter = args.get(3).cloned().unwrap_or_default().to_lowercase();
        let mut n = 0;
        for k in set.keys_of_type(types::CLIP).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            if let Some(name) = s3formats::sim::clip_name(&d) {
                n += 1;
                if name.to_lowercase().contains(&filter) {
                    println!("{k} {name}");
                }
            }
        }
        eprintln!("{n} clips");
        return;
    }
    if args[1] == "rigbones" {
        // rigbones <root> <rig name> [filter]: bone names with their parents.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let rig = s3formats::sim::Rig::parse(&set.read_ti(0x8EAF13DE, s3pkg::fnv64(&args[3])).expect("rig")).expect("rig parse");
        let f = args.get(4).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        for (i, b) in rig.bones.iter().enumerate() {
            if b.name.to_ascii_lowercase().contains(&f) {
                let parent = rig.bones.get(b.parent as usize).map_or("-", |p| p.name.as_str());
                println!("[{i}] {} <- {parent} pos {:?} rot {:?}", b.name, b.position, b.rotation);
            }
        }
        return;
    }
    if args[1] == "clipspeed" {
        // clipspeed <root> <rig name> <clip name>...: an in-place locomotion cycle's ground speed,
        // from its feet: how fast each moves backwards while planted (at its lowest).
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let rig = s3formats::sim::Rig::parse(&set.read_ti(0x8EAF13DE, s3pkg::fnv64(&args[3])).expect("rig")).expect("rig parse");
        let mut by_name = std::collections::HashMap::new();
        for k in set.keys_of_type(types::CLIP).copied().collect::<Vec<_>>() {
            if let Some(n) = set.read(&k).and_then(|d| s3formats::sim::clip_name(&d)) {
                by_name.insert(n.to_ascii_lowercase(), k);
            }
        }
        fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
            let [ax, ay, az, aw] = a;
            let [bx, by, bz, bw] = b;
            [aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx, aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz]
        }
        fn qrot(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
            let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]]);
            [p[0], p[1], p[2]]
        }
        fn at<const N: usize>(keys: &[(f32, [f32; N])], t: f32) -> Option<[f32; N]> {
            let i = keys.iter().position(|k| k.0 > t).unwrap_or(keys.len());
            if i == 0 || i == keys.len() {
                return keys.get(i.min(keys.len().saturating_sub(1))).map(|k| k.1);
            }
            let (a, b) = (keys[i - 1], keys[i]);
            let f = ((t - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
            let mut o = [0.0; N];
            for j in 0..N {
                o[j] = a.1[j] + (b.1[j] - a.1[j]) * f;
            }
            Some(o)
        }
        let feet: Vec<usize> = rig.bones.iter().enumerate().filter(|(_, b)| std::env::var("FEET").map_or(b.name == "b__L_Foot__" || b.name == "b__R_Foot__", |f| f.split(',').any(|x| x == b.name))).map(|(i, _)| i).collect();
        for name in &args[4..] {
            let Some(d) = by_name.get(&name.to_ascii_lowercase()).and_then(|k| set.read(k)) else { println!("{name}: missing"); continue };
            let Ok(c) = s3formats::sim::Clip::parse(&d) else { println!("{name}: unparsed"); continue };
            let n = 240;
            // Each foot's world position through the cycle.
            let mut paths = vec![Vec::new(); feet.len()];
            for s in 0..n {
                let t = c.duration * s as f32 / n as f32;
                let mut world: Vec<([f32; 3], [f32; 4])> = Vec::with_capacity(rig.bones.len());
                for b in &rig.bones {
                    let tr = c.tracks.get(&b.hash);
                    let lp = tr.and_then(|x| at(&x.translation, t)).unwrap_or(b.position);
                    let mut lr = tr.and_then(|x| at(&x.rotation, t)).unwrap_or(b.rotation);
                    let l = (lr.iter().map(|x| x * x).sum::<f32>()).sqrt().max(1e-6);
                    lr.iter_mut().for_each(|x| *x /= l);
                    let w = match world.get(b.parent.max(0) as usize).filter(|_| b.parent >= 0) {
                        Some(&(pp, pr)) => {
                            let o = qrot(pr, lp);
                            ([pp[0] + o[0], pp[1] + o[1], pp[2] + o[2]], qmul(pr, lr))
                        }
                        None => (lp, lr),
                    };
                    world.push(w);
                }
                for (j, &f) in feet.iter().enumerate() {
                    paths[j].push(world[f].0);
                }
            }
            let dt = c.duration / n as f32;
            let mut speeds = Vec::new();
            for p in &paths {
                let low = p.iter().map(|x| x[1]).fold(f32::MAX, f32::min);
                // (Planted: within 2 cm of its lowest; its backward speed then, along the walk.)
                for s in 0..n {
                    let (a, b) = (p[s], p[(s + 1) % n]);
                    let tol = std::env::var("TOL").ok().and_then(|v| v.parse().ok()).unwrap_or(0.02f32);
                    if a[1] < low + tol && b[1] < low + tol {
                        speeds.push(((a[2] - b[2]) / dt, (a[0] - b[0]) / dt));
                    }
                }
            }
            speeds.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mid = speeds.get(speeds.len() / 2).copied().unwrap_or_default();
            let mean = speeds.iter().map(|s| s.0).sum::<f32>() / speeds.len().max(1) as f32;
            let range = paths.first().map_or(0.0, |p| p.iter().map(|x| x[2]).fold(f32::MIN, f32::max) - p.iter().map(|x| x[2]).fold(f32::MAX, f32::min));
            println!("{name}: cycle {:.3}s, planted speed z median {:.2} mean {:.2} (x {:.2}) m/s over {} samples; foot range {range:.2} m", c.duration, mid.0, mean, mid.1, speeds.len());
        }
        return;
    }
    if args[1] == "cliproot" {
        // cliproot <root> <rig name> <clip name>...: the first and last root-ish bone keys.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let rig = s3formats::sim::Rig::parse(&set.read_ti(0x8EAF13DE, s3pkg::fnv64(&args[3])).expect("rig")).expect("rig parse");
        let mut by_name = std::collections::HashMap::new();
        for k in set.keys_of_type(types::CLIP).copied().collect::<Vec<_>>() {
            if let Some(n) = set.read(&k).and_then(|d| s3formats::sim::clip_name(&d)) {
                by_name.insert(n.to_ascii_lowercase(), k);
            }
        }
        for name in &args[4..] {
            let Some(d) = by_name.get(&name.to_ascii_lowercase()).and_then(|k| set.read(k)) else { println!("{name}: missing"); continue };
            let Ok(c) = s3formats::sim::Clip::parse(&d) else { println!("{name}: unparsed"); continue };
            println!("{name}: {:.2}s, {} tracks", c.duration, c.tracks.len());
            let only = std::env::var("BONES").ok();
            for (i, b) in rig.bones.iter().enumerate().filter(|(i, b)| match &only { Some(f) => f.split(',').any(|x| b.name.contains(x)), None => *i < 6 }) {
                let Some(t) = c.tracks.get(&b.hash) else { println!("  [{i}] {} -", b.name); continue };
                println!(
                    "  [{i}] {} bind {:?} t0 {:?} t1 {:?} r0 {:?}",
                    b.name,
                    b.position,
                    t.translation.first().map(|x| x.1),
                    t.translation.last().map(|x| x.1),
                    t.rotation.first().map(|x| x.1)
                );
            }
        }
        return;
    }
    if args[1] == "cliptracks" {
        // cliptracks <root> <clip name>...: every track's bone hash (named when it is the fnv32 of
        // a NAMES=a,b,c candidate) with its first and last keys.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let mut by_name = std::collections::HashMap::new();
        for k in set.keys_of_type(types::CLIP).copied().collect::<Vec<_>>() {
            if let Some(n) = set.read(&k).and_then(|d| s3formats::sim::clip_name(&d)) {
                by_name.insert(n.to_ascii_lowercase(), k);
            }
        }
        let names: Vec<String> = std::env::var("NAMES").unwrap_or_default().split(',').map(|s| s.to_string()).collect();
        for name in &args[3..] {
            let Some(d) = by_name.get(&name.to_ascii_lowercase()).and_then(|k| set.read(k)) else { println!("{name}: missing"); continue };
            let Ok(c) = s3formats::sim::Clip::parse(&d) else { println!("{name}: unparsed"); continue };
            println!("{name}: {:.2}s, {} tracks", c.duration, c.tracks.len());
            for (h, t) in &c.tracks {
                let n = names.iter().find(|n| s3pkg::fnv32(n) == *h || s3pkg::fnv32(&n.to_ascii_lowercase()) == *h).cloned().unwrap_or_default();
                println!("  {h:08X} {n:20} t0 {:?} t1 {:?} r0 {:?}", t.translation.first().map(|x| x.1), t.translation.last().map(|x| x.1), t.rotation.first().map(|x| x.1));
            }
        }
        return;
    }
    if args[1] == "clipevents" {
        // clipevents <root> <clip name>...: the event table's events (kind, time, name, payload).
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let mut by_name = std::collections::HashMap::new();
        for k in set.keys_of_type(types::CLIP).copied().collect::<Vec<_>>() {
            if let Some(n) = set.read(&k).and_then(|d| s3formats::sim::clip_name(&d)) {
                by_name.insert(n.to_ascii_lowercase(), k);
            }
        }
        let names: Vec<String> = std::env::var("NAMES").unwrap_or_default().split(',').map(|s| s.to_string()).collect();
        let named = |h: u32| names.iter().find(|n| s3pkg::fnv32(n) == h || s3pkg::fnv32(&n.to_ascii_lowercase()) == h).cloned().unwrap_or_else(|| format!("{h:08X}"));
        for name in &args[3..] {
            let Some(d) = by_name.get(&name.to_ascii_lowercase()).and_then(|k| set.read(k)) else { println!("{name}: missing"); continue };
            println!("{name}:");
            let rel = u32::from_le_bytes(d[0x18..0x1C].try_into().unwrap()) as usize;
            let start = 0x18 + rel;
            if d.get(start..start + 4) != Some(b"=CE=") {
                continue;
            }
            let rd = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
            let count = rd(start + 8) as usize;
            let end = (start + 20 + rd(start + 12) as usize + 64).min(d.len());
            let mut o = start + 20;
            let mut found = 0;
            while o + 28 <= end && found < count {
                let kind = u16::from_le_bytes([d[o], d[o + 1]]);
                if !(1..=40).contains(&kind) || rd(o + 12) != 0xBF80_0000 || rd(o + 16) != 0xBF80_0000 {
                    o += 4;
                    continue;
                }
                found += 1;
                let time = f32::from_bits(rd(o + 8));
                let len = rd(o + 24) as usize;
                let ename = String::from_utf8_lossy(&d[o + 28..o + 28 + len]).into_owned();
                let payload = o + 28 + ((len + 1 + 3) & !3);
                let words: Vec<String> = (0..20).filter(|i| payload + i * 4 + 4 <= d.len()).map(|i| {
                    let w = rd(payload + i * 4);
                    let f = f32::from_bits(w);
                    if f.abs() > 1e-4 && f.abs() < 1e4 { format!("{f:.3}") } else { named(w) }
                }).collect();
                println!("  kind {kind:2} t {time:.2} {ename:?} {}", words.join(" "));
                o = payload;
            }
        }
        return;
    }
    if args[1] == "clipsizes" {
        // clipsizes <root> <names file>: raw vs decoded vs compressed sizes of named clips.
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let names = std::fs::read_to_string(&args[3]).unwrap();
        let (mut raw, mut dec, mut lz, mut n) = (0usize, 0usize, 0usize, 0usize);
        for name in names.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let inst = s3pkg::fnv64(name) & 0x7FFF_FFFF_FFFF_FFFF;
            let Some(d) = set.read_ti(types::CLIP, inst) else { continue };
            let Ok(c) = s3formats::sim::Clip::parse(&d) else { continue };
            let p = postcard::to_stdvec(&c).unwrap();
            raw += d.len();
            dec += p.len();
            lz += lz4_flex::compress_prepend_size(&p).len();
            n += 1;
        }
        println!("{n} clips: raw {} KB, decoded {} KB, lz4 {} KB", raw / 1024, dec / 1024, lz / 1024);
        return;
    }
    if args[1] == "objn" {
        let w = Package::open(&args[2]).unwrap();
        let all = s3formats::objn::load_world_objects(&w);
        let mut total = 0;
        let mut models = std::collections::HashSet::new();
        let mut vpxys = std::collections::HashSet::new();
        let mut trees = 0;
        let mut spt = std::collections::HashSet::new();
        let mut cat_types: BTreeMap<u32, usize> = BTreeMap::new();
        let mut scripts: BTreeMap<String, usize> = BTreeMap::new();
        for (id, objs) in &all {
            total += objs.len();
            let _ = id;
            for o in objs {
                if let Some(m) = o.model { models.insert(m); }
                if let Some(v) = o.vpxy { vpxys.insert(v); }
                if let Some(c) = o.catalog { *cat_types.entry(c.t).or_default() += 1; }
                trees += o.trees.len();
                if let Some(s) = o.speedtree { spt.insert(s); }
                if let Some(s) = &o.script { *scripts.entry(s.rsplit('.').next().unwrap_or("").to_string()).or_default() += 1; }
            }
        }
        println!("{} OBJN resources (of {}), {total} objects, {} distinct MODL, {} distinct VPXY, {trees} tree instances, {} speedtrees", all.len(), w.of_type(s3formats::objn::T_OBJN).count(), models.len(), vpxys.len(), spt.len());
        println!("catalog types: {cat_types:X?}");
        let lots: std::collections::HashSet<u64> = w.of_type(0xD063545B).map(|e| e.key.i).collect();
        let mut layer_scripts: BTreeMap<String, usize> = BTreeMap::new();
        let mut layer_objs = 0;
        for (id, objs) in &all {
            if lots.contains(id) { continue; }
            layer_objs += objs.len();
            for o in objs {
                let k = o.script.clone().map(|s| s.rsplit('.').next().unwrap_or("").to_string()).unwrap_or_else(|| format!("noscript cat={:?} model={}", o.catalog.map(|c| format!("{:08X}", c.t)), o.model.is_some()));
                *layer_scripts.entry(k).or_default() += 1;
            }
        }
        let mut ls: Vec<_> = layer_scripts.into_iter().collect();
        ls.sort_by_key(|x| std::cmp::Reverse(x.1));
        println!("layer objects {layer_objs}: {:?}", &ls[..ls.len().min(40)]);
        for (id, objs) in &all { if !lots.contains(id) { for o in objs { if o.catalog.is_some_and(|c| c.t == 0x2DA18F83) { println!("2DA18F83 example {} model={:?} vpxy={:?}", o.catalog.unwrap(), o.model, o.vpxy); break; } } } }
        let mut sc: Vec<_> = scripts.into_iter().collect();
        sc.sort_by_key(|x| std::cmp::Reverse(x.1));
        println!("top scripts: {:?}", &sc[..sc.len().min(40)]);
        return;
    }
    if args[1] == "composite" {
        // composite <root> <objd hex> <outdir>: write each mesh's composited diffuse as raw RGBA.
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let k = s3pkg::ResourceKey::new(types::OBJD, 0, parse_hex(&args[3]));
        std::fs::create_dir_all(&args[4]).unwrap();
        for mk in s3formats::object::object_models(&set, &k) {
            for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                let Some(tk) = m.material.texture(s3formats::model::P_DIFFUSE_MAP) else { continue };
                let td = set.read(&tk).or_else(|| set.read_ti(tk.t, tk.i)).unwrap();
                let t0 = std::time::Instant::now();
                let img = if tk.t == types::TXTC {
                    s3formats::compositor::composite(&set, &td, 512)
                } else {
                    s3formats::dds::decode(&td, 512)
                };
                if let Some(img) = img {
                    let path = format!("{}/{:08X}_{}x{}.rgba", args[4], m.name_hash, img.width, img.height);
                    std::fs::write(&path, &img.data).unwrap();
                    println!("{path} in {:?}", t0.elapsed());
                }
            }
        }
        return;
    }
    if args[1] == "simo" {
        // simo <root> <outfit name> <outdir>: an outfit's parts, with each part's design
        // rendered from its preset (and the part's own default texture alongside).
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let d = set.read_ti(s3formats::sim::T_OUTFIT, s3pkg::fnv64(&args[3])).expect("no such outfit");
        let o = s3formats::sim::SimOutfit::parse(&d).unwrap();
        println!("age {:x} gender {:x}", o.age, o.gender);
        let out = std::path::Path::new(&args[4]);
        std::fs::create_dir_all(out).ok();
        let png = |img: &s3formats::dds::Rgba, name: &str| {
            let f = std::fs::File::create(out.join(name)).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width as u32, img.height as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&img.data).unwrap();
        };
        for (i, p) in o.parts.iter().enumerate() {
            let c = set.read(&p.casp).or_else(|| set.read_ti(p.casp.t, p.casp.i)).and_then(|d| s3formats::sim::CasPart::parse(&d).ok());
            println!("part {i}: {:?} body type {} {:?}", p.casp, p.body_type, c.as_ref().map(|c| (&c.name, c.clothing_type, c.presets.len())));
            if let Some(img) = s3formats::complate::render_preset(&set, &p.preset, 1024, true) {
                png(&img, &format!("{i}_outfit.png"));
            }
            if let Some(c) = &c {
                if let Some(t) = c.diffuse.first().and_then(|k| set.read(k)).and_then(|d| s3formats::txtc::Txtc::parse(&d).ok()) {
                    let mut comp = s3formats::compositor::Compositor::new(&set);
                    comp.max_size = 1024;
                    let (w, h) = comp.output_size(&t);
                    comp.layer_mode = true;
                    png(&comp.run(&t, w, h), &format!("{i}_txtc.png"));
                }
                if let Some(img) = c.presets.first().and_then(|x| s3formats::complate::render_preset(&set, x, 1024, true)) {
                    png(&img, &format!("{i}_preset0.png"));
                }
            }
        }
        return;
    }
    if args[1] == "raw" {
        // raw <root> <type:group:instance> <out>: one resource's (decompressed) bytes.
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let parts: Vec<&str> = args[3].split(':').collect();
        let k = s3pkg::ResourceKey::new(parse_hex(parts[0]) as u32, parse_hex(parts[1]) as u32, parse_hex(parts[2]));
        let d = set.read(&k).or_else(|| set.read_ti(k.t, k.i)).expect("no such resource");
        std::fs::write(&args[4], d).unwrap();
        return;
    }
    if args[1] == "txtcraw" {
        // txtcraw <root> <txtc key> <out.rgba>: composite a TXTC to raw RGBA (w, h as u32 header).
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let parts: Vec<&str> = args[3].split(':').collect();
        let tk = s3pkg::ResourceKey::new(parse_hex(parts[0]) as u32, parse_hex(parts[1]) as u32, parse_hex(parts[2]));
        let t = s3formats::txtc::Txtc::parse(&set.read(&tk).unwrap()).unwrap();
        let mut c = s3formats::compositor::Compositor::new(&set);
        let (w, h) = c.output_size(&t);
        let img = c.run(&t, w, h);
        let mut out = Vec::new();
        out.extend_from_slice(&(img.width as u32).to_le_bytes());
        out.extend_from_slice(&(img.height as u32).to_le_bytes());
        out.extend_from_slice(&img.data);
        std::fs::write(&args[4], out).unwrap();
        return;
    }
    if args[1] == "txtckey" {
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let parts: Vec<&str> = args[3].split(':').collect();
        let tk = s3pkg::ResourceKey::new(parse_hex(parts[0]) as u32, parse_hex(parts[1]) as u32, parse_hex(parts[2]));
        let td = set.read(&tk).unwrap();
        let t = s3formats::txtc::Txtc::parse(&td).unwrap();
        fn dump(t: &s3formats::txtc::Txtc, ind: &str) {
            println!("{ind}version {} keys:", t.version);
            for (i, k) in t.keys.iter().enumerate() { println!("{ind}  [{i}] {k}"); }
            for (idx, f) in &t.fabrics { println!("{ind}fabric tgi[{idx}]:"); dump(f, &format!("{ind}    ")); }
            for s in &t.steps {
                let props: Vec<String> = s.props.iter().filter(|(p, _)| ![0xD92A4C8B, 0x2EDF5F53, 0x06A775CE, 0xAE5FE82A, 0x331178DF, 0x6B7119C1].contains(p)).map(|(p, v)| format!("{}={:?}", s3formats::txtc::Txtc::prop_name(*p), v)).collect();
                println!("{ind}  {} {}", s3formats::txtc::Txtc::step_name(s.kind()), props.join(" "));
            }
        }
        dump(&t, "");
        return;
    }
    if args[1] == "txtc" {
        // txtc <root> <objd instance hex>: dump the diffuse TXTC of an object's meshes.
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let inst = parse_hex(&args[3]);
        let k = s3pkg::ResourceKey::new(types::OBJD, 0, inst);
        let d = set.read(&k).unwrap();
        // material presets
        for mk in s3formats::object::object_models(&set, &k) {
            for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                let Some(tk) = m.material.texture(s3formats::model::P_DIFFUSE_MAP) else { continue };
                println!("mesh {:08X} diffuse {tk}", m.name_hash);
                if tk.t != types::TXTC { continue; }
                let td = set.read(&tk).or_else(|| set.read_ti(tk.t, tk.i)).unwrap();
                let t = s3formats::txtc::Txtc::parse(&td).unwrap();
                fn dump(t: &s3formats::txtc::Txtc, ind: &str) {
                    println!("{ind}version {} keys:", t.version);
                    for (i, k) in t.keys.iter().enumerate() { println!("{ind}  [{i}] {k}"); }
                    for (idx, f) in &t.fabrics { println!("{ind}fabric tgi[{idx}]:"); dump(f, &format!("{ind}    ")); }
                    for s in &t.steps {
                        let props: Vec<String> = s.props.iter().map(|(p, v)| format!("{}={:?}", s3formats::txtc::Txtc::prop_name(*p), v)).collect();
                        println!("{ind}  {} {}", s3formats::txtc::Txtc::step_name(s.kind()), props.join(" "));
                    }
                }
                dump(&t, "  ");
            }
        }
        let _ = d;
        return;
    }
    if args[1] == "lotdesignstats" {
        // lotdesignstats <root> <world>: how many of the lots' objects have designs of their own,
        // how many different ones, and how many are one of the catalogue's.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let w = Package::open(&args[3]).unwrap();
        let all = s3formats::objn::load_world_objects(&w);
        let mut stock: std::collections::HashMap<String, (u64, usize)> = std::collections::HashMap::new();
        let mut presets_of: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        let (mut with, mut total, mut matched) = (0, 0, 0);
        let mut unique: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for objs in all.values() {
            for o in objs {
                total += 1;
                let (Some((c, keys)), Some(cat)) = (&o.design, o.catalog) else { continue };
                with += 1;
                if let std::collections::hash_map::Entry::Vacant(e) = presets_of.entry(cat.i) {
                    let objd = set.read(&cat).and_then(|d| s3formats::object::parse_objd(&d).ok());
                    for (i, p) in objd.iter().flat_map(|o| o.presets.iter()).enumerate() {
                        stock.insert(p.complate.canonical(&p.keys), (cat.i, i));
                    }
                    e.insert(objd.map_or(0, |o| o.presets.len()));
                }
                let canon = c.canonical(keys);
                if canon.contains("=OLD\\") {
                    *unique.entry("OLD".into()).or_default() += 1;
                    continue;
                }
                // (DIFF=<n>: the first n unmatched next to the object's first design.)
                if let Some(n) = std::env::var("DIFF").ok().and_then(|v| v.parse::<usize>().ok())
                    && unique.len() < n
                    && !stock.contains_key(&canon)
                {
                    let objd = set.read(&cat).and_then(|d| s3formats::object::parse_objd(&d).ok());
                    println!("LOT   {canon}");
                    if let Some(p) = objd.as_ref().and_then(|o| o.presets.first()) {
                        println!("STOCK {}", p.complate.canonical(&p.keys));
                    }
                }
                if stock.get(&canon).is_some_and(|(i, _)| *i == cat.i) {
                    matched += 1;
                } else {
                    *unique.entry(canon).or_default() += 1;
                }
            }
        }
        println!("{total} objects, {with} with a design, {matched} of them a catalogue design, {} other designs ({} uses)", unique.len(), unique.values().sum::<usize>());
        return;
    }
    if args[1] == "presetstats" {
        // presetstats <root>: how many catalogue objects have designs, and how many designs.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let (mut objs, mut multi, mut total, mut catalog_total) = (0, 0, 0, 0);
        let mut by_complate = BTreeMap::<String, usize>::new();
        for k in set.keys_of_type(types::OBJD).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            let Ok(o) = s3formats::object::parse_objd(&d) else { continue };
            objs += 1;
            total += o.presets.len();
            if o.presets.len() > 1 {
                multi += 1;
                if o.show_in_catalog {
                    catalog_total += o.presets.len();
                }
            }
            for p in &o.presets {
                *by_complate.entry(p.complate.name.clone()).or_default() += 1;
            }
        }
        println!("{objs} objects, {multi} with designs to choose from, {total} designs ({catalog_total} of catalogue objects with a choice)");
        for (n, c) in by_complate.iter().filter(|(_, c)| **c > 50) {
            println!("  {c:6} {n}");
        }
        return;
    }
    if args[1] == "objpresets" {
        // objpresets <root> <instance name filter> <out dir>: a catalogue object's designs
        // (OBJD presets), each rendered from its complate, and its meshes' diffuse textures.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let want = args[3].to_ascii_lowercase();
        let dir = std::path::Path::new(&args[4]);
        let _ = std::fs::create_dir_all(dir);
        let save = |img: &s3formats::dds::Rgba, name: &str| {
            let f = std::fs::File::create(dir.join(name)).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width as u32, img.height as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&img.data).unwrap();
        };
        let mut shown = 0;
        for k in set.keys_of_type(types::OBJD).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            let Ok(o) = s3formats::object::parse_objd(&d) else { continue };
            if !o.instance_name.to_ascii_lowercase().contains(&want) || shown >= 3 {
                continue;
            }
            shown += 1;
            println!("{k} {} ({}) presets {}", o.instance_name, o.name, o.presets.len());
            for (i, p) in o.presets.iter().enumerate() {
                let xml = p.keys.get(p.complate.xml as usize).map(|k| k.to_string()).unwrap_or_default();
                let ov: Vec<String> = p.complate.overrides.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                println!("  preset {i}: complate {} xml {xml} keys {} overrides {}", p.complate.name, p.keys.len(), ov.join(" "));
                for b in &p.complate.blocks {
                    let ov: Vec<String> = b.overrides.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                    println!("     block {} ({}) {}", b.name, b.pattern, ov.join(" "));
                }
                if let Some(img) = s3formats::complate::render(&set, &p.complate, &p.keys, 256, 256) {
                    save(&img, &format!("{}_{i}.png", o.instance_name));
                }
            }
            for mk in s3formats::object::object_models(&set, &k) {
                for (j, m) in s3formats::model::load_model(&set, &mk).unwrap_or_default().iter().enumerate() {
                    let t = m.material.texture(s3formats::model::P_DIFFUSE_MAP);
                    println!("  mesh {j}: shader {:08X} diffuse {:?}", m.material.shader, t.map(|k| k.to_string()));
                    if let Some(t) = t
                        && let Some(td) = set.read(&t).or_else(|| set.read_ti(t.t, t.i))
                    {
                        let img = if t.t == types::TXTC { s3formats::compositor::composite(&set, &td, 256) } else { s3formats::dds::decode(&td, 256) };
                        if let Some(img) = img {
                            save(&img, &format!("{}_mesh{j}.png", o.instance_name));
                        }
                    }
                }
            }
        }
        return;
    }
    if args[1] == "texpng" {
        // texpng <root> <type:group:instance> <out.png>: a DDS texture or TXTC composite as PNG
        // (WORLD=<world file> also searches that world).
        let mut set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        if let Ok(w) = std::env::var("WORLD") {
            set.add(Package::open(&w).unwrap());
        }
        let parts: Vec<&str> = args[3].split(':').collect();
        let k = s3pkg::ResourceKey::new(parse_hex(parts[0]) as u32, parse_hex(parts[1]) as u32, parse_hex(parts[2]));
        let d = set.read(&k).or_else(|| set.read_ti(k.t, k.i)).expect("not found");
        let img = if k.t == types::TXTC { s3formats::compositor::composite(&set, &d, 1024) } else { s3formats::dds::decode(&d, 1024) }.expect("decode");
        let f = std::fs::File::create(&args[4]).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width as u32, img.height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img.data).unwrap();
        println!("{}x{}", img.width, img.height);
        return;
    }
    if args[1] == "morphs" {
        // morphs <root> <casp name>: the part's fat/fit/thin/special blends and their morph meshes.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let want = args[3].to_ascii_lowercase();
        for k in set.keys_of_type(types::CASP).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            let Ok(c) = s3formats::sim::CasPart::parse(&d) else { continue };
            if c.name.to_ascii_lowercase() != want {
                continue;
            }
            println!("{} ag {:08X}", c.name, c.age_gender);
            for (slot, bk) in ["fat", "fit", "thin", "special"].iter().zip(c.blends.iter()) {
                let Some(bk) = bk else { println!("  {slot}: -"); continue };
                let Some(bd) = set.read(bk).or_else(|| set.read_ti(bk.t, bk.i)) else { println!("  {slot}: {bk} missing"); continue };
                println!("  {slot}: {bk} {} bytes: {:02x?}", bd.len(), &bd[..bd.len().min(48)]);
                match s3formats::sim::BlendInfo::parse(&bd) {
                    Ok(b) => {
                        println!("    part {:?}, {} entries, {} keys, bgeo {:?}", b.name, b.entries.len(), b.keys.len(), b.bgeo);
                        if let Some(gk) = b.bgeo {
                            match set.read(&gk).or_else(|| set.read_ti(gk.t, gk.i)).map(|d| s3formats::sim::parse_bgeo(&d)) {
                                Some(Ok(bl)) => {
                                    for x in &bl {
                                        println!("    bgeo blend ag {:08X} region {:#x}: {} vertex deltas", x.age_gender, x.region, x.deltas.len());
                                    }
                                }
                                Some(Err(e)) => println!("    bgeo unparsed: {e}"),
                                None => println!("    bgeo missing"),
                            }
                        }
                        for e in &b.entries {
                            println!("    region {:#x}: {} geoms, {} bones", e.region, e.geoms.len(), e.bones.len());
                            for g in &e.geoms {
                                let tk = b.keys.get(g.2 as usize);
                                print!("      ag {:08X} amount {} -> {:?}", g.0, g.1, tk);
                                if let Some(vk) = tk
                                    && let Some(vd) = set.read(vk).or_else(|| set.read_ti(vk.t, vk.i))
                                {
                                    let geoms = s3formats::sim::vpxy_lod_geoms(&vd, 0);
                                    print!(" lod0 geoms {}", geoms.len());
                                    for gk in geoms.iter().take(2) {
                                        if let Some(gd) = set.read(gk).or_else(|| set.read_ti(gk.t, gk.i))
                                            && let Ok(m) = s3formats::sim::Geom::parse(&gd)
                                        {
                                            print!(" [{gk}: {} verts, ids {:?}.. pos {:?}]", m.positions.len(), &m.ids[..m.ids.len().min(3)], &m.positions[..m.positions.len().min(2)]);
                                        }
                                    }
                                }
                                println!();
                            }
                        }
                    }
                    Err(e) => println!("    unparsed: {e}"),
                }
            }
            for g in c.lod0_geoms(&set) {
                if let Some(gd) = set.read(&g).or_else(|| set.read_ti(g.t, g.i))
                    && let Ok(m) = s3formats::sim::Geom::parse(&gd)
                {
                    println!("  base {g}: {} verts ids {:?}..", m.positions.len(), &m.ids[..m.ids.len().min(3)]);
                }
            }
        }
        return;
    }
    if args[1] == "partlods" {
        // partlods <root> <casp name>...: every VPXY entry of the parts, per LOD, with GEOM sizes.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let want: Vec<String> = args[3..].iter().map(|s| s.to_ascii_lowercase()).collect();
        for k in set.keys_of_type(types::CASP).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            let Ok(c) = s3formats::sim::CasPart::parse(&d) else { continue };
            if !want.contains(&c.name.to_ascii_lowercase()) {
                continue;
            }
            println!("{} {k}: {} vpxy, keys {:?}", c.name, c.vpxy.len(), c.keys);
            for vk in &c.vpxy {
                let Some(vd) = set.read(vk).or_else(|| set.read_ti(vk.t, vk.i)) else { continue };
                if let Ok(rcol) = s3formats::rcol::Rcol::parse(&vd)
                    && let Some(ci) = rcol.find_tag(b"VPXY")
                    && let Some(c) = rcol.chunk_data(ci)
                {
                    println!("  raw VPXY: {:02x?}", &c[..c.len().min(96)]);
                    for (i, k) in s3formats::model::tgi_table_at(c, 8).unwrap_or_default().iter().enumerate() {
                        let n = set.read(k).or_else(|| set.read_ti(k.t, k.i)).and_then(|d| s3formats::sim::Geom::parse(&d).ok()).map(|g| g.positions.len());
                        println!("    [{i}] {k} geom verts {n:?}");
                    }
                }
                for lod in 0..4u8 {
                    for g in s3formats::sim::vpxy_lod_geoms(&vd, lod) {
                        let Some(gd) = set.read(&g).or_else(|| set.read_ti(g.t, g.i)) else { continue };
                        let Ok(geom) = s3formats::sim::Geom::parse(&gd) else { println!("  lod {lod} {g}: unparsed"); continue };
                        println!("  vpxy {vk} lod {lod} geom {g}: {} verts {} bones", geom.positions.len(), geom.bone_hashes.len());
                    }
                }
            }
        }
        return;
    }
    if args[1] == "partrig" {
        // partrig <root> <casp name>...: how many of each part's GEOM bones each rig has.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let rigs: Vec<(String, s3formats::sim::Rig)> = ["auRig", "cuRig", "puRig"]
            .iter()
            .filter_map(|n| Some((n.to_string(), s3formats::sim::Rig::parse(&set.read_ti(0x8EAF13DE, s3pkg::fnv64(n))?).ok()?)))
            .collect();
        for (n, r) in &rigs {
            println!("{n}: {} bones", r.bones.len());
        }
        let want: Vec<String> = args[3..].iter().map(|s| s.to_ascii_lowercase()).collect();
        for k in set.keys_of_type(types::CASP).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            let Ok(c) = s3formats::sim::CasPart::parse(&d) else { continue };
            if !want.contains(&c.name.to_ascii_lowercase()) {
                continue;
            }
            for g in c.lod0_geoms(&set) {
                let Some(gd) = set.read(&g).or_else(|| set.read_ti(g.t, g.i)) else { continue };
                let Ok(geom) = s3formats::sim::Geom::parse(&gd) else { continue };
                let found: Vec<String> = rigs.iter().map(|(n, r)| format!("{n} {}/{}", geom.bone_hashes.iter().filter(|h| r.index_of(**h).is_some()).count(), geom.bone_hashes.len())).collect();
                println!("{} geom {g}: {} verts, bones found {found:?}", c.name, geom.positions.len());
            }
        }
        return;
    }
    if args[1] == "refs" {
        // refs <world file> <type:group:instance>: a REFS table's entries.
        let w = Package::open(&args[2]).unwrap();
        let parts: Vec<&str> = args[3].split(':').collect();
        let key = s3pkg::ResourceKey::new(parse_hex(parts[0]) as u32, parse_hex(parts[1]) as u32, parse_hex(parts[2]));
        let e = w.find(&key).expect("not found");
        let d = w.read(e).unwrap();
        let refs = s3formats::objn::parse_refs(&d).unwrap_or_default();
        let mut v: Vec<_> = refs.into_iter().collect();
        v.sort_by_key(|x| x.0);
        let mut by_type = BTreeMap::<u32, usize>::new();
        for (i, k) in &v {
            *by_type.entry(k.t).or_default() += 1;
            let show = std::env::var("REFS_RANGE").ok().and_then(|r| {
                let (a, b) = r.split_once('-')?;
                Some((a.parse::<u16>().ok()?, b.parse::<u16>().ok()?))
            });
            if show.is_some_and(|(a, b)| (a..=b).contains(i)) || (show.is_none() && (k.t == 0x515CA4CD || k.t == 0x9151E6BC)) {
                println!("{i:5} {k}");
            }
        }
        println!("{} refs by type {by_type:08X?}", v.len());
        return;
    }
    if args[1] == "cwal" {
        // cwal <root> <instance hex> [material] [out.png] [w] [h]: a wall/floor pattern's
        // materials, optionally rendered.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let inst = parse_hex(&args[3]);
        let d = set.read_ti(s3formats::catalog::T_CWAL, inst).expect("no such CWAL");
        let p = s3formats::catalog::WallFloorPattern::parse(&d).expect("parse");
        println!("{:?}: type {} ({} materials, {} keys)", p.name, p.pattern_type, p.materials.len(), p.keys.len());
        for (i, m) in p.materials.iter().enumerate() {
            let c = &m.complate;
            println!(
                "  [{i}] {} xml {:?} dae {:?} patterns {:?}",
                c.name,
                m.keys.get(c.xml as usize),
                c.get("daeFileName").map(|v| v.text()),
                c.blocks.iter().map(|b| format!("{}={} {:?}", b.pattern, b.name, b.get("Color"))).collect::<Vec<_>>()
            );
        }
        if let Some(out) = args.get(5) {
            let mi: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
            let w: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(256);
            let h: usize = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(256);
            let m = &p.materials[mi];
            let img = s3formats::complate::render(&set, &m.complate, &m.keys, w, h).expect("render");
            let f = std::fs::File::create(out).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width as u32, img.height as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&img.data).unwrap();
            println!("wrote {out}");
        }
        return;
    }
    if args[1] == "vpxy" {
        // vpxy <root> <type:group:instance hex>...: each VPXY's references, and its models'
        // meshes with bounds, vertex counts and diffuse textures.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        for a in &args[3..] {
            let p: Vec<&str> = a.split(':').collect();
            let k = s3pkg::ResourceKey::new(parse_hex(p[0]) as u32, parse_hex(p[1]) as u32, parse_hex(p[2]));
            let Some(v) = set.read(&k).or_else(|| set.read_ti(k.t, k.i)) else { println!("{k}: missing"); continue };
            println!("{k}:");
            for r in s3formats::model::vpxy_keys(&v) {
                println!("  refers to {r}");
            }
            for mk in s3formats::model::vpxy_models(&v) {
                println!("  model {mk}");
                for m in s3formats::model::load_model(&set, &mk).unwrap_or_default() {
                    println!(
                        "    mesh {:08X} verts {} tris {} bounds {:?}..{:?} tex {:?} joints {}",
                        m.name_hash,
                        m.positions.len(),
                        m.indices.len() / 3,
                        m.bounds_min,
                        m.bounds_max,
                        m.material.texture(s3formats::model::P_DIFFUSE_MAP),
                        m.joints.len()
                    );
                }
            }
        }
        return;
    }
    if args[1] == "caspresets" {
        // caspresets <root> [name part]: the base game's everyday clothes and hair (not hidden),
        // how many presets (colourways) they have, and how long a few take to render.
        use s3formats::sim::{CasPart, CAT_HIDDEN, CT_BODY, CT_BOTTOM, CT_HAIR, CT_SHOES, CT_TOP};
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let want = args.get(3).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        let mut parts = 0;
        let mut presets = 0;
        let mut hist = std::collections::BTreeMap::new();
        let mut sample: Vec<(String, String)> = Vec::new();
        for k in set.keys_of_type(s3pkg::types::CASP).copied().collect::<Vec<_>>() {
            let Some(c) = set.read(&k).and_then(|d| CasPart::parse(&d).ok()) else { continue };
            let human = matches!((c.age_gender >> 8) & 0xF, 0 | 1) && c.age_gender & 0x7E != 0;
            if !human || c.category & CAT_HIDDEN != 0 || k.g != 0 || !matches!(c.clothing_type, CT_TOP | CT_BOTTOM | CT_BODY | CT_SHOES | CT_HAIR) || !c.name.to_ascii_lowercase().contains(&want) {
                continue;
            }
            parts += 1;
            presets += c.presets.len();
            *hist.entry(c.presets.len()).or_insert(0) += 1;
            if sample.len() < 6 && c.presets.len() > 1 {
                sample.push((c.name.clone(), c.presets[1].clone()));
            }
        }
        println!("{parts} parts, {presets} presets; by count {hist:?}");
        if std::env::var("SHOW").is_ok()
            && let Some((name, p)) = sample.first()
        {
            println!("--- {name} preset 1:
{p}");
        }
        for (name, p) in &sample {
            let t0 = std::time::Instant::now();
            let img = s3formats::complate::render_preset(&set, p, 256, true);
            println!("{name}: rendered {:?} in {:?}", img.map(|i| (i.width, i.height)), t0.elapsed());
        }
        return;
    }
    if args[1] == "splitlevels" {
        // splitlevels <world file>: lots whose floors on one grid level stand at more than one
        // storey's height (a room on a foundation beside rooms without one), with the heights.
        use s3formats::lotdesign as ld;
        let w = Package::open(&args[2]).unwrap();
        for e in w.of_type(0xD063545B) {
            let Ok(l) = s3formats::world::LotInfo::parse(e.key.i, &w.read(e).unwrap()) else { continue };
            let Some(t) = s3formats::lot::LotTerrain::load(&w, l.id) else { continue };
            let Some(gdata) = w.find(&s3pkg::ResourceKey::new(ld::T_GRID, ld::G_FLOOR_GRID, l.id)).and_then(|e| w.read(e).ok()) else { continue };
            let Ok(grid) = ld::Grid::parse(&gdata) else { continue };
            let mut found = Vec::new();
            for gl in 0..(grid.levels as usize).min(t.levels.len()) {
                let mut bands = std::collections::BTreeMap::new();
                for x in 0..grid.width {
                    for z in 0..grid.depth {
                        if grid.quad(gl as u32, x, z).is_some_and(|q| q.iter().any(|v| *v != 0)) {
                            let h = t.at(gl, x as usize, z as usize);
                            *bands.entry((h * 4.0).round() as i32).or_insert(0) += 1;
                        }
                    }
                }
                let storeys: std::collections::BTreeSet<i32> = bands.keys().map(|k| ((*k as f32 / 4.0 + 1.75) / 3.0).floor() as i32).collect();
                if storeys.len() > 1 {
                    found.push(format!("grid {gl}: {:?}", bands.iter().map(|(k, n)| (*k as f32 / 4.0, *n)).collect::<Vec<_>>()));
                }
            }
            if !found.is_empty() {
                println!("{} {:016X}: {}", l.internal_name, l.id, found.join("; "));
            }
        }
        return;
    }
    if args[1] == "lotterrain" {
        // lotterrain <world file> <lot id hex> <x> <z>: the lot's level heights (relative to the
        // lot) at a vertex and their ranges, and the lot's corner height.
        let w = Package::open(&args[2]).unwrap();
        let lot = parse_hex(&args[3]);
        let (x, z): (usize, usize) = (args[4].parse().unwrap(), args[5].parse().unwrap());
        let t = s3formats::lot::LotTerrain::load(&w, lot).expect("terrain");
        println!("{}x{} vertices, {} levels", t.nx, t.nz, t.levels.len());
        for l in 0..t.levels.len() {
            let (lo, hi) = t.levels[l].iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
            println!("level {l}: at ({x},{z}) {:.3}, range {lo:.3}..{hi:.3}", t.at(l, x, z));
        }
        // (And the water table: there, its range, and a map of where it's above each level.)
        if let Some(wt) = &t.water {
            let (lo, hi) = wt.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
            println!("water: at ({x},{z}) {:.3}, range {lo:.3}..{hi:.3}", wt[x * t.nz + z]);
            if std::env::var("WATERMAP").is_ok() {
                let l: usize = std::env::var("WATERMAP").unwrap().parse().unwrap_or(1);
                for zz in 0..t.nz {
                    let row: String = (0..t.nx).map(|xx| if wt[xx * t.nz + zz] > t.at(l, xx, zz) + 0.01 { '~' } else { '.' }).collect();
                    println!("{row}");
                }
            }
        }
        return;
    }
    if args[1] == "stairs" {
        // stairs <root> <world file> <lot id hex>: the lot's staircases (0x04A09283), and each
        // stair style (CSTR 0x049CA4CD) they use, dumped with its TGI references.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let w = Package::open(&args[3]).unwrap();
        let lot = parse_hex(&args[4]);
        let rd = |t: u32, g: u32| w.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| w.read(e).ok());
        let refs = s3formats::objn::parse_refs(&rd(0x05ED1226, 0).unwrap_or_default()).unwrap_or_default();
        let Some(d) = rd(0x04A09283, 0) else { println!("no stairs"); return };
        let mut r = s3formats::util::Reader::new(&d);
        let ver = r.u32().unwrap();
        let n = r.u32().unwrap();
        println!("stairs v{ver}: {n}");
        let mut styles = std::collections::BTreeSet::new();
        for _ in 0..n {
            let unk = r.u32().unwrap();
            let style = r.u16().unwrap();
            let guid = r.u64().unwrap();
            let dir = r.u32().unwrap();
            let (x1, z1, l1) = (r.f32().unwrap(), r.f32().unwrap(), r.u32().unwrap());
            let (x2, z2, l2) = (r.f32().unwrap(), r.f32().unwrap(), r.u32().unwrap());
            let nr = r.u32().unwrap();
            let mut rails = Vec::new();
            for _ in 0..nr {
                let (_, rr, x, z) = (r.u32().unwrap(), r.u16().unwrap(), r.f32().unwrap(), r.f32().unwrap());
                rails.push((refs.get(&rr).copied(), x, z));
            }
            println!("  {unk} style r{style} {:?} guid {guid:016X} dir {dir} top ({x1},{z1}) L{l1} bottom ({x2},{z2}) L{l2} rails {rails:?}", refs.get(&style));
            if let Some(k) = refs.get(&style) {
                styles.insert(*k);
            }
        }
        for k in styles {
            let Some(d) = set.read(&k) else { println!("{k}: missing"); continue };
            println!("{k}: {} bytes", d.len());
            for (i, c) in d.chunks(16).enumerate().take(24) {
                println!("  {:04X} {}", i * 16, c.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "));
            }
        }
        return;
    }
    if args[1] == "floormap" {
        // floormap <world file> <lot id hex>: each floor-grid level as a map (a letter per
        // palette id, '.' for none), and the wall levels.
        use s3formats::lotdesign as ld;
        let w = Package::open(&args[2]).unwrap();
        let lot = parse_hex(&args[3]);
        let rd = |t: u32, g: u32| w.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| w.read(e).ok());
        let d = rd(ld::T_GRID, ld::G_FLOOR_GRID).expect("no floor grid");
        let g = ld::Grid::parse(&d).expect("grid");
        let pal = rd(ld::T_FLOOR_PALETTE, ld::G_FLOOR_PALETTE).and_then(|d| ld::parse_floor_palette(&d).ok()).unwrap_or_default();
        println!("grid {}x{}x{}, palette {} ids", g.width, g.depth, g.levels, pal.len());
        let mut missing = std::collections::BTreeMap::<u16, usize>::new();
        for l in 0..g.levels {
            println!("level {l}:");
            for z in 0..g.depth {
                let row: String = (0..g.width)
                    .map(|x| match g.quad(l, x, z) {
                        Some(q) if q.iter().any(|v| *v != 0) => {
                            let v = q.iter().copied().find(|v| *v != 0).unwrap_or(0);
                            if pal.contains_key(&(v as u32)) {
                                (b'A' + (v % 26) as u8) as char
                            } else {
                                *missing.entry(v).or_default() += 1;
                                '?'
                            }
                        }
                        _ => '.',
                    })
                    .collect();
                if row.chars().any(|c| c != '.') {
                    println!("{z:3} {row}");
                }
            }
        }
        println!("ids not in the palette: {missing:?}");
        let refs = s3formats::objn::parse_refs(&rd(0x05ED1226, 0).unwrap_or_default()).unwrap_or_default();
        let mut ids: Vec<_> = pal.iter().collect();
        ids.sort();
        for (id, (cwal, comp)) in ids {
            println!("  id {id} ({}) cwal r{cwal} {:?} comp r{comp} {:?}", (b'A' + (*id % 26) as u8) as char, refs.get(cwal), refs.get(comp));
        }
        return;
    }
    if args[1] == "lotfloors" {
        // lotfloors <root> <world file> <lot id hex>: the floor grids' values and the palettes'
        // pattern kinds.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let w = Package::open(&args[3]).unwrap();
        let lot = parse_hex(&args[4]);
        let rd = |t: u32, g: u32| w.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| w.read(e).ok());
        let refs = s3formats::objn::parse_refs(&rd(0x05ED1226, 0).unwrap()).unwrap_or_default();
        for pg in [0x002E7DF7u32, 0x004BE299, 0x008207AA, 0x00DD3460] {
            let Some(d) = rd(0xF12E5E12, pg) else { continue };
            let n = u32::from_le_bytes(d[20..24].try_into().unwrap()) as usize;
            println!("palette {pg:08X}: {n}");
            for i in 0..n.min(40) {
                let o = 24 + i * 10;
                let idx = u16::from_le_bytes([d[o], d[o + 1]]);
                let id = u32::from_le_bytes(d[o + 2..o + 6].try_into().unwrap());
                let area = u32::from_le_bytes(d[o + 6..o + 10].try_into().unwrap());
                let k = refs.get(&idx);
                let desc = k.and_then(|k| {
                    if k.t != s3formats::catalog::T_CWAL {
                        return Some(format!("{k}"));
                    }
                    let p = s3formats::catalog::WallFloorPattern::parse(&set.read(k)?).ok()?;
                    let dae = p.materials.first().and_then(|m| m.complate.get("daeFileName")).map(|v| v.text()).unwrap_or_default();
                    Some(format!("CWAL type {} {dae} ({} mats)", p.pattern_type, p.materials.len()))
                });
                println!("  id {id} r{idx} area {area}: {}", desc.unwrap_or("-".into()));
            }
        }
        let mut cw: Vec<_> = refs.iter().filter(|(_, k)| k.t == s3formats::catalog::T_CWAL).collect();
        cw.sort_by_key(|x| *x.0);
        for (i, k) in cw {
            let p = set.read(k).and_then(|d| s3formats::catalog::WallFloorPattern::parse(&d).ok());
            println!(
                "  REFS r{i} CWAL {:016X}: {:?}",
                k.i,
                p.map(|p| (p.pattern_type, p.materials.first().and_then(|m| m.complate.get("daeFileName")).map(|v| v.text()), p.materials.len()))
            );
        }
        for g in [0x002E7B0Eu32, 0x002E7CF0, 0x002E7CF1] {
            let Some(d) = rd(0xB125533A, g) else { continue };
            let (wd, ht, lv) = (
                u32::from_le_bytes(d[4..8].try_into().unwrap()),
                u32::from_le_bytes(d[8..12].try_into().unwrap()),
                u32::from_le_bytes(d[12..16].try_into().unwrap()),
            );
            let cells = &d[16..];
            let mut hist = BTreeMap::<u16, usize>::new();
            for c in cells.chunks_exact(2) {
                *hist.entry(u16::from_le_bytes([c[0], c[1]])).or_default() += 1;
            }
            println!("grid {g:08X}: {wd}x{ht}x{lv}, {} bytes, values {:?}", cells.len(), hist.iter().take(30).collect::<Vec<_>>());
        }
        return;
    }
    if args[1] == "lotcovers" {
        // lotcovers <root> <world file> <lot id hex> <outdir>: render the lot's floor designs.
        use s3formats::lotdesign as ld;
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let w = Package::open(&args[3]).unwrap();
        let lot = parse_hex(&args[4]);
        let out = std::path::Path::new(&args[5]);
        std::fs::create_dir_all(out).unwrap();
        let rd = |t: u32, g: u32| w.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| w.read(e).ok());
        let refs = s3formats::objn::parse_refs(&rd(0x05ED1226, 0).unwrap()).unwrap_or_default();
        let designs = ld::parse_designs(&rd(ld::T_DESIGNS, 0).unwrap_or_default());
        let fpal = rd(ld::T_FLOOR_PALETTE, ld::G_FLOOR_PALETTE).and_then(|d| ld::parse_floor_palette(&d).ok()).unwrap_or_default();
        println!("{} designs, {} floor palette entries", designs.len(), fpal.len());
        let save = |name: &str, img: &s3formats::dds::Rgba| {
            let f = std::fs::File::create(out.join(name)).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width as u32, img.height as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&img.data).unwrap();
        };
        let mut ids: Vec<_> = fpal.iter().collect();
        ids.sort();
        for (id, (cwal, comp)) in ids {
            let d = designs.get(comp);
            let keys: Vec<s3pkg::ResourceKey> = d.map(|d| d.refs.iter().map(|r| refs.get(r).copied().unwrap_or(s3pkg::ResourceKey::new(0, 0, 0))).collect()).unwrap_or_default();
            let desc = d.map(|d| {
                format!(
                    "{} dae {:?} xml {:?} patterns {:?}",
                    d.complate.name,
                    d.complate.get("daeFileName").map(|v| v.text()),
                    keys.get(d.complate.xml as usize),
                    d.complate.blocks.iter().map(|b| format!("{}={} xml {:?}", b.pattern, b.name, keys.get(b.xml as usize).map(|k| k.t))).collect::<Vec<_>>()
                )
            });
            let img = d.and_then(|d| s3formats::complate::render(&set, &d.complate, &keys, 256, 256));
            let stats = img.as_ref().map(|i| {
                let n = (i.data.len() / 4) as f32;
                let mean = |c: usize| i.data.chunks(4).map(|p| p[c] as f32).sum::<f32>() / n;
                format!("mean rgba {:.0} {:.0} {:.0} {:.0}", mean(0), mean(1), mean(2), mean(3))
            });
            println!("floor id {id}: cwal r{cwal} comp r{comp} -> {desc:?} render {stats:?}");
            if let Some(img) = img {
                save(&format!("floor_{id}.png"), &img);
            }
        }
        return;
    }
    if args[1] == "wallsides" {
        // wallsides <root> <world file> <lot id hex>: on outside walls, which covering side
        // (A or B) faces the outdoor room — by the pattern names landing there.
        use s3formats::lotdesign as ld;
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let w = Package::open(&args[3]).unwrap();
        let lot = parse_hex(&args[4]);
        let rd = |t: u32, g: u32| w.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| w.read(e).ok());
        let refs = s3formats::objn::parse_refs(&rd(0x05ED1226, 0).unwrap()).unwrap_or_default();
        let graph = s3formats::lot::WallGraph::parse(&rd(0x312E7545, 0x2E7B1E).unwrap()).unwrap();
        let sides = ld::parse_wall_sides(&rd(ld::T_WALL_SIDES, ld::G_WALL_PATTERNS).unwrap()).unwrap();
        let pal = ld::parse_palette(&rd(ld::T_PALETTE, ld::G_WALL_PATTERN_PALETTE).unwrap()).unwrap();
        let designs = ld::parse_designs(&rd(ld::T_DESIGNS, 0).unwrap_or_default());
        println!("{} designs parsed", designs.len());
        let name = |id: Option<u32>| -> String {
            let Some(k) = id.and_then(|i| pal.get(&i)).and_then(|r| refs.get(r)) else { return "-".into() };
            set.read(k)
                .and_then(|d| s3formats::catalog::WallFloorPattern::parse(&d).ok())
                .and_then(|p| p.materials.first().and_then(|m| m.complate.get("daeFileName")).map(|v| v.text()))
                .unwrap_or_else(|| format!("{k}"))
        };
        let by_edge: std::collections::HashMap<u32, &ld::WallSides> = sides.iter().map(|s| (s.edge, s)).collect();
        let mut tally = BTreeMap::<String, usize>::new();
        let mut rooms = BTreeMap::<(u32, u32), usize>::new();
        for e in &graph.edges {
            *rooms.entry((e.left.min(99), e.right.min(99))).or_default() += 1;
        }
        println!("1E left/right rooms: {:?}", rooms.iter().take(30).collect::<Vec<_>>());
        let walls = s3formats::lot::WallGraph::parse(&rd(0x312E7545, 0x2E7B1A).unwrap()).unwrap();
        let mut wrooms = BTreeMap::<(u32, u32), usize>::new();
        for e in &walls.edges {
            *wrooms.entry((e.left.min(99), e.right.min(99))).or_default() += 1;
        }
        println!("1A left/right rooms: {:?}", wrooms.iter().take(30).collect::<Vec<_>>());
        let segs: Vec<_> = walls.segments().collect();
        for (a, b, level, e) in graph.segments() {
            let Some(s) = by_edge.get(&e.id) else { continue };
            // The wall this edge lies on.
            let on = |p: [f32; 2], q: [f32; 2], r: [f32; 2]| {
                let (dx, dz) = (r[0] - q[0], r[1] - q[1]);
                let len2 = dx * dx + dz * dz;
                if len2 < 1e-6 {
                    return false;
                }
                let t = ((p[0] - q[0]) * dx + (p[1] - q[1]) * dz) / len2;
                let (cx, cz) = (q[0] + dx * t - p[0], q[1] + dz * t - p[1]);
                (-0.01..=1.01).contains(&t) && cx * cx + cz * cz < 1e-4
            };
            let Some(&(wa, wb, _, we)) = segs.iter().find(|w| w.2 == level && on(a, w.0, w.1) && on(b, w.0, w.1)) else { continue };
            if (we.left == 0) == (we.right == 0) {
                continue;
            }
            let same = (b[0] - a[0]) * (wb[0] - wa[0]) + (b[1] - a[1]) * (wb[1] - wa[1]) > 0.0;
            let (left, _right) = if same { (we.left, we.right) } else { (we.right, we.left) };
            let (outdoor, indoor) = if left == 0 { (s.a, s.b) } else { (s.b, s.a) };
            *tally.entry(format!("A-is-left: outdoor={} indoor={}", name(outdoor), name(indoor))).or_default() += 1;
        }
        for (k, v) in tally {
            println!("{v:4} {k}");
        }
        return;
    }
    if args[1] == "wallgraph" {
        // wallgraph <world> <lot id hex> <group hex>: a lot's wall-graph edges (lot tiles).
        let w = Package::open(&args[2]).unwrap();
        let lot = parse_hex(&args[3]);
        let g = parse_hex(&args[4]) as u32;
        let Some(d) = w.find(&s3pkg::ResourceKey::new(s3formats::lot::T_WALL_GRAPH, g, lot)).and_then(|e| w.read(e).ok()) else { println!("none"); return };
        match s3formats::lot::WallGraph::parse(&d) {
            Ok(gr) => {
                println!("{}x{}, {} vertices, {} edges", gr.width, gr.depth, gr.vertices.len(), gr.edges.len());
                for (a, b, level, e) in gr.segments() {
                    println!("{a:?}-{b:?} level {level} rooms {}|{}", e.left, e.right);
                }
            }
            Err(e) => println!("parse failed: {e:?}"),
        }
        return;
    }
    if args[1] == "lotwalls" {
        // lotwalls <root> <world file> <lot id hex>: each wall-side channel resolved through its
        // palette and the lot's REFS table.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let w = Package::open(&args[3]).unwrap();
        let lot = parse_hex(&args[4]);
        let rd = |t: u32, g: u32| w.find(&s3pkg::ResourceKey::new(t, g, lot)).and_then(|e| w.read(e).ok());
        let refs = rd(0x05ED1226, 0).and_then(|d| s3formats::objn::parse_refs(&d).ok()).unwrap_or_default();
        println!("{} REFS entries", refs.len());
        let tname = |t: u32| match t {
            0x515CA4CD => "CWAL",
            0x9151E6BC => "CWST",
            0x033A1435 => "TXTC",
            0x0341ACC9 => "TXTF",
            0x044AE110 => "COMP",
            0x00B2D882 => "DDS",
            0x0333406C => "_XML",
            0x0418FE2A => "CFEN",
            _ => "?",
        };
        let found = |k: &s3pkg::ResourceKey| {
            if w.find(k).is_some() {
                "world"
            } else if set.read(k).is_some() {
                "install"
            } else if w.entries.iter().any(|e| e.key.t == k.t && e.key.i == k.i) {
                "world(other group)"
            } else if set.find_ti(k.t, k.i).is_some() {
                "install(other group)"
            } else {
                "MISSING"
            }
        };
        // Palette: u32 version, 16 bytes, u32 count, then (u16 REFS index, u32 palette id, u32 area).
        let palette = |g: u32| -> std::collections::HashMap<u32, u16> {
            let mut out = std::collections::HashMap::new();
            let Some(d) = rd(0xF12E5E12, g) else { return out };
            let n = u32::from_le_bytes(d[20..24].try_into().unwrap()) as usize;
            for i in 0..n {
                let o = 24 + i * 10;
                if o + 10 > d.len() {
                    break;
                }
                let idx = u16::from_le_bytes([d[o], d[o + 1]]);
                let id = u32::from_le_bytes(d[o + 2..o + 6].try_into().unwrap());
                out.insert(id, idx);
            }
            out
        };
        for e in w.entries.iter().filter(|e| e.key.i == lot && (e.key.t == 0xB1422971 || e.key.t == 0xF12E5E12)) {
            println!("{} len {}", e.key, w.read(e).map(|d| d.len()).unwrap_or(0));
        }
        let pairs: Vec<(u32, u32)> = std::env::var("PAIRS")
            .ok()
            .map(|v| {
                v.split(',')
                    .filter_map(|p| {
                        let (a, b) = p.split_once('/')?;
                        Some((parse_hex(a) as u32, parse_hex(b) as u32))
                    })
                    .collect()
            })
            .unwrap_or(vec![(0x002FDACF, 0x002E7DF7), (0x0082079A, 0x008207AA), (0x004BFAAB, 0x004BE299), (0x00DD33E4, 0x00DD3460)]);
        let graphs: Vec<(u32, s3formats::lot::WallGraph)> = [0x2E7B1A, 0x2E7B1C, 0x2E7B1D, 0x2E7B1E, 0x2E7B1F]
            .into_iter()
            .filter_map(|g| Some((g, rd(0x312E7545, g).and_then(|d| s3formats::lot::WallGraph::parse(&d).ok())?)))
            .collect();
        for (g, gr) in &graphs {
            println!("graph {g:08X}: {} vertices, {} edges, edge ids {:?}..", gr.vertices.len(), gr.edges.len(), gr.edges.iter().take(5).map(|e| e.id).collect::<Vec<_>>());
        }
        for (g, pg) in pairs {
            let Some(d) = rd(0xB1422971, g) else { println!("no B1422971:{g:08X}"); continue };
            {
                let n = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
                let ids: std::collections::HashSet<u32> = (0..n).filter_map(|i| d.get(8 + i * 10..12 + i * 10)).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect();
                for (gg, gr) in &graphs {
                    let hit = gr.edges.iter().filter(|e| ids.contains(&e.id)).count();
                    println!("  {g:08X} ids in graph {gg:08X}: {hit}/{}", ids.len());
                }
            }
            let pal = palette(pg);
            let n = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
            println!("== channel {g:08X} / palette {pg:08X}: {n} edges, {} palette entries", pal.len());
            let mut kinds = BTreeMap::<String, usize>::new();
            for i in 0..n {
                let o = 8 + i * 10;
                if o + 10 > d.len() {
                    break;
                }
                let edge = u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
                let style = u16::from_le_bytes([d[o + 4], d[o + 5]]);
                let a = u16::from_le_bytes([d[o + 6], d[o + 7]]);
                let b = u16::from_le_bytes([d[o + 8], d[o + 9]]);
                let res = |id: u16| -> String {
                    if id == 0xFFFF {
                        return "-".into();
                    }
                    match pal.get(&(id as u32)).and_then(|ri| refs.get(ri).map(|k| (ri, k))) {
                        Some((ri, k)) => format!("{id}->r{ri} {} {k} [{}]", tname(k.t), found(k)),
                        None => format!("{id}->?"),
                    }
                };
                let sk = refs.get(&style).map(|k| format!("{} {}", tname(k.t), found(k))).unwrap_or("-".into());
                for x in [a, b] {
                    if x != 0xFFFF {
                        let k = pal.get(&(x as u32)).and_then(|ri| refs.get(ri)).map(|k| format!("{} {}", tname(k.t), found(k))).unwrap_or("unresolved".into());
                        *kinds.entry(k).or_default() += 1;
                    }
                }
                if i < 6 {
                    println!("  edge {edge} style r{style} ({sk}) A {} | B {}", res(a), res(b));
                }
            }
            println!("  side kinds: {kinds:?}");
        }
        return;
    }
    if args[1] == "lotrefs" {
        // lotrefs <root> <world file> <lot id hex>: which of the lot's resources mention wall,
        // floor and pattern catalogue entries.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let mut catalog: std::collections::HashMap<u64, &str> = std::collections::HashMap::new();
        for (t, name) in [(0x515CA4CDu32, "CWAL"), (0xB4DD716B, "CFLR?"), (0xD4D9FBE5, "PTRN"), (0x316C78F2, "CFND"), (0x9151E6BC, "CWST")] {
            for k in set.keys_of_type(t) {
                catalog.insert(k.i, name);
            }
        }
        println!("{} catalogue entries", catalog.len());
        let w = Package::open(&args[3]).unwrap();
        let lot = parse_hex(&args[4]);
        for e in w.entries.iter().filter(|e| e.key.i == lot) {
            let Ok(d) = w.read(e) else { continue };
            let mut hits = BTreeMap::<&str, usize>::new();
            for o in 0..d.len().saturating_sub(8) {
                let v = u64::from_le_bytes(d[o..o + 8].try_into().unwrap());
                if let Some(n) = catalog.get(&v) {
                    *hits.entry(n).or_default() += 1;
                }
            }
            if !hits.is_empty() {
                println!("{} len {} -> {hits:?}", e.key, d.len());
            }
        }
        return;
    }
    if args[1] == "bond" {
        // bond <root> <name>...: a BOND's bone adjustments (group 1, instance FNV-32 of the name).
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        for name in &args[3..] {
            let k = s3pkg::ResourceKey::new(0x0355E0A6, 1, s3pkg::fnv32(name) as u64);
            match set.read(&k).map(|d| s3formats::sim::parse_bond(&d)) {
                Some(Ok(v)) => {
                    println!("{name} {k}: {} bones", v.len());
                    for a in v {
                        println!("  {:08x} off {:?} scale {:?} rot {:?}", a.bone, a.offset, a.scale, a.rotation);
                    }
                }
                Some(Err(e)) => println!("{name}: unparsed {e:?}"),
                None => println!("{name}: not found"),
            }
        }
        return;
    }
    if args[1] == "faces" {
        // faces <root> [filter]: the face sliders (FACE 0x0358B08A): name, version, regions, BGEO.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let filter = args.get(3).map(|s| s.to_ascii_lowercase());
        let mut rows = Vec::new();
        for k in set.keys_of_type(0x0358B08A).copied().collect::<Vec<_>>() {
            let Some(d) = set.read(&k) else { continue };
            let version = u32::from_le_bytes(d[..4].try_into().unwrap());
            let Ok(b) = s3formats::sim::BlendInfo::parse(&d) else {
                rows.push(format!("{k} v{version}: unparsed"));
                continue;
            };
            if filter.as_ref().is_some_and(|f| !b.name.to_ascii_lowercase().contains(f)) {
                continue;
            }
            let regions: Vec<String> = b.entries.iter().map(|e| format!("{:x}:{}g/{}b", e.region, e.geoms.len(), e.bones.len())).collect();
            rows.push(format!("{} v{version} bgeo {:?} regions {regions:?}", b.name, b.bgeo.map(|k| k.to_string())));
            // (DETAIL=1: each entry's bone adjustments.)
            if std::env::var("DETAIL").is_ok() {
                for e in &b.entries {
                    for (ag, amount, idx) in &e.bones {
                        let Some(key) = b.keys.get(*idx as usize) else { continue };
                        let adj = set.read(key).or_else(|| set.read_ti(key.t, key.i)).and_then(|d| s3formats::sim::parse_bond(&d).ok()).unwrap_or_default();
                        rows.push(format!("    region {:x} ag {ag:x} amount {amount} {key}: {} bones", e.region, adj.len()));
                        for a in adj.iter().take(6) {
                            rows.push(format!("      bone {:08x} off {:?} scale {:?} rot {:?}", a.bone, a.offset, a.scale, a.rotation));
                        }
                    }
                }
            }
        }
        rows.sort();
        for r in &rows {
            println!("{r}");
        }
        println!("{} sliders", rows.len());
        return;
    }
    if args[1] == "objsclasses" {
        // objsclasses <world file> [filter]: the object graph's classes (and how many of each).
        let w = Package::open(&args[2]).unwrap();
        let e = w.of_type(s3formats::objs::T_OBJS).next().expect("no OBJS");
        let d = w.read(e).unwrap();
        let objs = s3formats::objs::ObjStream::parse(&d).expect("OBJS");
        let mut n = BTreeMap::<String, usize>::new();
        for id in 1..=objs.len() as u32 {
            *n.entry(objs.class_name(id).unwrap_or("?").to_string()).or_default() += 1;
        }
        for (c, k) in n.iter().filter(|(c, _)| args.get(3).is_none_or(|f| c.contains(f.as_str()))) {
            println!("{k:6} {c}");
        }
        return;
    }
    if args[1] == "objsdump" {
        // objsdump <world file> <class name> [count]: decoded fields of objects of a class.
        let w = Package::open(&args[2]).unwrap();
        let e = w.of_type(s3formats::objs::T_OBJS).next().expect("no OBJS");
        let d = w.read(e).unwrap();
        let objs = s3formats::objs::ObjStream::parse(&d).expect("OBJS");
        if let Ok(ks) = std::env::var("KEYS") {
            for i in ks.split(',').filter_map(|x| x.parse::<usize>().ok()) {
                let k = objs.keys.get(i);
                println!("key {i}: {k:?} in world: {:?}", k.map(|k| w.find(k).is_some()));
            }
        }
        let n: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(3);
        for id in objs.objects_of(&args[3]).into_iter().take(n) {
            println!("#{id} {}", objs.class_name(id).unwrap_or("?"));
            // (RAW=1: every object's bytes too.)
            if objs.fields(id).is_none() || std::env::var("RAW").is_ok() {
                let c = &objs.classes[objs.class(id).unwrap()];
                println!("   (undecodable) fields {:?}", c.fields.iter().map(|(n, t)| format!("{n}:{t:x}")).collect::<Vec<_>>());
                println!("   {:02x?}", objs.raw(id));
            }
            for (name, v) in objs.fields(id).unwrap_or_default() {
                let extra = match &v {
                    s3formats::objs::Value::Ref(r) if *r != 0 => {
                        format!(" -> {} {:?}", objs.class_name(*r).unwrap_or("array/?"), objs.string(*r).or_else(|| objs.fields(*r).map(|f| format!("{:?}", &f[..f.len().min(4)]))))
                    }
                    _ => String::new(),
                };
                println!("   {name} = {v:?}{extra}");
            }
        }
        return;
    }
    if args[1] == "premades" {
        // premades <root> <world file>: the world's premade households.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let strings = s3formats::stbl::load_english(&set);
        let tr = |k: &str| strings.get(&s3pkg::fnv64(k)).cloned().unwrap_or_else(|| k.to_string());
        let w = Package::open(&args[3]).unwrap();
        let e = w.of_type(s3formats::objs::T_OBJS).next().expect("no OBJS");
        let d = w.read(e).unwrap();
        let objs = s3formats::objs::ObjStream::parse(&d).expect("OBJS");
        println!("{} classes, {} objects, {} keys", objs.classes.len(), objs.len(), objs.keys.len());
        let p = s3formats::premade::read(&objs);
        for h in &p.households {
            println!("{} ({}) lot {:016X} funds {}", tr(&h.name), h.members.len(), h.lot_id, h.funds);
            for s in &h.members {
                println!(
                    "   {} {} age {:x} {} traits {:?} skin {:?}/{:.2} hair {:08X?} body f{:.2} t{:.2} fit{:.2} partner {:?} spouse {:?} parents {:?} career {:?} skills {:?}",
                    tr(&s.first_name), tr(&s.last_name), s.age, if s.female { "F" } else { "M" }, s.traits, s.skin_tone.map(|k| k.2), s.skin_shade, s.hair_color, s.fat, s.thin, s.fit, s.partner, s.spouse, s.parents, s.career, s.skills
                );
            }
        }
        let mut states = BTreeMap::<String, usize>::new();
        for r in &p.relationships {
            *states.entry(r.state.clone()).or_default() += 1;
        }
        println!("{} relationships: {states:?}", p.relationships.len());
        return;
    }
    if args[1] == "strkey" {
        // strkey <root> <key>...: the English text of string keys (fnv64 of the lowercase key).
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let strings = s3formats::stbl::load_english(&set);
        for k in &args[3..] {
            println!("{k}: {}", strings.get(&s3pkg::fnv64(&k.to_ascii_lowercase())).cloned().unwrap_or_default());
        }
        return;
    }
    if args[1] == "strgrep" {
        // strgrep <root> <text>: string-table entries whose text contains the text (any case).
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let strings = s3formats::stbl::load_english(&set);
        let want = args[3].to_ascii_lowercase();
        let mut hits: Vec<(&u64, &String)> = strings.iter().filter(|(_, v)| v.to_ascii_lowercase().contains(&want)).collect();
        hits.sort_by(|a, b| a.1.cmp(b.1));
        for (k, v) in hits {
            println!("{k:016X}  {}", v.replace(['\n', '\r'], " / "));
        }
        return;
    }
    if args[1] == "strfind" {
        // strfind <root> <text>...: string-table keys whose text equals one of the texts.
        let set = s3pkg::install::open_install(std::path::Path::new(&args[2]), |_| true);
        let strings = s3formats::stbl::load_english(&set);
        for t in &args[3..] {
            for (k, v) in &strings {
                if v == t {
                    println!("{t}: {k:016X}");
                }
            }
        }
        return;
    }
    if args[1] == "sounds" {
        // sounds <root> [name filter]: sound property records with their samples.
        use s3formats::audio;
        let root = std::path::Path::new(&args[2]);
        let set = s3pkg::install::open_install(root, |_| true);
        let mut names = std::collections::HashMap::new();
        for k in set.keys_of_type(types::NMAP).copied().collect::<Vec<_>>() {
            for (id, n) in audio::parse_name_map(&set.read(&k).unwrap_or_default()) {
                names.insert(id, n);
            }
        }
        let filter = args.get(3).cloned().unwrap_or_default();
        let mut keys: Vec<_> = set.keys_of_type(audio::T_SOUND_PROPS).copied().collect();
        keys.sort_by_key(|k| names.get(&k.i).cloned().unwrap_or_default());
        let mut kinds = BTreeMap::<u32, usize>::new();
        let (mut shown, mut total) = (0, 0);
        for k in &keys {
            total += 1;
            let name = names.get(&k.i).cloned().unwrap_or_else(|| format!("{:016X}", k.i));
            let d = set.read(k).unwrap_or_default();
            let p = audio::SoundProps::parse(&d);
            for (h, _) in &p.props {
                *kinds.entry(*h).or_default() += 1;
            }
            if !name.contains(&filter) || shown >= 200000 {
                continue;
            }
            shown += 1;
            let samples: Vec<String> = p.samples().iter().map(|s| names.get(s).cloned().unwrap_or_else(|| format!("{s:016X}"))).collect();
            let parent = p.parent().map(|s| names.get(&s).cloned().unwrap_or_else(|| format!("{s:016X}")));
            let other: Vec<String> = p.props.iter().filter(|(h, _)| *h != audio::P_SAMPLES && *h != audio::P_PARENT).map(|(h, v)| format!("{h:08X}={v:?}")).collect();
            println!("{name} [{}] parent={parent:?} samples={samples:?} {}", d.len(), other.join(" "));
        }
        println!("{total} records; property usage: {kinds:X?}");
        let snr: Vec<_> = set.keys_of_type(audio::T_SNR).copied().collect();
        let sns: Vec<_> = set.keys_of_type(audio::T_SNS).copied().collect();
        println!("{} SNR, {} SNS", snr.len(), sns.len());
        if filter == "@streams" {
            for k in &sns {
                let h = set.read(&k).map(|d| d[..16.min(d.len())].to_vec()).unwrap_or_default();
                let partner = snr.iter().find(|s| s.i == k.i).map(|s| set.read(s).map(|d| audio::Snr::parse(&d))).flatten().flatten();
                println!("SNS {k} {} {:02X?} snr={partner:?}", names.get(&k.i).cloned().unwrap_or_default(), h);
            }
        }
        return;
    }
    if args[1] == "snr" {
        // snr <out dir> <files...>: convert dumped SNR samples to .mp3 / .wav.
        use s3formats::audio;
        let out = std::path::Path::new(&args[2]);
        std::fs::create_dir_all(out).unwrap();
        for f in &args[3..] {
            let d = std::fs::read(f).unwrap();
            let Some(snr) = audio::Snr::parse(&d) else { continue };
            let blocks = audio::blocks(&d[snr.header_len..]);
            let stem = std::path::Path::new(f).file_stem().unwrap().to_string_lossy().into_owned();
            match snr.codec {
                audio::CODEC_EALAYER3_V1 => match audio::ealayer3_to_mp3(&snr, &blocks) {
                    Ok(m) => {
                        println!("{stem}: {} Hz {} ch {} samples -> {} granules ({}), mp3 {} frames {} bytes, pcm prefix {} skip {} ({} pcm granules, {} pcm samples)", snr.sample_rate, snr.channels, snr.samples, m.granules, m.granules * 576, m.frames, m.data.len(), m.pcm_prefix.len(), m.pcm_skip, m.pcm_blocks, m.pcm_total);
                        std::fs::write(out.join(format!("{stem}.mp3")), &m.data).unwrap();
                    }
                    Err(e) => println!("{stem}: {e}"),
                },
                audio::CODEC_XAS1 => {
                    let pcm = audio::decode_xas(&snr, &blocks);
                    let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
                    println!("{stem}: {snr:?} -> {} samples, peak {peak}", pcm.len());
                    std::fs::write(out.join(format!("{stem}.wav")), audio::wav(&pcm, snr.channels as u16, snr.sample_rate)).unwrap();
                }
                c => println!("{stem}: codec {c} unsupported"),
            }
        }
        return;
    }
    if args[1] == "install" {
        let root = std::path::Path::new(&args[2]);
        for r in s3pkg::install::discover_packages(root) {
            println!("{:5} {}", r.priority, r.path.display());
        }
        let t0 = std::time::Instant::now();
        let set = s3pkg::install::open_install(root, |_| true);
        println!("{} resources in {:?}", set.len(), t0.elapsed());
        for name in args.iter().skip(3) {
            let (t, i) = name.split_once(':').unwrap();
            let t = parse_hex(t) as u32;
            let i = if i.starts_with("0x") { parse_hex(i) } else { s3pkg::fnv64(i) };
            match set.find_ti(t, i) {
                Some(k) => {
                    let (p, e) = set.get_entry(&k).unwrap();
                    println!("{name}: {k} in {} size={}", p.path.display(), e.mem_size);
                    if let Some(out) = std::env::var_os("DUMP_DIR") {
                        let data = set.read(&k).unwrap();
                        std::fs::write(std::path::Path::new(&out).join(format!("{:08X}_{:016X}.bin", k.t, k.i)), data).unwrap();
                    }
                }
                None => println!("{name}: not found"),
            }
        }
        return;
    }
    let pkg = Package::open(&args[2]).expect("open package");
    match args[1].as_str() {
        "types" => {
            let mut hist: BTreeMap<u32, (usize, u64, u64)> = BTreeMap::new();
            for e in &pkg.entries {
                let h = hist.entry(e.key.t).or_default();
                h.0 += 1;
                h.1 += e.file_size as u64;
                h.2 += e.mem_size as u64;
            }
            println!("{} entries", pkg.entries.len());
            for (t, (n, fs, ms)) in hist {
                println!("{t:08X} {:6} n={n:7} file={fs:12} mem={ms:12}", types::name(t));
            }
        }
        "list" => {
            let filter = args.get(3).map(|s| parse_hex(s) as u32);
            for e in &pkg.entries {
                if filter.is_none_or(|t| t == e.key.t) {
                    println!(
                        "{} {:6} off={} fsize={} msize={} comp={:04X}",
                        e.key,
                        types::name(e.key.t),
                        e.offset,
                        e.file_size,
                        e.mem_size,
                        e.compressed
                    );
                }
            }
        }
        "dump" | "hex" => {
            let parts: Vec<&str> = args[3].split(':').collect();
            let t = parse_hex(parts[0]) as u32;
            let g = parse_hex(parts[1]) as u32;
            let i = parse_hex(parts[2]);
            let e = pkg
                .entries
                .iter()
                .find(|e| e.key.t == t && e.key.g == g && e.key.i == i)
                .expect("resource not found");
            let data = pkg.read(e).expect("read");
            if args[1] == "dump" {
                std::fs::write(&args[4], &data).expect("write");
                println!("wrote {} bytes", data.len());
            } else {
                let n: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(512);
                hexdump(&data[..n.min(data.len())]);
            }
        }
        "dumpall" => {
            let t = parse_hex(&args[3]) as u32;
            let dir = &args[4];
            let max: usize = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(usize::MAX);
            std::fs::create_dir_all(dir).unwrap();
            for e in pkg.of_type(t).take(max) {
                let data = pkg.read(e).expect("read");
                let name = format!("{}/{:08X}_{:08X}_{:016X}.bin", dir, e.key.t, e.key.g, e.key.i);
                std::fs::write(name, data).unwrap();
            }
        }
        "find" => {
            // find <package> <hex u64|u32>: resources containing the little-endian value.
            let v = parse_hex(&args[3]);
            let pat = if args[3].len() > 8 { v.to_le_bytes().to_vec() } else { (v as u32).to_le_bytes().to_vec() };
            let mut hits = std::collections::BTreeMap::<u32, usize>::new();
            for e in pkg.entries.iter().filter(|e| !e.is_deleted()) {
                let Ok(data) = pkg.read(e) else { continue };
                if let Some(off) = data.windows(pat.len()).position(|w| w == pat.as_slice()) {
                    let n = hits.entry(e.key.t).or_default();
                    if *n < 3 {
                        println!("{:08X}:{:08X}:{:016X} @ {off}", e.key.t, e.key.g, e.key.i);
                    }
                    *n += 1;
                }
            }
            println!("{hits:X?}");
        }
        _ => eprintln!("unknown command"),
    }
}

fn hexdump(d: &[u8]) {
    for (row, chunk) in d.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        let asc: String = chunk
            .iter()
            .map(|&b| if (32..127).contains(&b) { b as char } else { '.' })
            .collect();
        println!("{:08X}  {:<48} {}", row * 16, hex.join(" "), asc);
    }
}
