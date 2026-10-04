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
