//! Command-line baker: `s3bake [--data <install>] [--world <name>]... [--all-worlds]`.

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut data = PathBuf::from(std::env::var("SIMS3_DATA").unwrap_or_else(|_| String::from("//RYZEN-PC/Users/Laura/Documents/The Sims 3")));
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
    let t0 = Instant::now();
    let progress = |s: &str| println!("[{:7.1}s] {s}", t0.elapsed().as_secs_f32());
    println!("Opening {}", data.display());
    let mut pkgs = s3pkg::install::open_install(&data, |_| true);
    println!("{} resources", pkgs.len());
    if force || root.global_manifest().is_none() {
        match s3bake::bake_global(&root, &pkgs, &data.to_string_lossy(), &progress) {
            Ok(m) => println!("global: {m:?}"),
            Err(e) => {
                eprintln!("global bake failed: {e}");
                std::process::exit(1);
            }
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
