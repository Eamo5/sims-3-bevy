//! Command-line driven automation used for testing without a human at the keyboard:
//!   --world <name>          skip the menu and load this world
//!   --screenshot <path>     save a screenshot after the game has been running a while
//!   --shot-delay <secs>     seconds in-game before the screenshot (default 8)
//!   --cam x,z,dist,yaw,pitch  initial camera placement
//!   --exit-after-shot       quit once the screenshot is written
//!   --view-level <n>        floor of the house to view (PageUp / PageDown in play)
//!   --speed <0-3>           game speed once playing
//!   --hour <h>              start the day at this hour
//!   --save-at <secs>        save the game after this long in play
//!   --load <n>              load the n-th most recent save from the main menu

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
    /// `--place <script class part>`: put that catalog object beside the selected Sim.
    pub place: Option<String>,
    /// `--paint`: repaper the house's indoor walls and recover its ground floor.
    pub paint: bool,
    /// `--build <x,z,w,d>`: build a room there (lot tiles) with a door and a window.
    pub build: Option<[i32; 4]>,
    /// `--knock <x0,z0,x1,z1>`: knock down the walls along that grid line.
    pub knock: Option<[i32; 4]>,
    /// `--follow`: the camera keeps the selected Sim in view.
    pub follow: bool,
    /// `--home-after <seconds>`: the selected Sim, out on a community lot, heads home.
    pub home_after: Option<f32>,
    /// `--use <ObjectKind>`: once out on a lot (or at once, at home), the selected Sim uses the
    /// nearest object of that kind there.
    pub use_kind: Option<String>,
    /// `--storey`: after `--build`, stairs up inside the room, a floor and room above, and the
    /// selected Sim sent upstairs.
    pub storey: bool,
    /// `--relations`: open the Relationships panel.
    pub relations: bool,
    /// `--balloon <kind>:<icon>[:<axis>]`: keep showing this balloon over the selected Sim
    /// (kind thought / speech / dream; axis 1 like, 2 dislike).
    pub balloon: Option<String>,
    /// `--family <name>`: play this town family.
    pub family: Option<String>,
    /// `--select <first name>`: select this household member.
    pub select: Option<String>,
    pub ui_flow: Option<String>,
    pub view_level: Option<u8>,
    pub speed: Option<usize>,
    pub hour: Option<f64>,
    pub save_at: Option<f32>,
    pub load: Option<usize>,
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
                "--place" => a.place = next,
                "--build" | "--knock" => {
                    let v: Vec<i32> = next.as_deref().unwrap_or("").split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    let v = (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]]);
                    if args[i] == "--build" {
                        a.build = v;
                    } else {
                        a.knock = v;
                    }
                }
                "--balloon" => a.balloon = next,
                "--family" => a.family = next,
                "--select" => a.select = next,
                "--ui-flow" => a.ui_flow = next,
                "--view-level" => a.view_level = next.and_then(|s| s.parse().ok()),
                "--speed" => a.speed = next.and_then(|s| s.parse().ok()),
                "--hour" => a.hour = next.and_then(|s| s.parse().ok()),
                "--save-at" => a.save_at = next.and_then(|s| s.parse().ok()),
                "--load" => a.load = next.and_then(|s| s.parse().ok()),
                "--home-after" => a.home_after = next.and_then(|s| s.parse().ok()),
                "--use" => a.use_kind = next,
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
                "--paint" => {
                    a.paint = true;
                    i += 1;
                    continue;
                }
                "--follow" => {
                    a.follow = true;
                    i += 1;
                    continue;
                }
                "--storey" => {
                    a.storey = true;
                    i += 1;
                    continue;
                }
                "--relations" => {
                    a.relations = true;
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
            .add_systems(Update, apply_cam.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_screenshot.run_if(in_state(AppState::InGame)))
            .add_systems(Update, portrait_cam.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_action.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_place.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_paint.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_build.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_balloon.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(
                Update,
                (|mut commands: Commands, args: Res<AutoArgs>, mut p: ResMut<crate::relations::RelationsPanel>, mut j: ResMut<crate::collecting::JournalPanel>, mut done: Local<bool>, time: Res<Time>| {
                    if args.relations && !*done {
                        *done = true;
                        p.open = true;
                    }
                    // SIMTAB=<Skills|Career|Simology>: that tab of the Sim panel.
                    if let Ok(t) = std::env::var("SIMTAB") {
                        commands.queue(move |w: &mut World| {
                            let tab = crate::simpanel::SimTab::ALL.into_iter().find(|x| x.label().eq_ignore_ascii_case(&t)).unwrap_or_default();
                            if *w.resource::<crate::simpanel::SimTab>() != tab {
                                *w.resource_mut::<crate::simpanel::SimTab>() = tab;
                            }
                        });
                    }
                    // BUYTAB=<category>: buy mode open on that tab.
                    if let Ok(t) = std::env::var("BUYTAB")
                        && time.elapsed_secs() > 6.0
                    {
                        commands.queue(move |w: &mut World| {
                            if let Some(i) = crate::buy::CATEGORIES.iter().position(|c| c.eq_ignore_ascii_case(&t)) {
                                let mut b = w.resource_mut::<crate::buy::BuyMode>();
                                if !b.active || b.category != i {
                                    b.show(i);
                                }
                            }
                        });
                    }
                    // JOURNAL=<seconds>: the collection journal opens then.
                    if let Some(t) = std::env::var("JOURNAL").ok().and_then(|t| t.parse::<f32>().ok())
                        && time.elapsed_secs() > t
                        && !j.open
                    {
                        j.open = true;
                    }
                })
                .run_if(in_state(crate::PlayMode::Live)),
            )
            .add_systems(Update, auto_view_level.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(
                Update,
                (|args: Res<AutoArgs>,
                  mut done: Local<bool>,
                  time: Res<Time>,
                  mut sel: Query<(&mut crate::interact::ActionQueue, &Transform, Option<&crate::visit::OnLot>), With<crate::sim::Selected>>,
                  objects: Query<(Entity, &crate::interact::GameObject, &Transform, Option<&crate::visit::LotObject>)>| {
                    let Some(want) = &args.use_kind else { return };
                    if *done || time.elapsed_secs() < 8.0 {
                        return;
                    }
                    let Ok((mut q, tf, on)) = sel.single_mut() else { return };
                    if args.action.as_deref().is_some_and(|a| a.starts_with("Visit:")) && on.is_none() {
                        return;
                    }
                    let lot = on.map(|l| l.0);
                    let want = want.to_ascii_lowercase();
                    let best = objects
                        .iter()
                        .filter(|(_, o, _, l)| format!("{:?}", o.kind).to_ascii_lowercase() == want && l.map(|l| l.0) == lot)
                        .min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
                    if let Some((e, o, ..)) = best
                        && let Some(d) = crate::interact::interactions_for(o.kind).first()
                    {
                        *done = true;
                        info!("use test: {} on the {} ({e:?})", d.name, o.name);
                        q.0.clear();
                        q.push_player(crate::interact::Action::new(d.name, crate::interact::ActionKind::Object { target: e, def: 0 }, false));
                    }
                })
                .run_if(in_state(crate::PlayMode::Live)),
            )
            .add_systems(
                Update,
                (|args: Res<AutoArgs>,
                  time: Res<Time>,
                  mut since: Local<Option<f32>>,
                  mut done: Local<bool>,
                  mut sel: Query<&mut crate::interact::ActionQueue, (With<crate::sim::Selected>, With<crate::visit::OnLot>)>| {
                    let Some(after) = args.home_after else { return };
                    let t0 = *since.get_or_insert(time.elapsed_secs());
                    if *done || time.elapsed_secs() - t0 < after {
                        return;
                    }
                    if let Ok(mut q) = sel.single_mut() {
                        *done = true;
                        q.0.clear();
                        q.push_player(crate::interact::Action::new("Go Home", crate::interact::ActionKind::GoHomeFromLot, false));
                    }
                })
                .run_if(in_state(crate::PlayMode::Live)),
            )
            .add_systems(
                Update,
                (|mut commands: Commands,
                  args: Res<AutoArgs>,
                  sel: Query<(&Transform, &crate::interact::ActionQueue, Option<&crate::visit::OnLot>), With<crate::sim::Selected>>,
                  mut cam: Query<&mut SimsCamera>,
                  mut last: Local<f32>,
                  time: Res<Time>| {
                    if !args.follow || time.elapsed_secs() - *last < 2.0 {
                        return;
                    }
                    if *last == 0.0 {
                        commands.insert_resource(crate::opportunities::AutoDecline);
                    }
                    *last = time.elapsed_secs();
                    if let (Ok((tf, q, on)), Ok(mut c)) = (sel.single(), cam.single_mut()) {
                        c.look_at(tf.translation);
                        if let Some(d) = std::env::var("FOLLOW_DIST").ok().and_then(|d| d.parse::<f32>().ok()) {
                            c.distance = d;
                        }
                        info!(
                            "follow: at {:.1},{:.1},{:.1} on lot {:?} doing {:?}",
                            tf.translation.x,
                            tf.translation.y,
                            tf.translation.z,
                            on.map(|l| l.0),
                            q.0.front().map(|a| (&a.label, a.phase))
                        );
                    }
                })
                .run_if(in_state(crate::PlayMode::Live)),
            )
            .add_systems(Update, auto_speed.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_save.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_load.run_if(in_state(AppState::MainMenu)))
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
        let mut pending = crate::home::PendingHousehold::random();
        if let Some(fam) = &args.family {
            let root = s3bake::default_root();
            let stem = w.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let _ = s3bake::ensure_premades(&root, &w.path, &stem);
            let want = fam.to_ascii_lowercase();
            match s3bake::load_premades(&root, &stem).and_then(|p| p.playable().find(|h| h.name.to_ascii_lowercase().contains(&want)).cloned()) {
                Some(h) => {
                    pending.members = h.members.iter().map(crate::premade::to_sim).collect();
                    pending.last_name = h.name.clone();
                    pending.premade = Some(h);
                }
                None => warn!("--family {fam}: no such family in {}", w.name),
            }
        }
        commands.insert_resource(pending);
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
    // (Held for the first seconds of play, over the move-in camera.)
    let t0 = *since.get_or_insert(time.elapsed_secs());
    if let Ok(mut cam) = q.single_mut() {
        cam.look_at(Vec3::new(c[0], 0.0, c[1]));
        cam.distance = c[2];
        cam.yaw = c[3];
        cam.pitch = c[4];
        *done = time.elapsed_secs() - t0 > 6.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn auto_screenshot(
    args: Res<AutoArgs>,
    time: Res<Time>,
    mut start: Local<Option<f32>>,
    mut state: Local<u8>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    sims: Query<(Entity, &crate::sim::Sim, &Visibility, &GlobalTransform)>,
    children: Query<&Children>,
    vis: Query<(&InheritedVisibility, Option<&Name>, Has<Mesh3d>)>,
) {
    let Some(path) = &args.screenshot else { return };
    let t = time.elapsed_secs();
    let s = *start.get_or_insert(t);
    match *state {
        0 if t - s > args.shot_delay => {
            info!("autotest: {:.1} fps (frame {:.2} ms)", 1.0 / time.delta_secs().max(1e-4), time.delta_secs() * 1000.0);
            // Hidden Sims must not leave visible parts behind.
            for (e, sim, v, tf) in &sims {
                if *v != Visibility::Hidden {
                    continue;
                }
                let leaked = children.iter_descendants(e).filter(|c| vis.get(*c).is_ok_and(|(iv, _, mesh)| mesh && iv.get())).count();
                if leaked > 0 {
                    warn!("autotest: hidden {} at {:?} still shows {leaked} meshes", sim.full_name(), tf.translation());
                }
            }
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
    mut commands: Commands,
    args: Res<AutoArgs>,
    sel: Query<&Transform, With<crate::sim::Selected>>,
    members: Query<(Entity, &crate::sim::Sim, Has<crate::sim::Selected>), With<crate::sim::HouseholdMember>>,
    mut cam: Query<&mut SimsCamera>,
) {
    if let Some(want) = &args.select
        && let Some((e, _, is_sel)) = members
            .iter()
            .find(|(_, s, _)| s.first.eq_ignore_ascii_case(want) || (want == "@baby" && s.age == crate::sim::Age::Baby))
    {
        for (o, _, selected) in &members {
            if selected && o != e {
                commands.entity(o).remove::<crate::sim::Selected>();
            }
        }
        if !is_sel {
            commands.entity(e).insert(crate::sim::Selected);
        }
    }
    if !args.portrait {
        return;
    }
    if let (Ok(t), Ok(mut c)) = (sel.single(), cam.single_mut()) {
        let fwd = t.rotation * Vec3::Z;
        c.look_at(t.translation);
        c.height_offset = std::env::var("PORTRAIT_HEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(1.25);
        c.distance = std::env::var("PORTRAIT_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(2.6);
        c.pitch = std::env::var("PORTRAIT_PITCH").ok().and_then(|v| v.parse().ok()).unwrap_or(0.12);
        c.yaw = fwd.x.atan2(fwd.z) + std::env::var("PORTRAIT_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(0.35);
    }
}

/// `--balloon`: the balloon again every three seconds.
fn auto_balloon(
    args: Res<AutoArgs>,
    time: Res<Time>,
    mut next: Local<f32>,
    sel: Query<Entity, With<crate::sim::Selected>>,
    members: Query<Entity, With<crate::sim::HouseholdMember>>,
    mut commands: Commands,
) {
    let Some(spec) = &args.balloon else { return };
    if time.elapsed_secs() < *next {
        return;
    }
    let Ok(e) = sel.single() else { return };
    *next = time.elapsed_secs() + 3.0;
    let mut parts = spec.split(':');
    let kind = match parts.next() {
        Some("speech") => crate::balloons::BalloonKind::Speech,
        Some("dream") => crate::balloons::BalloonKind::Dream,
        _ => crate::balloons::BalloonKind::Thought,
    };
    let mut icon = parts.next().unwrap_or("balloon_question").to_string();
    // "@other": another household member's picture.
    if icon == "@other"
        && let Some(o) = members.iter().find(|m| *m != e)
    {
        icon = format!("@portrait:{}", o.to_bits());
    }
    let axis = parts.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    commands.entity(e).insert(crate::balloons::BalloonRequest { kind, icon, axis });
}

/// `--paint`: once the house is in, every indoor wall side gets the dearest wallpaper and the
/// ground floor the dearest flooring, through the same repainting the build tool uses.
#[allow(clippy::too_many_arguments)]
fn auto_paint(
    args: Res<AutoArgs>,
    mut done: Local<bool>,
    time: Res<Time>,
    mut since: Local<Option<f32>>,
    mut commands: Commands,
    ui: Option<Res<crate::icons::GameUi>>,
    mut building: Option<ResMut<crate::building::ActiveBuilding>>,
    (data, mut assets): (Res<crate::baked::Baked>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    if !args.paint || *done {
        return;
    }
    let t0 = *since.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - t0 < 3.0 {
        return;
    }
    let (Some(ui), Some(b)) = (ui, building.as_deref_mut()) else { return };
    *done = true;
    let pick = |floor: bool| ui.data.patterns.iter().filter(|p| p.floor == floor).max_by_key(|p| p.price).map(|p| p.texture);
    let (Some(wall_tex), Some(floor_tex)) = (pick(false), pick(true)) else { return };
    let mut ops = Vec::new();
    for (i, w) in b.data.walls.iter().enumerate() {
        for (side, kind) in [(0u8, w.left), (1, w.right)] {
            if kind != s3bake::ROOM_OUTSIDE {
                ops.push(crate::building::PaintOp::Wall { wall: i as u32, side, texture: wall_tex });
            }
        }
    }
    for f in b.data.floors.iter().filter(|f| f.level == 1) {
        ops.push(crate::building::PaintOp::Floor { level: 1, x: f.x, z: f.z, texture: floor_tex });
    }
    info!("paint test: {} repaintings", ops.len());
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
    commands.insert_resource(crate::building::LotPaint(ops));
}

/// `--build`: a room built with the room tool, a door put in its front wall and a window in its
/// side (through the same snapping as the pointer), then build mode left open on the tools.
#[allow(clippy::too_many_arguments)]
fn auto_build(
    args: Res<AutoArgs>,
    mut stage: Local<u8>,
    time: Res<Time>,
    mut since: Local<Option<f32>>,
    mut commands: Commands,
    mut building: Option<ResMut<crate::building::ActiveBuilding>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
    (mut log, mut grid, mut buy): (Option<ResMut<crate::building::LotPaint>>, Option<ResMut<crate::nav::NavGrid>>, ResMut<crate::buy::BuyMode>),
    (objects, mut removed, mut household, mut notes): (
        Query<(&crate::interact::GameObject, &Transform, Has<crate::save::Bought>)>,
        ResMut<crate::save::RemovedLotObjects>,
        Option<ResMut<crate::interact::Household>>,
        ResMut<crate::interact::Notifications>,
    ),
    mut sel: Query<(&mut crate::interact::ActionQueue, &Transform, &crate::nav::Floor, Option<&crate::nav::PathFollow>), With<crate::sim::Selected>>,
    mut last_log: Local<f32>,
) {
    if args.build.is_none() && args.knock.is_none() {
        return;
    }
    let [x, z, w, d] = args.build.unwrap_or([0, 0, 0, 0]);
    let t0 = *since.get_or_insert(time.elapsed_secs());
    let Some(b) = building.as_deref_mut() else { return };
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    let mut apply = |commands: &mut Commands, assets: &mut crate::objects::ObjectAssets, b: &mut crate::building::ActiveBuilding, ctx: &mut crate::objects::AssetCtx, ops: Vec<crate::building::PaintOp>| {
        crate::building::repaint(commands, b, assets, ctx, &ops, &mut faces);
        match log.as_mut() {
            Some(l) => l.0.extend(ops),
            None => commands.insert_resource(crate::building::LotPaint(ops)),
        }
    };
    match *stage {
        0 if time.elapsed_secs() - t0 > 3.0 && args.build.is_none() => {
            let long: Vec<String> = b
                .data
                .walls
                .iter()
                .filter(|w| Vec2::from(w.a).distance(Vec2::from(w.b)) > 1.05)
                .take(6)
                .map(|w| format!("({:.0},{:.0})-({:.0},{:.0})", w.a[0], w.a[1], w.b[0], w.b[1]))
                .collect();
            info!("build test: {} walls; long ones {}", b.data.walls.len(), long.join(" "));
            let [x0, z0, x1, z1] = args.knock.unwrap();
            let (ops, _) = crate::build::plan(b, crate::build::BuildTool::Sledgehammer, true, b.view_level, IVec2::new(x0, z0), IVec2::new(x1, z1));
            info!("build test: knock-down — {} ops: {:?}", ops.len(), ops);
            let c = b.world((x0 + x1) as f32 * 0.5, (z0 + z1) as f32 * 0.5, 0.0);
            info!("build test: knocked at {:.1},{:.1}", c.x, c.z);
            apply(&mut commands, &mut assets, b, &mut ctx, ops);
            *stage = 3;
        }
        0 if time.elapsed_secs() - t0 > 3.0 => {
            let (ops, cost) = crate::build::plan(b, crate::build::BuildTool::Room, false, b.view_level, IVec2::new(x, z), IVec2::new(x + w, z + d));
            let c = b.world(x as f32 + w as f32 * 0.5, z as f32 + d as f32 * 0.5, 0.0);
            info!("build test: room {}x{} at {:.1},{:.1} — {} ops, §{cost}", w, d, c.x, c.z, ops.len());
            apply(&mut commands, &mut assets, b, &mut ctx, ops);
            *stage = 1;
        }
        1 if time.elapsed_secs() - t0 > 4.0 => {
            // A door in the middle of the front wall (z = start), a window in the left wall.
            let y = b.levels[b.view_level as usize];
            let down = |lx: f32, lz: f32, b: &crate::building::ActiveBuilding| Ray3d::new(b.world(lx, lz, y + 40.0), Dir3::NEG_Y);
            let cheapest = |doors: bool| catalog.openings(doors).into_iter().find(|e| e.name.len() > 2).map(|e| e.key);
            for (doors, lx, lz) in [(true, x as f32 + w as f32 * 0.5, z as f32 - 0.3), (false, x as f32 - 0.3, z as f32 + d as f32 * 0.5)] {
                let Some(key) = cheapest(doors) else { continue };
                let tiles = crate::objects::parts_bounds(&assets.object(&mut ctx, key)).map_or(1, |(mn, mx)| ((mx.x - mn.x).round() as u32).max(1));
                let Some((pos, rot, ops)) = crate::build::snap_to_wall(b, down(lx, lz, b), tiles) else {
                    warn!("build test: no wall for the {}", if doors { "door" } else { "window" });
                    continue;
                };
                if !ops.is_empty() {
                    apply(&mut commands, &mut assets, b, &mut ctx, ops);
                }
                if let Some(o) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, key, pos, rot) {
                    commands.entity(o.entity).insert(crate::save::Bought);
                }
            }
            if let Some(g) = grid.as_mut() {
                g.dirty = true;
            }
            buy.show(crate::buy::BUILD_TAB);
            buy.tool = Some(crate::build::BuildTool::Wall);
            *stage = 2;
        }
        3 if time.elapsed_secs() - t0 > 6.0 => {
            *stage = 4;
            if !args.storey {
                return;
            }
            let level = b.view_level;
            match crate::build::plan_stairs(b, level, IVec2::new(x + 1, z + 1), 0, false) {
                Ok(ops) => {
                    info!("build test: stairs — {} ops", ops.len());
                    apply(&mut commands, &mut assets, b, &mut ctx, ops);
                }
                Err(why) => warn!("build test: stairs: {why}"),
            }
            let (ops, cost) = crate::build::plan(b, crate::build::BuildTool::Floor, false, level + 1, IVec2::new(x, z), IVec2::new(x + w - 1, z + d - 1));
            info!("build test: upstairs floor — {} tiles, §{cost}", ops.len());
            apply(&mut commands, &mut assets, b, &mut ctx, ops);
            let (ops, cost) = crate::build::plan(b, crate::build::BuildTool::Room, false, level + 1, IVec2::new(x, z), IVec2::new(x + w, z + d));
            info!("build test: upstairs room — {} ops, §{cost}", ops.len());
            apply(&mut commands, &mut assets, b, &mut ctx, ops);
            if let Some(g) = grid.as_mut() {
                g.dirty = true;
            }
            let to = b.world(x as f32 + w as f32 - 1.5, z as f32 + d as f32 - 1.5, 0.0).xz();
            if let Ok((mut q, ..)) = sel.single_mut() {
                q.0.clear();
                q.0.push_back(crate::interact::Action::new("Go Here", crate::interact::ActionKind::GoHere(to, level + 1), false));
            }
        }
        2 if time.elapsed_secs() - t0 > 5.0 => {
            *stage = 3;
            let Some([x0, z0, x1, z1]) = args.knock else { return };
            let (s, e) = (IVec2::new(x0, z0), IVec2::new(x1, z1));
            let (ops, _) = crate::build::plan(b, crate::build::BuildTool::Sledgehammer, true, b.view_level, s, e);
            info!("build test: knock-down — {} ops", ops.len());
            let level = b.view_level;
            crate::build::sell_openings(&mut commands, b, level, &crate::build::edges(crate::build::BuildTool::Sledgehammer, s, e), &objects, &mut removed, household.as_deref_mut(), &mut notes);
            apply(&mut commands, &mut assets, b, &mut ctx, ops);
            if let Some(g) = grid.as_mut() {
                g.dirty = true;
            }
        }
        4 if args.storey && time.elapsed_secs() - *last_log > 4.0 => {
            *last_log = time.elapsed_secs();
            if let Ok((q, tf, floor, path)) = sel.single() {
                info!(
                    "build test: selected at {:.1},{:.1},{:.1} floor {} — {:?}, path {:?}",
                    tf.translation.x,
                    tf.translation.y,
                    tf.translation.z,
                    floor.0,
                    q.0.front().map(|a| (&a.label, a.phase)),
                    path.map(|p| p.waypoints.len())
                );
            }
        }
        _ => {}
    }
}

/// `--place <kind>`: the cheapest catalog object of that kind (`Telescope`, `HotTub`…), set down
/// 2 m in front of the selected Sim.
#[allow(clippy::too_many_arguments)]
fn auto_place(
    args: Res<AutoArgs>,
    mut done: Local<bool>,
    sel: Query<&Transform, With<crate::sim::Selected>>,
    (catalog, data): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut commands: Commands,
) {
    let Some(want) = &args.place else { return };
    if *done {
        return;
    }
    let Ok(tf) = sel.single() else { return };
    *done = true;
    let want = want.to_ascii_lowercase();
    let Some(e) = catalog.entries.iter().filter(|e| e.price > 0 && format!("{:?}", e.kind).to_ascii_lowercase() == want).min_by_key(|e| e.price) else {
        warn!("--place {want}: nothing in the catalog");
        return;
    };
    let pos = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 2.0);
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, e.key, pos, 0.0);
    info!("placed {} for the test", e.name);
}

/// `--do <interaction>`: the selected sim performs this interaction on the first object offering it.
fn auto_action(
    args: Res<AutoArgs>,
    (mut done, mut expecting): (Local<bool>, Local<bool>),
    mut sel: Query<&mut crate::interact::ActionQueue, With<crate::sim::Selected>>,
    objects: Query<(Entity, &crate::interact::GameObject)>,
    visitors: Query<Entity, With<crate::interact::Visitor>>,
    sel_e: Query<Entity, With<crate::sim::Selected>>,
    mut rels_q: Query<&mut crate::sim::Relationships>,
    world: Res<crate::loading::CurrentWorld>,
    (mut commands, clock, members): (Commands, Res<crate::clock::GameClock>, Query<(Entity, &crate::sim::Sim), With<crate::sim::HouseholdMember>>),
    (time, mut since): (Res<Time>, Local<Option<f32>>),
    (opps_q, ui, sel_tf): (Query<&crate::opportunities::SimOpportunities>, Option<Res<crate::icons::GameUi>>, Query<&Transform, With<crate::sim::Selected>>),
) {
    let Some(name) = &args.action else { return };
    if *done {
        return;
    }
    // (Once everyone has settled in.)
    let t0 = *since.get_or_insert(time.elapsed_secs());
    if (name == "Meal" || name == "Die") && time.elapsed_secs() - t0 < 4.0 {
        return;
    }
    // "Care:<social>": the selected Sim looks after the household's baby (or toddler with
    // "CareT:"), bringing a baby into the world first if there isn't one.
    if let Some((want, social)) = name.strip_prefix("Care:").map(|s| (crate::sim::Age::Baby, s)).or_else(|| name.strip_prefix("CareT:").map(|s| (crate::sim::Age::Toddler, s))) {
        let little = members.iter().find(|(_, s)| s.age == want).map(|(e, _)| e);
        match little {
            Some(target) => {
                let Ok(mut q) = sel.single_mut() else { return };
                let si = crate::social::social_index(social).unwrap();
                q.0.clear();
                q.push_player(crate::interact::Action::new(social, crate::interact::ActionKind::Social { target, social: si }, false));
                *done = true;
            }
            None if want == crate::sim::Age::Baby => {
                let mum = members.iter().find(|(_, s)| s.female && s.age.is_grown() && s.age != crate::sim::Age::Child).map(|(e, _)| e);
                if let Some(e) = mum
                    && !*expecting
                {
                    commands.entity(e).insert(crate::little::Pregnancy { since: clock.minutes - 3.0 * 1440.0 + 5.0, other_parent: None, stage: 2 });
                    *expecting = true;
                }
            }
            None => {}
        }
        return;
    }
    // "Baby": a grown woman of the household is about to give birth.
    if name == "Baby" {
        if let Some((e, _)) = members.iter().find(|(_, s)| s.female && s.age.is_grown() && s.age != crate::sim::Age::Child) {
            let since = clock.minutes - 3.0 * 1440.0 + 20.0;
            commands.entity(e).insert(crate::little::Pregnancy { since, other_parent: None, stage: 2 });
        }
        *done = true;
        return;
    }
    let Ok(mut q) = sel.single_mut() else { return };
    // "Opp": an opportunity is offered now. "OppDo": accepted, and the Sim goes to do it.
    if name == "Opp" || name == "OppDo" {
        commands.insert_resource(crate::opportunities::ForceOffer);
        if name == "OppDo" {
            commands.insert_resource(crate::opportunities::AutoAccept);
        } else {
            *done = true;
        }
        let Ok(mut q) = sel.single_mut() else { return };
        let Ok(me) = sel_e.single() else { return };
        let Ok(opps) = opps_q.get(me) else { return };
        let Some(ui) = ui.as_ref() else { return };
        for lot in 0..world.data.lots.len() {
            if let Some((label, kind)) = crate::opportunities::lot_options(&world.data, lot, Some(opps), &ui.data, 12.0).into_iter().next() {
                info!("going to do {label}");
                q.0.clear();
                q.push_player(crate::interact::Action::new(label, kind, false));
                *done = true;
                return;
            }
        }
        return;
    }
    // "Grave": a grave by the selected Sim, of someone who died in a fire (for the ghosts).
    if name == "Grave" {
        let Ok(me) = sel_e.single() else { return };
        let Ok(tf) = sel_tf.get(me) else { return };
        let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 2.5);
        let rot = tf.rotation;
        commands.queue(move |w: &mut World| {
            let sim = crate::sim::random_sim(&mut rand::rng(), "Lothario", Some(false), crate::sim::Age::Adult);
            let objd = w.resource::<crate::baked::Baked>().0.catalog.iter().find(|c| c.instance_name == "UrnstoneHuman").map(|c| c.objd);
            let Some(objd) = objd else { return };
            w.resource_scope(|w, mut assets: Mut<crate::objects::ObjectAssets>| {
                w.resource_scope(|w, catalog: Mut<crate::loading::Catalog>| {
                    w.resource_scope(|w, data: Mut<crate::baked::Baked>| {
                        w.resource_scope(|w, mut meshes: Mut<Assets<Mesh>>| {
                            w.resource_scope(|w, mut images: Mut<Assets<Image>>| {
                                w.resource_scope(|w, mut mats: Mut<Assets<StandardMaterial>>| {
                                    let mut queue = bevy::ecs::world::CommandQueue::default();
                                    let mut commands = Commands::new(&mut queue, w);
                                    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                                    if let Some(o) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, objd, at, rot) {
                                        commands.entity(o.entity).insert(crate::ghosts::Grave { sim, cause: "in a fire".into() });
                                    }
                                    queue.apply(w);
                                });
                            });
                        });
                    });
                });
            });
        });
        *done = true;
        return;
    }
    // "Burglar": a burglar breaks in now.
    if name == "Burglar" {
        commands.queue(|w: &mut World| w.resource_mut::<crate::burglar::Break>().force = true);
        *done = true;
        return;
    }
    // "Fire": the stove catches fire.
    if name == "Fire" {
        if let Some((e, _)) = objects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Stove) {
            commands.entity(e).queue_silenced(|w: EntityWorldMut| {
                let lift = w.get::<crate::interact::GameObject>().map_or(0.0, |o| o.height * 0.85);
                let at = w.get::<Transform>().map(|t| t.translation + Vec3::Y * lift);
                let level = w.get::<crate::nav::Floor>().map_or(1, |f| f.0);
                if let Some(at) = at {
                    w.into_world_mut().write_message(crate::fire::StartFire { at, level });
                }
            });
        }
        *done = true;
        return;
    }
    // "Visit:<lot name part>": the selected Sim drives to that community lot.
    if let Some(want) = name.strip_prefix("Visit:") {
        let want = want.to_ascii_lowercase();
        let found = (0..world.data.lots.len()).find(|&i| {
            crate::visit::visitable(&world.data, i)
                && (world.data.lots[i].internal_name.to_ascii_lowercase().contains(&want) || world.data.lot_names.get(i).is_some_and(|n| n.to_ascii_lowercase().contains(&want)))
        });
        match found {
            Some(lot) => {
                q.0.clear();
                q.push_player(crate::interact::Action::new("Visit", crate::interact::ActionKind::GoToLot { lot }, false));
            }
            None => warn!("--do {name}: no such community lot"),
        }
        *done = true;
        return;
    }
    // "Garden": the selected Sim plants a tomato seed a few steps away (outdoors).
    if name == "Garden" {
        let Some(ui) = ui.as_ref() else { return };
        let Some(tomato) = ui.data.plants.iter().position(|p| p.produce == "Tomato") else { return };
        let Ok(me) = sel_e.single() else { return };
        let Ok(tf) = sel_tf.get(me) else { return };
        commands.insert_resource(crate::gardening::Garden { seeds: [(tomato, 2)].into_iter().collect() });
        let at = tf.translation + tf.rotation * Vec3::new(0.0, 0.0, 3.0);
        if let Ok(mut q) = sel.single_mut() {
            q.push_player(crate::interact::Action::new("Plant", crate::interact::ActionKind::PlantSeed { at: Vec2::new(at.x, at.z), level: 1, plant: tomato }, false));
        }
        *done = true;
        return;
    }
    // "Break": a shower breaks and the selected Sim repairs it.
    if name == "Break" {
        if let Some((e, _)) = objects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Shower) {
            commands.entity(e).insert(crate::interact::Broken);
            if let Ok(mut q) = sel.single_mut() {
                q.push_player(crate::interact::Action::new("Repair", crate::interact::ActionKind::Repair { target: e }, false));
            }
        }
        *done = true;
        return;
    }
    // "Party": the selected Sim throws a party.
    if name == "Party" {
        if let Ok(mut q) = sel.single_mut() {
            q.push_player(crate::interact::Action::new("Throw a Party", crate::interact::ActionKind::ThrowParty, false));
        }
        *done = true;
        return;
    }
    // "Pizza": the selected Sim phones for a pizza.
    if name == "Pizza" {
        if let Ok(mut q) = sel.single_mut() {
            q.push_player(crate::interact::Action::new("Order Pizza", crate::interact::ActionKind::OrderPizza, false));
        }
        *done = true;
        return;
    }
    // "Die": the selected Sim's time has come.
    if name == "Die" {
        let me = sel_e.single().unwrap();
        commands.entity(me).insert(crate::death::Dying::new());
        *done = true;
        return;
    }
    // "Meal": dinner is on the table (served from the nearest stove) for the household.
    if name == "Meal" {
        let me = sel_e.single().unwrap();
        if let Some((stove, _)) = objects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Stove) {
            commands.entity(me).insert(crate::meals::MealRequest::Serve(stove));
        }
        *done = true;
        return;
    }
    // "Social:<name>": the selected Sim does this social three times with another grown-up of
    // the household (or a visitor), as friends.
    if let Some(social) = name.strip_prefix("Social:") {
        let me = sel_e.single().unwrap();
        let target = members.iter().find(|(e, s)| *e != me && s.age.is_grown() && s.age != crate::sim::Age::Child).map(|(e, _)| e).or_else(|| visitors.iter().next());
        let (Some(target), Some(si)) = (target, crate::social::social_index(social)) else { return };
        for (a, b) in [(me, target), (target, me)] {
            if let Ok(mut rels) = rels_q.get_mut(a) {
                let r = rels.entry(b);
                r.friendship = r.friendship.max(60.0);
            }
        }
        q.0.clear();
        for _ in 0..3 {
            q.push_player(crate::interact::Action::new(social, crate::interact::ActionKind::Social { target, social: si }, false));
        }
        *done = true;
        return;
    }
    // "Try for Baby": with a household member (or visitor) of the other sex.
    if name == "Try for Baby" {
        let me = sel_e.single().unwrap();
        let Ok((_, me_sim)) = members.get(me) else { return };
        let target = members
            .iter()
            .find(|(e, s)| *e != me && s.female != me_sim.female && s.age.is_grown() && s.age != crate::sim::Age::Child)
            .map(|(e, _)| e)
            .or_else(|| visitors.iter().next());
        let Some(target) = target else { return };
        for (a, b) in [(me, target), (target, me)] {
            if let Ok(mut rels) = rels_q.get_mut(a) {
                let r = rels.entry(b);
                r.friendship = r.friendship.max(60.0);
                r.romance = r.romance.max(80.0);
                if !matches!(r.status, crate::social::RelStatus::Married | crate::social::RelStatus::Engaged) {
                    r.status = crate::social::RelStatus::Partner;
                }
            }
        }
        q.0.clear();
        let si = crate::social::social_index("Try for Baby").unwrap();
        for _ in 0..3 {
            q.push_player(crate::interact::Action::new(name.clone(), crate::interact::ActionKind::Social { target, social: si }, false));
        }
        *done = true;
        return;
    }
    // "Romance": the selected Sim courts a visitor through to marriage.
    if name == "Romance" {
        let Some(target) = visitors.iter().next() else { return };
        if let Ok(mut rels) = rels_q.get_mut(sel_e.single().unwrap()) {
            let r = rels.entry(target);
            r.friendship = 60.0;
            r.romance = 80.0;
            r.status = crate::social::RelStatus::Partner;
        }
        if let Ok(mut rels) = rels_q.get_mut(target) {
            let me = sel_e.single().unwrap();
            let r = rels.entry(me);
            r.friendship = 60.0;
            r.romance = 80.0;
            r.status = crate::social::RelStatus::Partner;
        }
        q.0.clear();
        for social in ["Kiss", "Propose Marriage", "Get Married"] {
            let si = crate::social::social_index(social).unwrap();
            q.push_player(crate::interact::Action::new(social, crate::interact::ActionKind::Social { target, social: si }, false));
        }
        *done = true;
        return;
    }
    // "Visit <place>": drive to that community lot's first activity.
    if let Some(place) = name.strip_prefix("Visit ") {
        let place = place.to_ascii_lowercase();
        let lot = world.data.lots.iter().position(|l| {
            l.internal_name.to_ascii_lowercase().contains(&place) && !crate::rabbitholes::activities(l).is_empty()
        });
        if let Some(lot) = lot {
            q.0.clear();
            q.push_player(crate::interact::Action::new(name.clone(), crate::interact::ActionKind::Visit { lot, activity: 0 }, false));
        }
        *done = true;
        return;
    }
    // "Join <career>": apply at the computer.
    if let Some(career) = name.strip_prefix("Join ") {
        let track = crate::careers::careers().iter().position(|c| c.name.eq_ignore_ascii_case(career));
        let computer = objects.iter().find(|(_, o)| matches!(o.kind, crate::interact::ObjectKind::Computer)).map(|(e, _)| e);
        if let (Some(track), Some(target)) = (track, computer) {
            q.0.clear();
            q.push_player(crate::interact::Action::new(name.clone(), crate::interact::ActionKind::JoinCareer { target, track }, false));
        }
        *done = true;
        return;
    }
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
    world: Option<Res<crate::loading::CurrentWorld>>,
    (mut panel, settings, mut game_menu, mut clock): (
        ResMut<crate::options::OptionsPanel>,
        Res<crate::options::Settings>,
        ResMut<crate::options::GameMenu>,
        Option<ResMut<crate::clock::GameClock>>,
    ),
    mut buy: ResMut<crate::buy::BuyMode>,
) {
    let Some(dir) = &args.ui_flow else { return };
    let now = time.elapsed_secs();
    let since = now - stage.1;
    let shot = |commands: &mut Commands, name: &str| {
        let _ = std::fs::create_dir_all(dir);
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
            if let Some((mut i, _)) = menu.iter_mut().find(|(_, a)| matches!(a, crate::menu::MenuAction::Options)) {
                *i = Interaction::Pressed;
            }
            *stage = (30, now);
        }
        (30, AppState::MainMenu, _) if since > 1.0 => {
            shot(&mut commands, "1a_options");
            crate::options::close_options(&mut commands, &mut panel, &settings);
            *stage = (1, now);
        }
        (1, AppState::MainMenu, _) if since > 0.5 => {
            if let Some((mut i, _)) = menu.iter_mut().find(|(_, a)| matches!(a, crate::menu::MenuAction::PlayWorld(0))) {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (2, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2_household");
            *stage = (40, now);
        }
        // Body shape: five steps heavier, a picture, ten steps thinner, another.
        (40..=44, AppState::CreateHousehold, _) if since > 0.3 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| **a == crate::home::CasAction::Weight(1)) {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (45, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2w_heavy");
            advance(&mut stage);
        }
        (46..=55, AppState::CreateHousehold, _) if since > 0.3 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| **a == crate::home::CasAction::Weight(-1)) {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (56, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2w_thin");
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Tab(crate::cas::CasTab::Traits))) {
                *i = Interaction::Pressed;
            }
            *stage = (3, now);
        }
        (3, AppState::CreateHousehold, _) if since > 1.0 => {
            shot(&mut commands, "2a_traits");
            *stage = (68, now);
        }
        (68, AppState::CreateHousehold, _) if since > 0.5 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Tab(crate::cas::CasTab::Hair))) {
                *i = Interaction::Pressed;
            }
            *stage = (65, now);
        }
        (65, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2h_hair");
            *stage = (67, now);
        }
        (67, AppState::CreateHousehold, _) if since > 0.5 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Tab(crate::cas::CasTab::Tops))) {
                *i = Interaction::Pressed;
            }
            *stage = (66, now);
        }
        (66, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2t_tops");
            // With --family, browse the town's families first.
            let want = if args.family.is_some() { crate::home::CasAction::Families } else { crate::home::CasAction::Done };
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| **a == want) {
                *i = Interaction::Pressed;
            }
            if args.family.is_some() {
                *stage = (20, now);
            } else {
                *stage = (4, now);
            }
        }
        (20, AppState::CreateHousehold, _) if since > 1.0 => {
            shot(&mut commands, "2b_families");
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Family(0))) {
                *i = Interaction::Pressed;
            }
            advance(&mut stage);
        }
        (21, AppState::CreateHousehold, _) if since > 2.0 => {
            shot(&mut commands, "2c_family");
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Done)) {
                *i = Interaction::Pressed;
            }
            *stage = (4, now);
        }
        // A town family moves straight into their home.
        (5, AppState::InGame, Some(crate::PlayMode::Live)) => *stage = (7, now),
        (4, AppState::Loading, _) if since > 1.0 => {
            shot(&mut commands, "3_loading");
            advance(&mut stage);
        }
        (4, AppState::InGame, _) => advance(&mut stage),
        (5, AppState::InGame, Some(crate::PlayMode::ChooseLot)) if since > 3.0 => {
            shot(&mut commands, "4_choose_lot");
            // The first furnished house, as a player would pick from the top of the list.
            let house = world.as_ref().and_then(|w| lots.iter().map(|(_, b)| b.0).filter(|i| w.data.buildings.get(i).is_some_and(|b| b.is_furnished())).min());
            if let (Some(h), Some(w)) = (house, world.as_ref())
                && let Some(l) = w.data.lots.get(h)
            {
                info!("ui flow lot {h}: {:016X} {} {:?}", l.id, l.internal_name, l.string_keys);
            }
            if let Some((mut i, _)) = lots.iter_mut().find(|(_, b)| house.is_none_or(|h| b.0 == h)) {
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
        (8, AppState::InGame, _) if since > 1.0 => {
            buy.show(0);
            *stage = (63, now);
        }
        (63, AppState::InGame, _) if since > 1.5 => {
            shot(&mut commands, "6a_buy");
            *stage = (64, now);
        }
        (64, AppState::InGame, _) if since > 0.5 => {
            buy.show(crate::buy::WALLPAPER_TAB);
            *stage = (60, now);
        }
        (60, AppState::InGame, _) if since > 1.5 => {
            shot(&mut commands, "6b_wallpaper");
            *stage = (61, now);
        }
        (61, AppState::InGame, _) if since > 0.5 => {
            buy.show(0);
            buy.active = false;
            *stage = (62, now);
        }
        (62, AppState::InGame, _) if since > 1.0 => {
            crate::options::toggle_game_menu(&mut commands, &mut game_menu, clock.as_deref_mut());
            *stage = (9, now);
        }
        (9, AppState::InGame, _) if since > 1.0 => {
            shot(&mut commands, "7_game_menu");
            advance(&mut stage);
        }
        (10, AppState::InGame, _) if since > 1.0 => {
            crate::options::open_options(&mut commands, &mut panel, &settings);
            advance(&mut stage);
        }
        (11, AppState::InGame, _) if since > 1.0 => {
            shot(&mut commands, "8_options_in_game");
            crate::options::close_options(&mut commands, &mut panel, &settings);
            crate::options::toggle_game_menu(&mut commands, &mut game_menu, clock.as_deref_mut());
            advance(&mut stage);
        }
        (12, _, _) if since > 2.0 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn auto_speed(args: Res<AutoArgs>, mut clock: ResMut<crate::clock::GameClock>, mut done: Local<bool>) {
    if *done {
        return;
    }
    if let Some(s) = args.speed {
        clock.speed = s.min(3);
    }
    if let Some(h) = args.hour {
        clock.minutes = h * 60.0;
    }
    *done = true;
}

/// `--view-level <n>` once, or `--view-level 0`: keep viewing the selected Sim's floor.
fn auto_view_level(
    args: Res<AutoArgs>,
    building: Option<ResMut<crate::building::ActiveBuilding>>,
    mut done: Local<bool>,
    sel: Query<&crate::nav::Floor, With<crate::sim::Selected>>,
) {
    match (args.view_level, building) {
        (Some(0), Some(mut b)) => {
            if let Ok(f) = sel.single() {
                let l = f.0.clamp(1, b.top_level);
                if b.view_level != l {
                    b.view_level = l;
                }
            }
        }
        (Some(l), Some(mut b)) if !*done => {
            b.view_level = l.clamp(1, b.top_level);
            *done = true;
        }
        _ => {}
    }
}

fn auto_save(args: Res<AutoArgs>, time: Res<Time>, mut since: Local<Option<f32>>, mut done: Local<bool>, mut w: MessageWriter<crate::save::SaveRequest>) {
    let Some(at) = args.save_at else { return };
    let start = *since.get_or_insert(time.elapsed_secs());
    if !*done && time.elapsed_secs() - start >= at {
        *done = true;
        crate::save::request_save(&mut w);
    }
}

fn auto_load(args: Res<AutoArgs>, worlds: Res<WorldList>, mut commands: Commands, mut next: ResMut<NextState<AppState>>, mut done: Local<bool>) {
    let Some(n) = args.load else { return };
    if *done {
        return;
    }
    *done = true;
    if let Some((_, g)) = crate::save::list_saves().into_iter().nth(n)
        && crate::save::begin_load(&mut commands, &worlds, g)
    {
        next.set(AppState::Loading);
    }
}
