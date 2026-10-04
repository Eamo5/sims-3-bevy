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
    if let Some(i) = args.iter().position(|a| a == "--lots") {
        let w: s3bake::WorldBaked = s3bake::read_value(&root.world_dir(&args[i + 1]).join("world.bin")).expect("world");
        for (k, l) in w.lots.iter().enumerate() {
            if !l.info.is_residential() {
                println!("{k:3} {:40} {}", l.info.internal_name, l.display_name);
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
