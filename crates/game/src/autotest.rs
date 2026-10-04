//! Command-line driven automation used for testing without a human at the keyboard:
//!   --world <name>          skip the menu and load this world
//!   --screenshot <path>     save a screenshot after the game has been running a while
//!   --shot-delay <secs>     seconds in-game before the screenshot (default 8)
//!   --cam x,z,dist,yaw,pitch  initial camera placement
//!   --exit-after-shot       quit once the screenshot is written

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::AppState;
use crate::camera::{CameraStart, SimsCamera};
use crate::data::{SelectedWorld, WorldList};

#[derive(Resource, Default, Clone)]
pub struct AutoArgs {
    pub world: Option<String>,
    pub screenshot: Option<String>,
    pub shot_delay: f32,
    pub cam: Option<[f32; 5]>,
    pub exit_after: bool,
    pub showroom: Option<(usize, usize)>,
    pub lot: Option<String>,
}

impl AutoArgs {
    pub fn from_env() -> Self {
        let mut a = AutoArgs { shot_delay: 8.0, ..default() };
        let args: Vec<String> = std::env::args().collect();
        let mut i = 1;
        while i < args.len() {
            let next = args.get(i + 1).cloned();
            match args[i].as_str() {
                "--world" => a.world = next,
                "--screenshot" => a.screenshot = next,
                "--shot-delay" => a.shot_delay = next.and_then(|s| s.parse().ok()).unwrap_or(8.0),
                "--cam" => {
                    a.cam = next.and_then(|s| {
                        let v: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
                        (v.len() == 5).then(|| [v[0], v[1], v[2], v[3], v[4]])
                    })
                }
                "--lot" => a.lot = next,
                "--showroom" => {
                    a.showroom = next.and_then(|s| {
                        let mut it = s.split(',').filter_map(|x| x.parse().ok());
                        Some((it.next()?, it.next().unwrap_or(0)))
                    })
                }
                "--exit-after-shot" => {
                    a.exit_after = true;
                    i += 1;
                    continue;
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
            i += 2;
        }
        a
    }
}

pub struct AutoTestPlugin;

impl Plugin for AutoTestPlugin {
    fn build(&self, app: &mut App) {
        let args = AutoArgs::from_env();
        if let Some(c) = args.cam {
            app.insert_resource(CameraStart(Vec3::new(c[0], 0.0, c[1])));
        }
        app.insert_resource(args)
            .add_systems(Update, auto_pick_world.run_if(in_state(AppState::MainMenu)))
            .add_systems(Update, apply_cam.run_if(in_state(AppState::InGame)))
            .add_systems(Update, auto_screenshot.run_if(in_state(AppState::InGame)))
            .add_systems(OnEnter(AppState::InGame), showroom);
    }
}

fn auto_pick_world(
    args: Res<AutoArgs>,
    worlds: Res<WorldList>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Some(name) = &args.world else { return };
    *done = true;
    let lname = name.to_ascii_lowercase();
    if let Some(w) = worlds.0.iter().find(|w| w.name.to_ascii_lowercase().contains(&lname)) {
        commands.insert_resource(SelectedWorld(w.clone()));
        commands.insert_resource(crate::home::PendingHousehold::random());
        next.set(AppState::Loading);
    } else {
        warn!("--world {name}: no such world");
    }
}

fn apply_cam(args: Res<AutoArgs>, mut q: Query<&mut SimsCamera, Added<SimsCamera>>) {
    if let (Some(c), Ok(mut cam)) = (args.cam, q.single_mut()) {
        cam.distance = c[2];
        cam.yaw = c[3];
        cam.pitch = c[4];
    }
}

fn auto_screenshot(
    args: Res<AutoArgs>,
    time: Res<Time>,
    mut start: Local<Option<f32>>,
    mut state: Local<u8>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = &args.screenshot else { return };
    let t = time.elapsed_secs();
    let s = *start.get_or_insert(t);
    match *state {
        0 if t - s > args.shot_delay => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
            *state = 1;
        }
        1 if t - s > args.shot_delay + 2.0 => {
            *state = 2;
            if args.exit_after {
                exit.write(AppExit::Success);
            }
        }
        _ => {}
    }
}

/// Lays out catalog objects in a grid near the camera start, for visual checks.
fn showroom(
    args: Res<AutoArgs>,
    data: Res<crate::data::GameData>,
    world: Res<crate::loading::CurrentWorld>,
    start: Option<Res<CameraStart>>,
    mut assets: ResMut<crate::objects::ObjectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some((count, skip)) = args.showroom else { return };
    let origin = start.map(|s| s.0).unwrap_or(Vec3::new(1024.0, 0.0, 1024.0));
    let mut keys: Vec<_> = data.0.keys_of_type(s3pkg::types::OBJD).copied().collect();
    keys.sort();
    let mut ctx = crate::objects::AssetCtx {
        pkgs: &data.0,
        meshes: &mut meshes,
        images: &mut images,
        materials: &mut materials,
    };
    let cols = (count as f32).sqrt().ceil() as usize;
    let mut placed = 0;
    for k in keys.into_iter().skip(skip) {
        if placed >= count {
            break;
        }
        let parts = assets.object(&mut ctx, k);
        let Some((mn, mx)) = crate::objects::parts_bounds(&parts) else { continue };
        if (mx - mn).max_element() > 6.0 {
            continue;
        }
        let (gx, gz) = ((placed % cols) as f32, (placed / cols) as f32);
        let x = origin.x + gx * 3.0;
        let z = origin.z + gz * 3.0;
        let y = world.data.heightmap.sample(x, z);
        crate::objects::spawn_parts(&mut commands, &parts, Transform::from_xyz(x, y, z));
        placed += 1;
    }
    info!("showroom: placed {placed} objects");
}
