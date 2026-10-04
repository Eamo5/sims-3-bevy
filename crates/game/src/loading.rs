//! Loading screen: opens the game's packages and the chosen world on a worker thread.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use s3formats::world::WorldData;
use s3pkg::{Package, PackageSet};

use crate::AppState;
use crate::data::{GameData, InstallPath, SelectedWorld};
use crate::menu::{PLUMBOB_GREEN, text};
use crate::terrain::{self, TerrainBuild};

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Loading), start_loading)
            .add_systems(Update, poll_loading.run_if(in_state(AppState::Loading)));
    }
}

/// Everything produced by the loader thread.
pub struct LoadResult {
    pub packages: Arc<PackageSet>,
    pub world_pkg: Arc<Package>,
    pub world: Arc<WorldData>,
    pub terrain: TerrainBuild,
    pub strings: Strings,
    pub catalog: Catalog,
    pub world_build: crate::world::WorldBuild,
}

/// English localised strings keyed by FNV64.
#[derive(Resource, Default)]
pub struct Strings(pub std::collections::HashMap<u64, String>);

#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub key: s3pkg::ResourceKey,
    pub name: String,
    pub price: i32,
    pub kind: crate::interact::ObjectKind,
}

/// Every buyable object in the installed game.
#[derive(Resource, Default)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    index: std::collections::HashMap<s3pkg::ResourceKey, usize>,
}

impl Catalog {
    pub fn build(pkgs: &PackageSet, strings: &Strings) -> Self {
        use s3formats::object::{objd_objk, parse_objd};
        let mut keys: Vec<_> = pkgs.keys_of_type(s3pkg::types::OBJD).copied().collect();
        keys.sort();
        let mut entries = Vec::new();
        for k in keys {
            let Some(d) = pkgs.read(&k) else { continue };
            let Ok(info) = parse_objd(&d) else { continue };
            let script = objd_objk(pkgs, &d).and_then(|o| o.script_class).unwrap_or_default();
            let kind = crate::interact::ObjectKind::from_script(&script, &info.name);
            let name = strings.0.get(&info.name_guid).cloned().unwrap_or_else(|| info.instance_name.clone());
            entries.push(CatalogEntry {
                key: k,
                name,
                price: if info.show_in_catalog { info.price as i32 } else { -1 },
                kind,
            });
        }
        let index = entries.iter().enumerate().map(|(i, e)| (e.key, i)).collect();
        Self { entries, index }
    }

    pub fn by_key(&self, k: &s3pkg::ResourceKey) -> Option<&CatalogEntry> {
        self.index.get(k).map(|&i| &self.entries[i])
    }

    /// Buyable entries in a buy-mode category, cheapest first.
    pub fn in_category(&self, cat: &str) -> Vec<&CatalogEntry> {
        let mut v: Vec<&CatalogEntry> = self
            .entries
            .iter()
            .filter(|e| e.price > 0 && e.kind.category() == cat && !e.name.is_empty())
            .collect();
        v.sort_by(|a, b| a.price.cmp(&b.price).then(a.name.cmp(&b.name)));
        v.dedup_by(|a, b| a.name == b.name);
        v
    }
}

#[derive(Resource)]
struct LoadTask {
    task: Task<Result<LoadResult, String>>,
    progress: Arc<Mutex<String>>,
}

#[derive(Component)]
struct ProgressText;

/// The world currently being played.
#[derive(Resource, Clone)]
pub struct CurrentWorld {
    pub name: String,
    pub world_pkg: Arc<Package>,
    pub data: Arc<WorldData>,
}

fn start_loading(mut commands: Commands, install: Res<InstallPath>, selected: Res<SelectedWorld>) {
    commands.spawn((Camera2d, DespawnOnExit(AppState::Loading)));
    commands
        .spawn((
            DespawnOnExit(AppState::Loading),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.16, 0.30)),
        ))
        .with_children(|p| {
            p.spawn(text(format!("Loading {}…", selected.0.name), 48.0, Color::WHITE));
            p.spawn((text("Starting", 22.0, PLUMBOB_GREEN), ProgressText));
        });

    let progress = Arc::new(Mutex::new(String::from("Starting")));
    let root = install.0.clone();
    let world_path = selected.0.path.clone();
    let prog = progress.clone();
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let set_status = |s: &str| *prog.lock().unwrap() = s.to_string();
        set_status("Reading game packages (base game + expansions)…");
        let mut set = s3pkg::install::open_install(&root, |_| true);
        if set.is_empty() {
            return Err(format!("No game packages found under {}", root.display()));
        }
        set_status("Opening world file…");
        // The world's own resources (lot imposters, their textures) join the package stack.
        set.add(Package::open(&world_path).map_err(|e| e.to_string())?);
        let packages = Arc::new(set);
        let world_pkg = Arc::new(Package::open(&world_path).map_err(|e| e.to_string())?);
        set_status("Reading terrain…");
        let world = Arc::new(WorldData::load(&world_pkg)?);
        set_status("Building terrain meshes and textures…");
        let terrain = terrain::build_terrain(&world, &packages);
        set_status("Reading string tables…");
        let strings = Strings(s3formats::stbl::load_english(&packages));
        set_status("Building the buy catalog…");
        let catalog = Catalog::build(&packages, &strings);
        let world_build = crate::world::build_world(&packages, &world_pkg, &world, &set_status);
        set_status("Done");
        Ok(LoadResult { packages, world_pkg, world, terrain, strings, catalog, world_build })
    });
    commands.insert_resource(LoadTask { task, progress });
}

fn poll_loading(
    mut commands: Commands,
    task: Option<ResMut<LoadTask>>,
    selected: Res<SelectedWorld>,
    mut text_q: Query<&mut Text, With<ProgressText>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Some(mut task) = task else { return };
    if let Ok(mut t) = text_q.single_mut() {
        let s = task.progress.lock().unwrap().clone();
        if t.0 != s {
            t.0 = s;
        }
    }
    let Some(result) = block_on(poll_once(&mut task.task)) else { return };
    commands.remove_resource::<LoadTask>();
    match result {
        Ok(r) => {
            commands.insert_resource(GameData(r.packages));
            commands.insert_resource(CurrentWorld {
                name: selected.0.name.clone(),
                world_pkg: r.world_pkg,
                data: r.world,
            });
            commands.insert_resource(r.terrain);
            commands.insert_resource(r.strings);
            commands.insert_resource(r.catalog);
            commands.insert_resource(r.world_build);
            next.set(AppState::InGame);
        }
        Err(e) => {
            error!("Loading failed: {e}");
            if let Ok(mut t) = text_q.single_mut() {
                t.0 = format!("Loading failed: {e}");
            }
        }
    }
}
