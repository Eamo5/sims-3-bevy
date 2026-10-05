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
    pub premades: Option<Arc<s3bake::PremadesBaked>>,
}

#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub key: Key,
    pub name: String,
    pub price: i32,
    pub kind: crate::interact::ObjectKind,
    /// A door or archway (`Some(true)`) or a window (`Some(false)`), set into a wall.
    pub opening: Option<bool>,
    /// Its buy-mode tab.
    pub category: &'static str,
}

/// The buy-mode tab of a catalogue object: by what it does, else by the game's own grouping of
/// its script (`Sims3.Gameplay.Objects.<Group>.…`).
fn buy_category(kind: crate::interact::ObjectKind, script: &str) -> &'static str {
    let by_kind = kind.category();
    if by_kind != "Misc" {
        return by_kind;
    }
    let group = script.strip_prefix("Sims3.Gameplay.Objects.").and_then(|s| s.split('.').next()).unwrap_or("");
    match group {
        // Fountain jets only work set into a build-mode fountain pool.
        _ if script.contains("FountainJet") => "",
        _ if script.contains("SmokeDetector") || script.contains("BurglarAlarm") => "Electronics",
        "Appliances" => "Appliances",
        "Plumbing" => "Plumbing",
        "Beds" => "Beds",
        "Seating" => "Seating",
        "Tables" | "Counters" | "ShelvesStorage" => "Surfaces",
        "Electronics" => "Electronics",
        "HobbiesSkills" | "Entertainment" => "Hobbies",
        "Toys" | "Swingset" => "Kids",
        "Lighting" => "Lighting",
        "Decorations" => "Decor",
        "Environment" | "Flora" => "Outdoors",
        "Miscellaneous" => "Misc",
        _ => "",
    }
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
                opening: crate::building::is_opening(&c.script),
                category: buy_category(crate::interact::ObjectKind::from_script(&c.script, &c.instance_name), &c.script),
            })
            .collect();
        let index = entries.iter().enumerate().map(|(i, e)| (e.key, i)).collect();
        Self { entries, index }
    }

    pub fn by_key(&self, k: &Key) -> Option<&CatalogEntry> {
        self.index.get(k).map(|&i| &self.entries[i])
    }

    /// Buyable doors (or windows), cheapest first.
    pub fn openings(&self, doors: bool) -> Vec<&CatalogEntry> {
        let mut v: Vec<&CatalogEntry> = self.entries.iter().filter(|e| e.price > 0 && e.opening == Some(doors) && !e.name.is_empty()).collect();
        v.sort_by(|a, b| a.price.cmp(&b.price).then(a.name.cmp(&b.name)));
        v.dedup_by(|a, b| a.name == b.name);
        v
    }

    /// Buyable entries in a buy-mode category, cheapest first.
    pub fn in_category(&self, cat: &str) -> Vec<&CatalogEntry> {
        let mut v: Vec<&CatalogEntry> =
            self.entries.iter().filter(|e| e.price > 0 && e.category == cat && e.opening.is_none() && !e.name.is_empty()).collect();
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
    /// The road graph (for traffic).
    pub road_curves: Vec<[[f32; 2]; 4]>,
    pub road_intersections: Vec<[f32; 3]>,
    /// Ponds on lots.
    pub ponds: Vec<s3bake::PondBaked>,
}

impl WorldInfo {
    /// The water surface at a point, if it's on a pond.
    pub fn pond_at(&self, x: f32, z: f32) -> Option<f32> {
        self.ponds.iter().find_map(|p| {
            let l = self.lots.get(p.lot as usize)?;
            let (s, c) = l.rotation.sin_cos();
            let (dx, dz) = (x - l.corner[0], z - l.corner[2]);
            let (lx, lz) = (dx * c - dz * s, dx * s + dz * c);
            if lx < 0.0 || lz < 0.0 || lx >= (p.nx - 1) as f32 || lz >= (p.nz - 1) as f32 {
                return None;
            }
            let (ix, iz, nz) = (lx as usize, lz as usize, p.nz as usize);
            let level = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(a, b)| p.water[(ix + a) * nz + iz + b]).filter(|v| !v.is_nan()).fold(f32::NAN, f32::max);
            (!level.is_nan()).then_some(level)
        })
    }
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
    let members: Vec<crate::sim::Sim> = pending.as_ref().map(|p| p.members.clone()).unwrap_or_default();
    let playing: Option<u64> = pending.as_ref().and_then(|p| p.premade.as_ref()).map(|h| h.id);
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
        let need_sounds = !s3bake::sounds_ready(&root);
        let need_gamedata = !s3bake::gamedata_ready(&root);
        if need_global || need_world || need_clips || need_sounds || need_gamedata {
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
            if need_sounds {
                s3bake::bake_sounds(&root, &pkgs, &set_status)?;
            }
            if need_gamedata {
                s3bake::bake_gamedata(&root, &pkgs, &root_path, &set_status)?;
            }
            if need_world {
                pkgs.add(s3pkg::Package::open(&world_path).map_err(|e| e.to_string())?);
                s3bake::bake_world(&root, &pkgs, &world_path, &world_name, &set_status)?;
            }
        }
        if let Err(e) = s3bake::ensure_premades(&root, &world_path, &world_name) {
            warn!("premade households: {e}");
        }
        set_status("Loading converted assets…");
        let world: WorldBaked =
            s3bake::read_value(&root.world_dir(&world_name).join("world.bin")).map_err(|e| format!("world cache: {e}"))?;
        let premades = s3bake::load_premades(&root, &world_name).map(Arc::new);
        let baked = Arc::new(BakedData::open(root, Some(&world_name))?);
        set_status("Building terrain…");
        let terrain = terrain::build_terrain(&world);
        set_status("Placing the town…");
        let world_build = crate::world::build_world(&baked, &world);
        let roads = crate::roads::build_roads(&baked, &world);
        let catalog = Catalog::from_baked(&baked);
        set_status("Dressing your Sims…");
        let cas = crate::simbody::CasData::from_baked(&baked);
        // The town's own Sims stroll past and come to visit.
        let town: Vec<crate::sim::Sim> = premades
            .as_ref()
            .map(|p| crate::premade::TownPremades(p.clone()).others(playing).into_iter().map(crate::premade::to_sim).collect())
            .unwrap_or_default();
        let sims = crate::simbody::prepare_sims(&baked, &cas, &members, known.as_deref(), &town);
        let info = WorldInfo {
            lot_names: world.lots.iter().map(|l| l.display_name.clone()).collect(),
            lots: world.lots.iter().map(|l| l.info.clone()).collect(),
            heightmap: world.heightmap,
            sea_level: world.sea_level,
            buildings: world.buildings.into_iter().map(|b| (b.lot as usize, b)).collect(),
            road_curves: world.road_curves,
            road_intersections: world.road_intersections,
            ponds: world.ponds,
        };
        set_status("Done");
        Ok(LoadResult { baked, world: Arc::new(info), terrain, catalog, world_build, roads, cas, sims, premades })
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
            match r.premades {
                Some(p) => commands.insert_resource(crate::premade::TownPremades(p)),
                None => commands.remove_resource::<crate::premade::TownPremades>(),
            }
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
