//! Loading screen. On first run (or when the cache is stale) it converts the installed game's
//! assets into the baked cache; afterwards everything loads straight from the cache.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use s3bake::{Heightmap, Key, LotInfo, WorldBaked};

use crate::AppState;
use crate::baked::{Baked, BakedData};
use crate::data::{InstallPath, SelectedWorld};
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
    pub baked: Arc<BakedData>,
    pub world: Arc<WorldInfo>,
    pub terrain: TerrainBuild,
    pub catalog: Catalog,
    pub world_build: crate::world::WorldBuild,
    pub roads: crate::roads::RoadBuild,
    pub cas: crate::simbody::CasData,
    pub sims: crate::simbody::PreparedSims,
}

#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub key: Key,
    pub name: String,
    pub price: i32,
    pub kind: crate::interact::ObjectKind,
}

/// Every object in the installed game, with buy-mode prices.
#[derive(Resource, Default)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    index: std::collections::HashMap<Key, usize>,
}

impl Catalog {
    pub fn from_baked(b: &BakedData) -> Self {
        let entries: Vec<CatalogEntry> = b
            .catalog
            .iter()
            .map(|c| CatalogEntry {
                key: c.objd,
                name: if c.name.is_empty() { c.instance_name.clone() } else { c.name.clone() },
                price: c.price,
                kind: crate::interact::ObjectKind::from_script(&c.script, &c.instance_name),
            })
            .collect();
        let index = entries.iter().enumerate().map(|(i, e)| (e.key, i)).collect();
        Self { entries, index }
    }

    pub fn by_key(&self, k: &Key) -> Option<&CatalogEntry> {
        self.index.get(k).map(|&i| &self.entries[i])
    }

    /// Buyable entries in a buy-mode category, cheapest first.
    pub fn in_category(&self, cat: &str) -> Vec<&CatalogEntry> {
        let mut v: Vec<&CatalogEntry> =
            self.entries.iter().filter(|e| e.price > 0 && e.kind.category() == cat && !e.name.is_empty()).collect();
        v.sort_by(|a, b| a.price.cmp(&b.price).then(a.name.cmp(&b.name)));
        v.dedup_by(|a, b| a.name == b.name);
        v
    }
}

/// Terrain heights, lots and water of the world being played.
pub struct WorldInfo {
    pub heightmap: Heightmap,
    pub lots: Vec<LotInfo>,
    pub lot_names: Vec<String>,
    pub sea_level: f32,
    /// Pre-built houses by lot index.
    pub buildings: std::collections::HashMap<usize, s3bake::LotBuildingBaked>,
}

/// The world currently being played.
#[derive(Resource, Clone)]
pub struct CurrentWorld {
    pub name: String,
    pub data: Arc<WorldInfo>,
}

#[derive(Resource)]
struct LoadTask {
    task: Task<Result<LoadResult, String>>,
    progress: Arc<Mutex<String>>,
}

#[derive(Component)]
struct ProgressText;

fn start_loading(
    mut commands: Commands,
    install: Res<InstallPath>,
    selected: Res<SelectedWorld>,
    pending: Option<Res<crate::home::PendingHousehold>>,
    save: Option<Res<crate::save::PendingLoad>>,
) {
    let members: Vec<crate::sim::Sim> = pending.map(|p| p.members.clone()).unwrap_or_default();
    let known: Option<Vec<crate::sim::Sim>> = save.map(|s| s.0.known_sims());
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
    let root_path = install.0.clone();
    let world_path = selected.0.path.clone();
    let world_name = world_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let prog = progress.clone();
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let set_status = |s: &str| *prog.lock().unwrap() = s.to_string();
        let root = s3bake::default_root();
        let need_global = root.global_manifest().is_none();
        let need_world = !root.world_ready(&world_name);
        let need_clips = !s3bake::clips_ready(&root);
        if need_global || need_world || need_clips {
            // One-time conversion of the installed game into GPU-ready assets.
            set_status("First run: reading the installed game (this conversion happens once)…");
            let mut pkgs = s3pkg::install::open_install(&root_path, |_| true);
            if pkgs.is_empty() {
                return Err(format!("No game packages found under {}", root_path.display()));
            }
            if need_global {
                s3bake::bake_global(&root, &pkgs, &root_path.to_string_lossy(), &set_status)?;
                s3bake::bake_music(&root, &root_path);
            }
            if need_clips && !need_global {
                s3bake::bake_clips(&root, &pkgs, &set_status)?;
            }
            if need_world {
                pkgs.add(s3pkg::Package::open(&world_path).map_err(|e| e.to_string())?);
                s3bake::bake_world(&root, &pkgs, &world_path, &world_name, &set_status)?;
            }
        }
        set_status("Loading converted assets…");
        let world: WorldBaked =
            s3bake::read_value(&root.world_dir(&world_name).join("world.bin")).map_err(|e| format!("world cache: {e}"))?;
        let baked = Arc::new(BakedData::open(root, Some(&world_name))?);
        set_status("Building terrain…");
        let terrain = terrain::build_terrain(&world);
        set_status("Placing the town…");
        let world_build = crate::world::build_world(&baked, &world);
        let roads = crate::roads::build_roads(&baked, &world);
        let catalog = Catalog::from_baked(&baked);
        set_status("Dressing your Sims…");
        let cas = crate::simbody::CasData::from_baked(&baked);
        let sims = crate::simbody::prepare_sims(&baked, &cas, &members, known.as_deref());
        let info = WorldInfo {
            lot_names: world.lots.iter().map(|l| l.display_name.clone()).collect(),
            lots: world.lots.iter().map(|l| l.info.clone()).collect(),
            heightmap: world.heightmap,
            sea_level: world.sea_level,
            buildings: world.buildings.into_iter().map(|b| (b.lot as usize, b)).collect(),
        };
        set_status("Done");
        Ok(LoadResult { baked, world: Arc::new(info), terrain, catalog, world_build, roads, cas, sims })
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
            commands.insert_resource(Baked(r.baked));
            commands.insert_resource(CurrentWorld { name: selected.0.name.clone(), data: r.world });
            commands.insert_resource(r.terrain);
            commands.insert_resource(r.catalog);
            commands.insert_resource(r.world_build);
            commands.insert_resource(r.roads);
            commands.insert_resource(r.cas);
            commands.insert_resource(r.sims);
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
