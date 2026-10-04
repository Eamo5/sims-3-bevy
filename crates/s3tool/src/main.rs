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
