//! Discovers a Sims 3 installation's packages and their override priority by
//! reading the game's `Resource.cfg` files.

use std::path::{Path, PathBuf};

use crate::{Package, PackageSet};

#[derive(Debug, Clone)]
pub struct PackageRef {
    pub priority: i32,
    pub path: PathBuf,
}

/// Matches a single path component against a pattern with `*` wildcards.
fn wildcard(pat: &str, name: &str) -> bool {
    let p = pat.to_ascii_lowercase();
    let n = name.to_ascii_lowercase();
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == n;
    }
    let mut rest = n.as_str();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !rest.starts_with(part) {
                return false;
            }
            rest = &rest[part.len()..];
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else if let Some(idx) = rest.find(part) {
            rest = &rest[idx + part.len()..];
        } else {
            return false;
        }
    }
    true
}

fn glob(base: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut current = vec![base.to_path_buf()];
    for comp in pattern.split(['/', '\\']) {
        let mut next = Vec::new();
        for dir in &current {
            if comp == ".." || comp == "." || !comp.contains('*') {
                next.push(dir.join(comp));
                continue;
            }
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    if wildcard(comp, &e.file_name().to_string_lossy()) {
                        next.push(e.path());
                    }
                }
            }
        }
        current = next;
    }
    current.retain(|p| p.is_file());
    current.sort();
    current
}

/// Parses a Resource.cfg, returning the packed files it references.
pub fn parse_resource_cfg(cfg: &Path) -> Vec<PackageRef> {
    let Ok(text) = std::fs::read_to_string(cfg) else {
        return Vec::new();
    };
    let base = cfg.parent().unwrap_or(Path::new("."));
    let mut priority = 0;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let mut it = line.split_whitespace();
        match it.next() {
            Some("Priority") => {
                priority = it.next().and_then(|s| s.parse().ok()).unwrap_or(priority);
            }
            Some("PackedFile") => {
                if let Some(pat) = it.next() {
                    for path in glob(base, pat) {
                        out.push(PackageRef { priority, path });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The roots of the base game and every installed pack (EPn/SPn folders).
pub fn product_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(root) {
        let mut packs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().unwrap().to_string_lossy().to_ascii_uppercase();
                (n.starts_with("EP") || n.starts_with("SP")) && p.join("GameData").is_dir()
            })
            .collect();
        packs.sort();
        roots.extend(packs);
    }
    roots
}

/// Collects every package of the installation sorted from lowest to highest priority.
pub fn discover_packages(root: &Path) -> Vec<PackageRef> {
    let mut refs = Vec::new();
    for pr in product_roots(root) {
        let shared = pr.join("GameData").join("Shared");
        refs.extend(parse_resource_cfg(&shared.join("Resource.cfg")));
        let delta = shared.join("DeltaPackages");
        if let Ok(rd) = std::fs::read_dir(&delta) {
            let mut cfgs: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cfg")))
                .collect();
            cfgs.sort();
            for c in cfgs {
                refs.extend(parse_resource_cfg(&c));
            }
        }
    }
    // Thumbnails are irrelevant for us and may be absent.
    refs.retain(|r| {
        !r.path
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains("thumbnails")
    });
    refs.sort_by_key(|r| r.priority);
    // Patch packages (DeltaPackages/pNN) are copied into several pack folders; keep one copy.
    let mut seen = std::collections::HashSet::new();
    refs.retain(|r| {
        let name = r.path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        seen.insert((r.priority, name))
    });
    refs
}

/// Lists all `.world` files shipped with the base game and packs.
pub fn discover_worlds(root: &Path) -> Vec<PathBuf> {
    let mut worlds = Vec::new();
    for pr in product_roots(root) {
        let dir = pr.join("GameData/Shared/NonPackaged/Worlds");
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("world")) {
                    worlds.push(p);
                }
            }
        }
    }
    worlds
}

/// Opens every package of the install into a single override-aware set.
pub fn open_install(root: &Path, mut filter: impl FnMut(&PackageRef) -> bool) -> PackageSet {
    let mut set = PackageSet::new();
    for r in discover_packages(root) {
        if !filter(&r) {
            continue;
        }
        match Package::open(&r.path) {
            Ok(p) => set.add(p),
            Err(e) => eprintln!("warning: failed to open {}: {e}", r.path.display()),
        }
    }
    set
}
