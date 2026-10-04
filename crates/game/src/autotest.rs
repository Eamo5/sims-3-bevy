//! Command-line driven automation used for testing without a human at the keyboard:
//!   --world <name>          skip the menu and load this world
//!   --screenshot <path>     save a screenshot after the game has been running a while
//!   --shot-delay <secs>     seconds in-game before the screenshot (default 8)
//!   --cam x,z,dist,yaw,pitch  initial camera placement
//!   --exit-after-shot       quit once the screenshot is written
//!   --view-level <n>        floor of the house to view (PageUp / PageDown in play)

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
    pub portrait: bool,
    pub action: Option<String>,
    pub ui_flow: Option<String>,
    pub view_level: Option<u8>,
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
                "--do" => a.action = next,
                "--ui-flow" => a.ui_flow = next,
                "--view-level" => a.view_level = next.and_then(|s| s.parse().ok()),
                "--showroom" => {
                    a.showroom = next.and_then(|s| {
                        let mut it = s.split(',').filter_map(|x| x.parse().ok());
                        Some((it.next()?, it.next().unwrap_or(0)))
                    })
                }
                "--portrait" => {
                    a.portrait = true;
                    i += 1;
                    continue;
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
            .add_systems(Update, portrait_cam.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_action.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_view_level.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(PreUpdate, ui_flow.after(bevy::ui::UiSystems::Focus))
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

/// Applies `--cam` shortly after play starts (after the move-in camera placement).
fn apply_cam(args: Res<AutoArgs>, time: Res<Time>, mut since: Local<Option<f32>>, mut done: Local<bool>, mut q: Query<&mut SimsCamera>) {
    let Some(c) = args.cam else { return };
    if *done {
        return;
    }
    let t0 = *since.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - t0 < 1.0 {
        return;
    }
    if let Ok(mut cam) = q.single_mut() {
        cam.look_at(Vec3::new(c[0], 0.0, c[1]));
        cam.distance = c[2];
        cam.yaw = c[3];
        cam.pitch = c[4];
        *done = true;
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
            info!("autotest: {:.1} fps (frame {:.2} ms)", 1.0 / time.delta_secs().max(1e-4), time.delta_secs() * 1000.0);
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
    data: Res<crate::baked::Baked>,
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
    let keys: Vec<s3bake::Key> = data.0.catalog.iter().map(|c| c.objd).collect();
    let mut ctx = crate::objects::AssetCtx {
        baked: &data.0,
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

/// `--portrait`: keep the camera on the selected sim at head height.
fn portrait_cam(
    args: Res<AutoArgs>,
    sel: Query<&Transform, With<crate::sim::Selected>>,
    mut cam: Query<&mut SimsCamera>,
) {
    if !args.portrait {
        return;
    }
    if let (Ok(t), Ok(mut c)) = (sel.single(), cam.single_mut()) {
        let fwd = t.rotation * Vec3::Z;
        c.look_at(t.translation);
        c.height_offset = 1.25;
        c.distance = 2.6;
        c.pitch = 0.12;
        c.yaw = fwd.x.atan2(fwd.z) + 0.35;
    }
}

/// `--do <interaction>`: the selected sim performs this interaction on the first object offering it.
fn auto_action(
    args: Res<AutoArgs>,
    mut done: Local<bool>,
    mut sel: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>,
    objects: Query<(Entity, &crate::interact::GameObject)>,
) {
    let Some(name) = &args.action else { return };
    if *done {
        return;
    }
    let Ok(mut q) = sel.single_mut() else { return };
    for (e, o) in &objects {
        if let Some(i) = crate::interact::interactions_for(o.kind).iter().position(|d| d.name.eq_ignore_ascii_case(name)) {
            q.0.clear();
            q.push_player(crate::interact::Action::new(name.clone(), crate::interact::ActionKind::Object { target: e, def: i }, false));
            *done = true;
            return;
        }
    }
}

/// `--ui-flow <dir>`: clicks through the real menus (world → household → lot → move in),
/// saving a screenshot of each screen, then exits after a while in live mode.
#[allow(clippy::too_many_arguments)]
fn ui_flow(
    args: Res<AutoArgs>,
    time: Res<Time>,
    state: Res<State<AppState>>,
    play: Option<Res<State<crate::PlayMode>>>,
    mut commands: Commands,
    mut stage: Local<(u8, f32)>,
    mut menu: Query<(&mut Interaction, &crate::menu::MenuAction), (Without<crate::home::CasAction>, Without<crate::home::LotButton>, Without<crate::home::MoveInButton>)>,
    mut cas: Query<(&mut Interaction, &crate::home::CasAction), (Without<crate::home::LotButton>, Without<crate::home::MoveInButton>)>,
    mut lots: Query<(&mut Interaction, &crate::home::LotButton), Without<crate::home::MoveInButton>>,
    mut move_in: Query<&mut Interaction, With<crate::home::MoveInButton>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(dir) = &args.ui_flow else { return };
    let now = time.elapsed_secs();
    let since = now - stage.1;
    let shot = |commands: &mut Commands, name: &str| {
        let path = format!("{dir}/{name}.png");
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    };
    let advance = |stage: &mut (u8, f32)| {
        stage.0 += 1;
        stage.1 = now;
    };
    match (stage.0, state.get(), play.as_ref().map(|p| *p.get())) {
        (0, AppState::MainMenu, _) if since > 1.5 => {
            shot(&mut commands, "1_menu");
            advance(&mut stage);
        }
        (1, AppState::MainMenu, _) if since > 0.5 => {
            if let Some((mut i, _)) = menu.iter_mut().find(|(_, a)| matches!(a, crate::menu::MenuAction::PlayWorld(0))) {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (2, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2_household");
            advance(&mut stage);
        }
        (3, AppState::CreateHousehold, _) if since > 0.5 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Done)) {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (4, AppState::Loading, _) if since > 1.0 => {
            shot(&mut commands, "3_loading");
            advance(&mut stage);
        }
        (4, AppState::InGame, _) => advance(&mut stage),
        (5, AppState::InGame, Some(crate::PlayMode::ChooseLot)) if since > 3.0 => {
            shot(&mut commands, "4_choose_lot");
            if let Some((mut i, _)) = lots.iter_mut().next() {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (6, AppState::InGame, Some(crate::PlayMode::ChooseLot)) if since > 2.0 => {
            shot(&mut commands, "5_lot_selected");
            if let Ok(mut i) = move_in.single_mut() {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (7, AppState::InGame, Some(crate::PlayMode::Live)) if since > 10.0 => {
            shot(&mut commands, "6_live");
            advance(&mut stage);
        }
        (8, _, _) if since > 2.0 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn auto_view_level(args: Res<AutoArgs>, building: Option<ResMut<crate::building::ActiveBuilding>>, mut done: Local<bool>) {
    if let (Some(l), Some(mut b), false) = (args.view_level, building, *done) {
        b.view_level = l.clamp(1, b.top_level);
        *done = true;
    }
}
