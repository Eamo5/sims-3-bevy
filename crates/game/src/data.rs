//! Locating the user's Sims 3 installation and holding the opened game data.

use std::path::PathBuf;
use bevy::prelude::*;

const DEFAULT_INSTALL: &str = r"\\RYZEN-PC\Users\Laura\Documents\The Sims 3";

/// Root folder of the installed game (the one containing `GameData`, `EP1`, ...).
#[derive(Resource, Clone)]
pub struct InstallPath(pub PathBuf);

#[derive(Clone, Debug)]
pub struct WorldEntry {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Resource, Default)]
pub struct WorldList(pub Vec<WorldEntry>);

/// The world the player picked in the main menu.
#[derive(Resource, Clone)]
pub struct SelectedWorld(pub WorldEntry);


pub struct DataPlugin;

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        let path = find_install();
        info!("Using Sims 3 installation at {}", path.display());
        let worlds = list_worlds(&path);
        app.insert_resource(InstallPath(path))
            .insert_resource(WorldList(worlds));
    }
}

fn find_install() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--data" {
            if let Some(p) = args.next() {
                return PathBuf::from(p);
            }
        }
    }
    if let Some(p) = std::env::var_os("SIMS3_DATA") {
        return PathBuf::from(p);
    }
    let candidates = [
        PathBuf::from(DEFAULT_INSTALL),
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\The Sims 3"),
        PathBuf::from(r"C:\Program Files (x86)\Electronic Arts\The Sims 3"),
        PathBuf::from(r"C:\Program Files (x86)\Origin Games\The Sims 3"),
    ];
    candidates
        .iter()
        .find(|p| p.join("GameData").is_dir())
        .cloned()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_INSTALL))
}

fn list_worlds(root: &std::path::Path) -> Vec<WorldEntry> {
    let mut worlds: Vec<WorldEntry> = s3pkg::install::discover_worlds(root)
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            // Skip internal test / tiny vacation sub-worlds that aren't playable towns.
            let lower = name.to_ascii_lowercase();
            if lower.contains("test") || lower.starts_with("gpe") {
                return None;
            }
            Some(WorldEntry { name: pretty_world_name(&name), path })
        })
        .collect();
    // Worlds already converted into the local cache stay playable without the installation.
    let cached = s3bake::default_root().dir.join("worlds");
    for e in std::fs::read_dir(cached).into_iter().flatten().flatten() {
        let stem = e.file_name().to_string_lossy().into_owned();
        let lower = stem.to_ascii_lowercase();
        if !e.path().join("world.bin").is_file() || lower.contains("test") || lower.starts_with("gpe") || lower.ends_with(".baking") {
            continue;
        }
        let name = pretty_world_name(&stem);
        if !worlds.iter().any(|w| w.name == name) {
            worlds.push(WorldEntry { name, path: PathBuf::from(format!("{stem}.world")) });
        }
    }
    // Sunset Valley first: it's the base game's home town.
    worlds.sort_by_key(|w| (!w.name.contains("Sunset"), w.name.clone()));
    worlds
}

fn pretty_world_name(stem: &str) -> String {
    match stem {
        "AppaloosaPlains" => "Appaloosa Plains".into(),
        "IslaParadiso" => "Isla Paradiso".into(),
        other => other.into(),
    }
}
