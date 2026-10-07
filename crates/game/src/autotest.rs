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
            .add_systems(Update, list_cams)
            .add_systems(Update, auto_pick_world.run_if(in_state(AppState::MainMenu)))
            .add_systems(Update, (apply_cam, watch_insect, ask_question, keep_hungry, wear_uniform, give_items, make_mess, show_uniforms, auto_terrain, auto_sculpt, run_out, face_hook, shots_every, show_designs, walls_hook, hang_paintings, buy_close, diving_board).run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_screenshot.run_if(in_state(AppState::InGame)))
            .add_systems(Update, portrait_cam.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_action.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_place.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_paint.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_build.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_fence.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(Update, auto_balloon.run_if(in_state(crate::PlayMode::Live)))
            .add_systems(
                Update,
                (|mut commands: Commands, args: Res<AutoArgs>, mut p: ResMut<crate::relations::RelationsPanel>, mut j: ResMut<crate::collecting::JournalPanel>, mut done: Local<bool>, time: Res<Time>| {
                    if args.relations && !*done {
                        *done = true;
                        p.open = true;
                        // (FAMILY_TREE=1: the family tree too.)
                        p.with_tree = std::env::var("FAMILY_TREE").is_ok();
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
                            let paint = crate::buy::PAINT_TABS.iter().position(|c| c.eq_ignore_ascii_case(&t)).map(|i| crate::buy::CATEGORIES.len() + i);
                            if let Some(i) = crate::buy::CATEGORIES.iter().position(|c| c.eq_ignore_ascii_case(&t)).or(paint) {
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
                // --use <Kind>[:<interaction>]: the selected Sim uses the nearest such object (the
                // named interaction, or its first); USE_REPEAT=1 queues it again whenever they're
                // idle. SKILL=<skill>:<level> sets a skill first.
                (|args: Res<AutoArgs>,
                  mut done: Local<bool>,
                  time: Res<Time>,
                  mut sel: Query<(&mut crate::interact::ActionQueue, &Transform, Option<&crate::visit::OnLot>, &mut crate::interact::Skills), With<crate::sim::Selected>>,
                  objects: Query<(Entity, &crate::interact::GameObject, &Transform, Option<&crate::visit::LotObject>)>| {
                    let Some(first) = &args.use_kind else { return };
                    let Ok((mut q, tf, on, mut skills)) = sel.single_mut() else { return };
                    // THEN_USE=<Kind>:<interaction>: once that's done, this.
                    let mut want = first.clone();
                    if *done
                        && let Ok(then) = std::env::var("THEN_USE")
                        && let Some(kind) = then.split(':').next()
                        && objects.iter().any(|(_, o, _, _)| format!("{:?}", o.kind).eq_ignore_ascii_case(kind))
                    {
                        // SAFETY: a test setting, read only here, on this thread; cleared once used.
                        unsafe { std::env::remove_var("THEN_USE") };
                        want = then;
                        *done = false;
                    }
                    if *done && std::env::var("USE_REPEAT").is_ok() && q.0.is_empty() {
                        *done = false;
                    }
                    if *done || time.elapsed_secs() < 8.0 {
                        return;
                    }
                    if let Some((s, l)) = std::env::var("SKILL").ok().and_then(|v| v.split_once(':').map(|(s, l)| (s.to_string(), l.parse::<f32>().unwrap_or(0.0))))
                        && let Some(name) = crate::save::SKILLS.iter().find(|n| n.eq_ignore_ascii_case(&s))
                    {
                        let v = skills.0.entry(name).or_insert(0.0);
                        *v = v.max(l);
                    }
                    if args.action.as_deref().is_some_and(|a| a.starts_with("Visit:")) && on.is_none() {
                        return;
                    }
                    let lot = on.map(|l| l.0);
                    let want = want.to_ascii_lowercase();
                    let (want, named) = want.split_once(':').map_or((want.as_str(), None), |(k, n)| (k, Some(n)));
                    let best = objects
                        .iter()
                        .filter(|(_, o, _, l)| format!("{:?}", o.kind).to_ascii_lowercase() == want && l.map(|l| l.0) == lot)
                        .min_by(|a, b| a.2.translation.distance(tf.translation).total_cmp(&b.2.translation.distance(tf.translation)));
                    if let Some((e, o, ..)) = best
                        && let Some((i, d)) = crate::interact::interactions_for(o.kind).iter().enumerate().find(|(_, d)| named.is_none_or(|n| d.name.eq_ignore_ascii_case(n)))
                    {
                        *done = true;
                        info!("use test: {} on the {} ({e:?})", d.name, o.name);
                        q.0.clear();
                        q.push_player(crate::interact::Action::new(d.name, crate::interact::ActionKind::Object { target: e, def: i }, false));
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
                  named: Query<(&Transform, &crate::interact::ActionQueue, Option<&crate::visit::OnLot>, &crate::sim::Sim)>,
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
                    // (FOLLOW_NAME=<name>: whoever has that first or last name, while they're about.)
                    let who = std::env::var("FOLLOW_NAME").ok().and_then(|n| named.iter().find(|s| s.3.first == n || s.3.last == n).map(|s| (s.0, s.1, s.2)));
                    if let (Some((tf, q, on)), Ok(mut c)) = (who.or(sel.single().ok()), cam.single_mut()) {
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
            .add_systems(PreUpdate, auto_move_house.after(bevy::ui::UiSystems::Focus).run_if(in_state(crate::PlayMode::ChooseLot)))
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
        // TIES=<i>-<j>:<Spouses|Partners|Siblings|ParentOf|ChildOf>[;...]: what the new
        // household's Sims are to each other.
        if let Ok(v) = std::env::var("TIES") {
            let ties = v
                .split(';')
                .filter_map(|t| {
                    let (pair, kind) = t.split_once(':')?;
                    let (i, j) = pair.split_once('-')?;
                    let (a, b) = (pending.members.get(i.parse::<usize>().ok()?)?.id, pending.members.get(j.parse::<usize>().ok()?)?.id);
                    let kind = crate::family::Tie::ALL.into_iter().find(|k| format!("{k:?}").eq_ignore_ascii_case(kind))?;
                    Some((a, b, kind))
                })
                .collect();
            commands.insert_resource(crate::family::HouseholdTies { members: pending.members.iter().map(|m| m.id).collect(), ties });
        }
        commands.insert_resource(pending);
        next.set(AppState::Loading);
    } else {
        warn!("--world {name}: no such world");
    }
}

/// WALLS=up|cutaway|down: the walls shown that way.
fn walls_hook(mut walls: ResMut<crate::building::WallMode>, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    *walls = match std::env::var("WALLS").as_deref() {
        Ok("up") => crate::building::WallMode::Up,
        Ok("down") => crate::building::WallMode::Down,
        _ => return,
    };
}

/// SHOTS_EVERY=<seconds> (with SHOTS_DIR=<folder>): a picture that often while playing, with
/// the game clock in its name (for long runs looked over afterwards).
fn shots_every(mut commands: Commands, time: Res<Time>, clock: Res<crate::clock::GameClock>, mut last: Local<f32>, mut n: Local<u32>) {
    let (Some(every), Ok(dir)) = (std::env::var("SHOTS_EVERY").ok().and_then(|v| v.parse::<f32>().ok()), std::env::var("SHOTS_DIR")) else { return };
    if time.elapsed_secs() - *last < every {
        return;
    }
    *last = time.elapsed_secs();
    *n += 1;
    let m = clock.minutes as i64;
    let path = std::path::Path::new(&dir).join(format!("{:03}_day{}_{:02}{:02}.png", *n, m / 1440, (m / 60) % 24, m % 60));
    info!("shot {}", path.display());
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

/// CAMS=1: the cameras and UI roots, every couple of seconds (debugging).
fn list_cams(time: Res<Time>, mut last: Local<f32>, cams: Query<(Entity, &Camera, Has<Camera3d>, Has<Camera2d>, Option<&bevy::camera::RenderTarget>)>, roots: Query<(Entity, &ComputedNode, Option<&UiTargetCamera>), (With<Node>, Without<ChildOf>)>) {
    if std::env::var("CAMS").is_err() || time.elapsed_secs() - *last < 2.0 {
        return;
    }
    *last = time.elapsed_secs();
    for (e, c, d3, d2, t) in &cams {
        info!("camera {e:?} order {} active {} 3d {d3} 2d {d2} target {:?}", c.order, c.is_active, t);
    }
    for (e, n, t) in &roots {
        info!("ui root {e:?} size {:?} target {:?}", n.size(), t.map(|t| t.0));
    }
}

/// `--do Move:<lot name part>`: moving house, the lot is chosen and Move In pressed.
fn auto_move_house(
    args: Res<AutoArgs>,
    moving: Option<Res<crate::home::Moving>>,
    world: Res<crate::loading::CurrentWorld>,
    mut chosen: ResMut<crate::home::ChosenLot>,
    mut button: Query<&mut Interaction, With<crate::home::MoveInButton>>,
    time: Res<Time>,
    mut since: Local<Option<f32>>,
) {
    let (Some(_), Some(want)) = (moving, args.action.as_deref().and_then(|a| a.strip_prefix("Move:"))) else { return };
    let t0 = *since.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - t0 < 2.0 {
        return;
    }
    let want = want.to_ascii_lowercase();
    let Some(l) = world.data.lot_names.iter().position(|n| n.to_ascii_lowercase().contains(&want)) else { return };
    chosen.0 = Some(l);
    if let Ok(mut i) = button.single_mut() {
        *i = Interaction::Pressed;
    }
}

/// HUNGRY=<first name>: that Sim's hunger stays at the bottom.
/// UNIFORM=<career>:<level>: the selected Sim takes that job (level from 1) and puts its
/// uniform on. CHANGE_INTO=<outfit>: the outfit they'll change into at a dresser (with
/// `--use "Dresser:Change Into"`).
fn wear_uniform(mut commands: Commands, sel: Query<Entity, With<crate::sim::Selected>>, mut done: Local<bool>, time: Res<Time>) {
    if let Some(k) = std::env::var("CHANGE_INTO").ok().and_then(|n| crate::simbody::OutfitKind::CHOICES.into_iter().find(|k| k.label().eq_ignore_ascii_case(&n)))
        && let Ok(e) = sel.single()
        && !*done
        && time.elapsed_secs() > 6.0
    {
        *done = true;
        commands.entity(e).insert(crate::simbody::ChangeIntoPlan(k));
    }
    // PLAN_OUTFIT=1: the selected Sim's wardrobe opens (PLAN_OUTFIT=<outfit>: on that outfit).
    if let Ok(v) = std::env::var("PLAN_OUTFIT")
        && let Ok(e) = sel.single()
        && time.elapsed_secs() > 6.0
        && !*done
    {
        *done = true;
        let kind = crate::simbody::OutfitKind::CHOICES.into_iter().find(|k| k.label().eq_ignore_ascii_case(&v)).unwrap_or_default();
        let p = crate::planner::OutfitPlanner::open_on(&mut commands, e, kind);
        commands.insert_resource(p);
    }
    let Ok(v) = std::env::var("UNIFORM") else { return };
    if *done || time.elapsed_secs() < 4.0 {
        return;
    }
    let Ok(e) = sel.single() else { return };
    let (name, level) = v.split_once(':').map_or((v.as_str(), 1), |(n, l)| (n, l.parse().unwrap_or(1)));
    let Some(track) = crate::careers::careers().iter().position(|c| c.name.eq_ignore_ascii_case(name)) else {
        warn!("no career {name}");
        *done = true;
        return;
    };
    *done = true;
    let mut job = crate::careers::Job::new(track);
    job.level = level.max(1) - 1;
    info!("uniform test: {} level {}", name, job.level + 1);
    commands.entity(e).insert((job, crate::simbody::Wearing(crate::simbody::OutfitKind::Career), crate::aging::NeedsNewBody));
}

/// GIVE=<item>,...: puts things in the selected Sim's inventory: a collectible's key
/// (`Ruby`, `Minnow`) or `<produce>/<quality 0-9>`, each with an optional `*<count>`.
/// EAT=1: then they eat the first produce.
fn give_items(
    mut commands: Commands,
    mut sel: Query<(Entity, &mut crate::interact::ActionQueue, Option<&crate::inventory::Inventory>), With<crate::sim::Selected>>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut done: Local<u8>,
    time: Res<Time>,
) {
    let Ok(v) = std::env::var("GIVE") else { return };
    let (Ok((e, mut queue, inv)), Some(ui)) = (sel.single_mut(), ui) else { return };
    if *done == 1
        && std::env::var("EAT").is_ok()
        && let Some(s) = inv.and_then(|i| i.0.iter().find(|s| s.kind == crate::inventory::ItemKind::Produce))
    {
        *done = 2;
        info!("eating {} (have {})", s.name, s.count);
        queue.push_player(crate::interact::Action::new(format!("Eat {}", s.name), crate::interact::ActionKind::EatItem { key: s.key.clone(), quality: s.quality }, false));
    }
    if *done > 0 || time.elapsed_secs() < 6.0 {
        return;
    }
    *done = 1;
    use crate::inventory::ItemKind;
    for item in v.split(',') {
        let (what, n) = item.split_once('*').map_or((item, 1), |(w, n)| (w, n.parse().unwrap_or(1)));
        // A fish of a quality: `Minnow/9`.
        if let Some((key, q)) = what.split_once('/')
            && let Some(c) = ui.data.collectibles.iter().find(|c| c.key.eq_ignore_ascii_case(key) && c.kind == s3bake::gamedata::CollectKind::Fish)
        {
            let q: usize = q.parse().unwrap_or(3).min(9);
            let word = crate::gardening::QUALITIES[q].0;
            crate::inventory::give(&mut commands, e, ItemKind::Fish, c.key.clone(), format!("{word} {}", c.name), q as u8, c.max_price as i64, n);
            if std::env::var("BOWL").is_ok() {
                commands.entity(e).insert(crate::fishbowl::FishPlan(c.key.clone(), q as u8));
            }
            continue;
        }
        if what.eq_ignore_ascii_case("painting") {
            crate::inventory::give(&mut commands, e, ItemKind::Painting, "painting#test".into(), "Fine Painting".into(), 0, 120, n);
        } else if let Some((produce, q)) = what.split_once('/') {
            let q: usize = q.parse().unwrap_or(3).min(9);
            let Some(p) = ui.data.plants.iter().find(|p| p.produce.eq_ignore_ascii_case(produce)) else { continue };
            let (word, m) = crate::gardening::QUALITIES[q];
            let each = (p.price as f32 * m).round() as i64;
            crate::inventory::give(&mut commands, e, ItemKind::Produce, p.produce.clone(), format!("{word} {}", p.produce), q as u8, each, n);
        } else if let Some(c) = ui.data.collectibles.iter().find(|c| c.key.eq_ignore_ascii_case(what)) {
            let kind = match c.kind {
                s3bake::gamedata::CollectKind::Fish => ItemKind::Fish,
                s3bake::gamedata::CollectKind::Butterfly | s3bake::gamedata::CollectKind::Beetle => ItemKind::Insect,
                _ => ItemKind::Find,
            };
            crate::inventory::give(&mut commands, e, kind, c.key.clone(), c.name.clone(), 0, c.min_price as i64, n);
        } else {
            warn!("GIVE: no such item {what}");
        }
    }
}

/// MESS=<n>: n dirty plates set down around the selected Sim (who then stays put).
#[allow(clippy::too_many_arguments)]
fn make_mess(
    mut commands: Commands,
    mut sel: Query<(&Transform, &mut crate::interact::ActionQueue), With<crate::sim::Selected>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut done: Local<bool>,
    time: Res<Time>,
) {
    let Some(n) = std::env::var("MESS").ok().and_then(|v| v.parse::<usize>().ok()) else { return };
    let Ok((tf, mut queue)) = sel.single_mut() else { return };
    queue.0.clear();
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    *done = true;
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    for i in 0..n {
        let a = i as f32 * 1.3;
        let at = tf.translation + Vec3::new(a.cos() * 1.2, 0.0, a.sin() * 1.2);
        let e = crate::meals::spawn_dish(&mut commands, &mut assets, &mut ctx, &catalog, crate::meals::PLATE, crate::interact::ObjectKind::DirtyDishes, "Dirty Dishes", at, a);
        info!("mess: plate {e:?} at {at:.1?}");
    }
}

/// TERRAIN=<x>,<z>,<radius>,<layer>[;...]: terrain paint strokes on the lot (lot coordinates;
/// layer 255 erases), with the camera on the first.
fn auto_terrain(
    mut strokes: ResMut<crate::terrain_paint::Strokes>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut cam: Query<&mut SimsCamera>,
    mut done: Local<bool>,
    time: Res<Time>,
) {
    let Ok(v) = std::env::var("TERRAIN") else { return };
    let Some(b) = building else { return };
    if *done || time.elapsed_secs() < 5.0 {
        return;
    }
    *done = true;
    for (i, s) in v.split(';').enumerate() {
        let n: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if n.len() < 4 {
            continue;
        }
        let at = b.world(n[0], n[1], 0.0);
        strokes.pending.push(crate::terrain_paint::Stroke { x: at.x, z: at.z, radius: n[2], layer: n[3] as u8 });
        if i == 0
            && let Ok(mut c) = cam.single_mut()
        {
            c.focus = Vec3::new(at.x, c.focus.y, at.z);
            c.distance = 18.0;
        }
    }
}

/// SCULPT=<x>,<z>,<tool>,<steps>: the terrain tool (0 raise, 1 lower, 2 flatten, 3 smooth)
/// held down at that lot point, with the camera on it.
fn auto_sculpt(
    mut test: ResMut<crate::terrain_paint::SculptAt>,
    mut buy: ResMut<crate::buy::BuyMode>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    world: Res<crate::loading::CurrentWorld>,
    mut cam: Query<&mut SimsCamera>,
    mut done: Local<bool>,
    time: Res<Time>,
) {
    let Ok(v) = std::env::var("SCULPT") else { return };
    let Some(b) = building else { return };
    if *done || time.elapsed_secs() < 5.0 {
        return;
    }
    *done = true;
    let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if n.len() < 4 {
        return;
    }
    let at = b.world(n[0], n[1], 0.0);
    let at = Vec3::new(at.x, world.data.heightmap.sample(at.x, at.z), at.z);
    buy.sculpt = n[2] as u8;
    test.0 = Some((at, n[3] as u32));
    if let Ok(mut c) = cam.single_mut() {
        c.focus = Vec3::new(at.x, c.focus.y, at.z);
        c.distance = 16.0;
    }
}

/// FACE=<beard index>[,<glasses index>]: the household's men get that beard (and everyone
/// those glasses), from the game's baked lists, a few seconds in.
fn face_hook(mut commands: Commands, mut sims: Query<(Entity, &mut crate::sim::Sim), With<crate::sim::HouseholdMember>>, cas: Option<Res<crate::simbody::CasData>>, mut done: Local<bool>, time: Res<Time>) {
    let Ok(v) = std::env::var("FACE") else { return };
    let Some(cas) = cas else { return };
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    *done = true;
    let n: Vec<usize> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let list = |t: u32| cas.parts.iter().filter(|p| p.baked && p.clothing_type == t && p.age_gender & 0x20 != 0).map(|p| (p.key, p.name.clone())).collect::<Vec<_>>();
    for (e, mut s) in &mut sims {
        if !s.female
            && let Some((k, name)) = n.first().and_then(|i| list(s3formats::sim::CT_BEARD).get(*i).cloned())
        {
            info!("face test: {} grows {name}", s.first);
            s.outfit.beard = Some(k);
        }
        if let Some((k, name)) = n.get(1).and_then(|i| list(s3formats::sim::CT_GLASSES).get(*i).cloned()) {
            info!("face test: {} puts on {name}", s.first);
            s.outfit.glasses = Some(k);
        }
        commands.entity(e).insert(crate::aging::NeedsNewBody);
    }
}

/// SHOW_UNIFORM=1: the maid, the repairman, the mail carrier and the pizza delivery stand in a
/// row in front of the selected Sim (who stays put), with the camera on them.
fn show_uniforms(mut commands: Commands, mut sel: Query<(&Transform, &mut crate::interact::ActionQueue), With<crate::sim::Selected>>, mut cam: Query<&mut SimsCamera>, mut done: Local<bool>, time: Res<Time>) {
    if std::env::var("SHOW_UNIFORM").is_err() {
        return;
    }
    let Ok((tf, mut queue)) = sel.single_mut() else { return };
    queue.0.clear();
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    *done = true;
    use crate::simbody::ServiceUniform as U;
    let mut rng = rand::rng();
    let ahead = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
    let side = Vec3::new(ahead.z, 0.0, -ahead.x);
    for (i, (u, female, age)) in [(U::Maid, true, crate::sim::Age::Adult), (U::Repair, false, crate::sim::Age::Adult), (U::MailCarrier, true, crate::sim::Age::Adult), (U::PizzaDelivery, false, crate::sim::Age::Teen)].into_iter().enumerate() {
        let at = tf.translation + ahead * 2.2 + side * (i as f32 - 1.5) * 0.9;
        let sim = crate::sim::random_sim(&mut rng, "Uniform", Some(female), age);
        let e = crate::services::arrive(&mut commands, sim, at, Some(u), crate::interact::Skills::default());
        commands.entity(e).insert(Transform::from_translation(at).looking_to(-ahead, Vec3::Y));
    }
    if let Ok(mut c) = cam.single_mut() {
        c.look_at(tf.translation + ahead * 2.2 + Vec3::Y * 0.9);
        c.distance = 5.5;
    }
}

/// DESIGNS=<catalogue instance name>: that object in each of its designs, in a row in front of
/// the selected Sim, with the camera on them (DESIGNS_BUY=1: and buy mode holding one, its
/// designs on show).
#[allow(clippy::too_many_arguments)]
fn show_designs(
    mut commands: Commands,
    sel: Query<&Transform, With<crate::sim::Selected>>,
    mut cam: Query<&mut SimsCamera>,
    mut done: Local<bool>,
    time: Res<Time>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Ok(name) = std::env::var("DESIGNS") else { return };
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    let Ok(tf) = sel.single() else { return };
    *done = true;
    let Some(objd) = data.0.catalog.iter().find(|e| e.instance_name.eq_ignore_ascii_case(&name)).map(|e| e.objd) else {
        warn!("designs test: no catalogue object {name}");
        return;
    };
    let Some(entry) = catalog.by_key(&objd) else { return };
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let n = crate::objects::ObjectAssets::design_count(&ctx, objd);
    info!("designs test: {} has {n} designs", entry.name);
    let width = crate::objects::parts_bounds(&assets.object(&mut ctx, objd)).map_or(1.0, |(a, b)| (b.x - a.x).max(b.z - a.z) + 0.4);
    let ahead = (tf.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
    let side = Vec3::new(ahead.z, 0.0, -ahead.x);
    let face = Quat::from_rotation_arc(Vec3::Z, -ahead);
    for d in 0..n.max(1) {
        let at = tf.translation + ahead * 2.5 + side * (d as f32 - (n as f32 - 1.0) / 2.0) * width;
        crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, objd, at, face, Some(crate::objects::design_texture(objd, d)));
    }
    if let Ok(mut c) = cam.single_mut() {
        c.look_at(tf.translation + ahead * 2.5);
        c.distance = (n as f32 * width * 0.9).max(4.0);
    }
    // Buy mode, holding one.
    if std::env::var("DESIGNS_BUY").is_err() {
        return;
    }
    commands.queue(move |w: &mut World| {
        let parts = w.resource_scope(|w, mut assets: Mut<crate::objects::ObjectAssets>| {
            w.resource_scope(|w, data: Mut<crate::baked::Baked>| {
                w.resource_scope(|w, mut meshes: Mut<Assets<Mesh>>| {
                    w.resource_scope(|w, mut images: Mut<Assets<Image>>| {
                        let mut mats = w.resource_mut::<Assets<StandardMaterial>>();
                        let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                        assets.object_design(&mut ctx, objd, Some(crate::objects::design_texture(objd, 1)))
                    })
                })
            })
        });
        let ghost = w.spawn((Transform::from_xyz(0.0, -1000.0, 0.0), Visibility::default())).id();
        for p in parts {
            let child = w.spawn((Mesh3d(p.mesh.clone()), MeshMaterial3d(p.material.clone()))).id();
            w.entity_mut(ghost).add_child(child);
        }
        let mut b = w.resource_mut::<crate::buy::BuyMode>();
        b.show(0);
        b.placing = Some(crate::buy::Placing::new(objd, ghost, false, Some(crate::objects::design_texture(objd, 1))));
    });
}

/// PAINTINGS=<Painting level>: the selected Sim paints a painting on each canvas at that level
/// (into their inventory), and three more hang on the nearest long wall, with the camera on
/// them (PAINTINGS_BUY=1: and buy mode holding the first from the inventory). PAINTINGS=look:
/// the camera on the first painting already hanging; PAINTINGS=easel: on the lot's easel.
#[allow(clippy::too_many_arguments)]
fn hang_paintings(
    mut commands: Commands,
    sel: Query<(Entity, &Transform, &crate::sim::Sim), With<crate::sim::Selected>>,
    mut cam: Query<&mut SimsCamera>,
    mut done: Local<bool>,
    time: Res<Time>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    hung: Query<(&Transform, &crate::paintings::Hung)>,
    easels: Query<(&Transform, &crate::interact::GameObject)>,
    world: Res<crate::loading::CurrentWorld>,
) {
    use crate::inventory::{ItemKind, Stack};
    let Ok(v) = std::env::var("PAINTINGS") else { return };
    if *done || time.elapsed_secs() < 6.0 {
        return;
    }
    let frame = |c: &mut SimsCamera, pos: Vec3, rot: Quat| {
        let fwd = rot * Vec3::Z;
        c.look_at(pos - fwd * 0.5 + Vec3::Y * 1.2);
        c.yaw = fwd.x.atan2(fwd.z);
        c.pitch = 0.12;
        c.distance = 5.0;
        c.height_offset = 2.6;
    };
    if v == "easel" {
        if let (Some((tf, _)), Ok(mut c)) = (easels.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Easel), cam.single_mut()) {
            *done = true;
            let fwd = tf.rotation * Vec3::Z;
            c.look_at(tf.translation + Vec3::Y * 1.0);
            c.yaw = (fwd.x + fwd.z * 0.6).atan2(fwd.z - fwd.x * 0.6);
            c.pitch = 0.3;
            c.distance = 4.5;
            // (Upstairs, maybe: the camera's height is over the ground.)
            c.height_offset = tf.translation.y - world.data.heightmap.sample(tf.translation.x, tf.translation.z) + 1.3;
            info!("paintings test: easel at {:.1?}", tf.translation);
        }
        return;
    }
    if v == "look" {
        *done = true;
        info!("paintings test: {} hanging: {:?}", hung.iter().count(), hung.iter().map(|(_, h)| &h.0.key).collect::<Vec<_>>());
        if let (Some((tf, _)), Ok(mut c)) = (hung.iter().next(), cam.single_mut()) {
            frame(&mut c, tf.translation, tf.rotation);
        }
        return;
    }
    let (Ok((me, tf, sim)), Some(b)) = (sel.single(), building) else { return };
    *done = true;
    let level: u32 = v.parse().unwrap_or(5);
    let pd = &data.0.paintings;
    info!("paintings test: {} pictures, canvases {:?}", pd.paintings.len(), pd.canvases.iter().map(|c| (c.objd, c.uv)).collect::<Vec<_>>());
    let mut rng = rand::rng();
    let items: Vec<Stack> = [0u8, 1, 2, 2, 1, 0]
        .into_iter()
        .map(|size| {
            let p = crate::paintings::paint(Some(pd), size, level, &sim.traits, false, false, &mut rng);
            info!("painted {} ({}, §{})", p.key, p.name, p.worth);
            Stack { kind: ItemKind::Painting, key: p.key, name: p.name.into(), quality: 0, count: 1, worth: p.worth }
        })
        .collect();
    for s in &items[3..] {
        crate::inventory::give(&mut commands, me, s.kind, s.key.clone(), s.name.clone(), 0, s.worth, 1);
    }
    // Three spots on the nearest walls.
    let here = b.local(tf.translation);
    let dist = |w: &&s3bake::types::WallBaked| {
        let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
        let t = ((here - a).dot(c - a) / (c - a).length_squared()).clamp(0.0, 1.0);
        here.distance(a + (c - a) * t)
    };
    let straight: Vec<&s3bake::types::WallBaked> =
        b.data.walls.iter().filter(|w| w.level.max(1) == 1 && ((w.a[0] - w.b[0]).abs() < 0.01 || (w.a[1] - w.b[1]).abs() < 0.01)).collect();
    let y = b.levels.get(1).copied().unwrap_or(0.0);
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    let mut first = None;
    // (With no window behind, two metres apart, facing the same way.)
    let mut near = straight;
    near.sort_by(|x, y| dist(x).total_cmp(&dist(y)));
    let mut spots: Vec<(Vec3, Quat)> = Vec::new();
    for w in near {
        let (a, c) = (Vec2::from(w.a), Vec2::from(w.b));
        let n = (c - a).normalize().perp();
        for side in [1.0, -1.0] {
            let lp = (a + c) / 2.0 + n * 0.4 * side;
            let ray = Ray3d::new(b.world(lp.x, lp.y, y) + Vec3::Y * 5.0, Dir3::NEG_Y);
            if let Some((pos, rot, _)) = crate::build::snap_to_wall(&b, ray, 1)
                && !b.opening_behind(pos, rot, 1)
                && spots.first().is_none_or(|(_, r)| (*r * Vec3::Z).dot(rot * Vec3::Z) > 0.9)
                && spots.iter().all(|(p, _)| p.distance(pos) > 1.8)
            {
                spots.push((pos, rot));
            }
        }
        if spots.len() == 3 {
            break;
        }
    }
    for (s, (pos, rot)) in items[..3].iter().zip(spots) {
        let Some((objd, design)) = crate::paintings::object(pd, s) else { continue };
        if let Some(o) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, rot, Some(design)) {
            commands.entity(o.entity).insert((crate::save::Bought, crate::paintings::Hung(s.clone())));
            first.get_or_insert((pos, rot));
            info!("hung {} at {pos:.1?}", s.key);
        }
    }
    if let (Some((pos, rot)), Ok(mut c)) = (first, cam.single_mut()) {
        frame(&mut c, pos, rot);
    }
    if std::env::var("PAINTINGS_BUY").is_ok()
        && let Some((objd, design)) = crate::paintings::object(pd, &items[3])
    {
        commands.insert_resource(crate::buy::HoldRequest { objd, design: Some(design), item: items[3].clone(), from: me });
    }
}

/// DIVE=1: a diving board on the lot's pool's edge, a couple of metres from its ladder, with the
/// camera on it.
#[allow(clippy::too_many_arguments)]
fn diving_board(
    mut commands: Commands,
    mut done: Local<bool>,
    time: Res<Time>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    ladders: Query<(&Transform, &crate::interact::GameObject)>,
    mut cam: Query<&mut SimsCamera>,
    world: Res<crate::loading::CurrentWorld>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<crate::loading::Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    if std::env::var("DIVE").is_err() || *done || time.elapsed_secs() < 5.0 {
        return;
    }
    let Some(b) = building else { return };
    *done = true;
    // (Near its ladder, or else by one of the pool's tiles.)
    let near = match ladders.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::PoolLadder) {
        Some((ltf, _)) => ltf.translation + ltf.rotation * Vec3::X * 2.5,
        None => {
            let Some(t) = b.data.pool.first() else {
                warn!("dive test: no pool");
                return;
            };
            b.world(t.x as f32 + 0.5, t.z as f32 + 0.5, b.levels.first().copied().unwrap_or(0.0))
        }
    };
    let Some(objd) = data.0.catalog.iter().find(|c| c.instance_name == "DivingBoardClassic").map(|c| c.objd) else { return };
    let Some((pos, rot)) = crate::build::snap_to_pool(&b, near) else {
        warn!("dive test: nowhere on the pool's edge");
        return;
    };
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    if let Some(o) = crate::home::spawn_game_object_design(&mut commands, &mut assets, &mut ctx, &catalog, objd, pos, rot, None) {
        commands.entity(o.entity).insert(crate::save::Bought);
        info!("dive test: a diving board at {pos:.1?}");
    }
    if let Ok(mut c) = cam.single_mut() {
        let fwd = rot * Vec3::Z;
        c.look_at(pos + fwd * 1.5);
        c.yaw = (fwd.x + fwd.z * 0.9).atan2(fwd.z - fwd.x * 0.9) + std::f32::consts::PI;
        c.pitch = 0.35;
        c.distance = 8.0;
        c.height_offset = (pos.y - world.data.heightmap.sample(pos.x, pos.z)).max(0.0) + 0.5;
    }
}

/// BUY_CLOSE_AT=<seconds>: buy mode closed then (with whatever's in hand), and the selected
/// Sim's inventory logged before and after.
fn buy_close(mut commands: Commands, time: Res<Time>, mut state: Local<u8>, mut buy: ResMut<crate::buy::BuyMode>, sel: Query<&crate::inventory::Inventory, With<crate::sim::Selected>>) {
    let Some(at) = std::env::var("BUY_CLOSE_AT").ok().and_then(|v| v.parse::<f32>().ok()) else { return };
    let t = time.elapsed_secs();
    let log = |when: &str| {
        let items: Vec<String> = sel.single().map(|i| i.0.iter().map(|s| format!("{}×{}", s.key, s.count)).collect()).unwrap_or_default();
        info!("buy close test {when}: holding {}, inventory {items:?}", buy.placing.as_ref().map_or("nothing".to_string(), |p| format!("{:?}", p.item.as_ref().map(|i| &i.key))));
    };
    if *state == 0 && t > at {
        log("before");
        buy.active = false;
        buy.drop_tools(&mut commands);
        *state = 1;
    } else if *state == 1 && t > at + 2.0 {
        log("after");
        *state = 2;
    }
}

/// EXHAUST=<first name>: that Sim's energy (or BLADDER_FAIL=<first name>: bladder) runs out,
/// once, a few seconds in.
fn run_out(mut sims: Query<(&crate::sim::Sim, &mut crate::sim::Motives)>, mut done: Local<bool>, time: Res<Time>) {
    if *done || time.elapsed_secs() < 8.0 {
        return;
    }
    for (var, motive) in [("EXHAUST", crate::sim::ENERGY), ("BLADDER_FAIL", crate::sim::BLADDER)] {
        let Ok(who) = std::env::var(var) else { continue };
        *done = true;
        for (s, mut m) in &mut sims {
            if s.first == who {
                m.0[motive] = -100.0;
            }
        }
    }
}

fn keep_hungry(mut sims: Query<(&crate::sim::Sim, &mut crate::sim::Motives)>) {
    let Ok(who) = std::env::var("HUNGRY") else { return };
    for (s, mut m) in &mut sims {
        if s.first == who {
            m.0[crate::sim::HUNGER] = -100.0;
        }
    }
}

/// ASK=<lifetime|career:<career>>: once play is under way, the selected Sim is asked to pick a
/// lifetime wish, or is put just past the branch of that career and asked which path to take.
fn ask_question(
    mut commands: Commands,
    time: Res<Time>,
    mut done: Local<bool>,
    mut questions: ResMut<crate::dialog::Questions>,
    ui: Option<Res<crate::icons::GameUi>>,
    sel: Query<(Entity, &crate::sim::Sim), With<crate::sim::Selected>>,
    mut wishes: Query<&mut crate::wishes::Wishes>,
) {
    // RECIPE=<key>: the selected Sim's next meal is that recipe.
    if let (Ok(key), Some(ui)) = (std::env::var("RECIPE"), ui.as_ref())
        && let Ok((e, _)) = sel.single()
        && !*done
        && let Some(r) = ui.data.recipes.iter().position(|r| r.key == key)
    {
        commands.entity(e).insert(crate::meals::MealPlan(r));
    }
    // LTW=<name>: the selected Sim's lifetime wish.
    if let Ok(name) = std::env::var("LTW")
        && let Ok((e, _)) = sel.single()
        && !*done
        && time.elapsed_secs() > 4.0
        && let Some(i) = crate::lifetime::LIFETIME_WISHES.iter().position(|w| w.name.eq_ignore_ascii_case(&name))
    {
        *done = true;
        commands.entity(e).insert(crate::lifetime::LifetimeWish::new(i));
    }
    let Ok(what) = std::env::var("ASK") else { return };
    if *done || time.elapsed_secs() < 3.0 {
        return;
    }
    let Ok((e, sim)) = sel.single() else { return };
    // LTH=<points>: lifetime happiness to spend.
    if what == "rewards" {
        if let (Some(ui), Ok(mut w)) = (ui.as_ref(), wishes.get_mut(e)) {
            *done = true;
            if let Some(n) = std::env::var("LTH").ok().and_then(|n| n.parse().ok()) {
                w.points = n;
            }
            crate::wishes::ask_reward(&mut questions, &ui.data, e, sim, &w);
        }
        return;
    }
    *done = true;
    if what == "lifetime" {
        crate::lifetime::ask_lifetime_wish(&mut questions, ui.as_ref().map(|u| &*u.data), e, sim);
        commands.entity(e).remove::<crate::lifetime::LifetimeWish>().insert(crate::lifetime::ChoosingLifetimeWish);
    } else if let Some(name) = what.strip_prefix("career:")
        && let Some(track) = crate::careers::careers().iter().position(|c| c.name == name)
        && let Some(at) = crate::careers::careers()[track].branch_at
    {
        let mut job = crate::careers::Job::new(track);
        job.level = at;
        questions.ask_career_path(e, sim, &job);
        commands.entity(e).insert(job);
    }
}

/// WATCH_INSECT=<distance>: the camera keeps to the first butterfly or beetle about.
fn watch_insect(
    mut q: Query<&mut SimsCamera>,
    insects: Query<(&GlobalTransform, &crate::interact::GameObject), With<crate::collecting::Insect>>,
    objects: Query<(&GlobalTransform, &crate::interact::GameObject, (Has<crate::fireplace::Lit>, Has<crate::effects::TvOn>), &crate::interact::UsedBy)>,
) {
    // WATCH_KIND=<kind>,<distance>[,<yaw>[,<pitch>]]: the camera on the first object of that kind.
    if let Some((kind, d, yaw, pitch)) = std::env::var("WATCH_KIND").ok().and_then(|v| {
        let mut it = v.split(',');
        let (k, d) = (it.next()?.to_string(), it.next()?.parse::<f32>().ok()?);
        Some((k, d, it.next().and_then(|y| y.parse::<f32>().ok()), it.next().and_then(|p| p.parse::<f32>().ok())))
    }) {
        // (One in use, or a lit fireplace, first.)
        let mut of_kind: Vec<_> = objects.iter().filter(|(_, o, _, _)| format!("{:?}", o.kind) == kind || o.name.contains(&kind)).collect();
        of_kind.sort_by_key(|(_, _, (lit, on), used)| (used.0.is_none() && !*on, !*lit));
        if let (Ok(mut cam), Some((g, _, _, _))) = (q.single_mut(), of_kind.first()) {
            cam.look_at(g.translation());
            cam.distance = d;
            if let Some(y) = yaw {
                cam.yaw = y;
            }
            if let Some(p) = pitch {
                cam.pitch = p;
            }
        }
        return;
    }
    let Some(d) = std::env::var("WATCH_INSECT").ok().and_then(|v| v.parse::<f32>().ok()) else { return };
    // (Butterflies first.)
    let first = insects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Butterfly).or_else(|| insects.iter().next()).map(|(g, _)| g);
    let (Ok(mut cam), Some(g)) = (q.single_mut(), first) else { return };
    cam.look_at(g.translation());
    cam.distance = d;
}

/// Applies `--cam` shortly after play starts (after the move-in camera placement).
fn apply_cam(args: Res<AutoArgs>, time: Res<Time>, mut since: Local<Option<f32>>, mut done: Local<bool>, mut q: Query<&mut SimsCamera>, world: Option<Res<crate::loading::CurrentWorld>>) {
    let Some(c) = args.cam else { return };
    if *done {
        return;
    }
    // (Held for the first seconds of play, over the move-in camera.)
    let t0 = *since.get_or_insert(time.elapsed_secs());
    if let Ok(mut cam) = q.single_mut() {
        // (At the ground there.)
        let y = world.as_ref().map_or(0.0, |w| w.data.heightmap.sample(c[0], c[1]));
        cam.look_at(Vec3::new(c[0], y, c[1]));
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
    // SHOWROOM_NAMES=<a,b,...>: just those catalogue objects (by internal name).
    let names: Option<Vec<String>> = std::env::var("SHOWROOM_NAMES").ok().map(|v| v.split(',').map(|s| s.to_ascii_lowercase()).collect());
    let keys: Vec<s3bake::Key> =
        data.0.catalog.iter().filter(|c| names.as_ref().is_none_or(|n| n.contains(&c.instance_name.to_ascii_lowercase()))).map(|c| c.objd).collect();
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

/// POOL_MAP=1 logs the lot's floors and pools as a map, a row of tiles a line.
/// FENCE=<x0>,<z0>,<x1>,<z1>[,<style>]: a fence put up along the grid between those lot points
/// (the style an index into the game's fences); FENCE_CAM=1 then looks at it (FENCE_CAM=<n>: from n metres).
#[allow(clippy::too_many_arguments)]
fn auto_fence(
    mut commands: Commands,
    time: Res<Time>,
    mut done: Local<bool>,
    mut building: Option<ResMut<crate::building::ActiveBuilding>>,
    (data, mut assets, ui): (Res<crate::baked::Baked>, ResMut<crate::objects::ObjectAssets>, Option<Res<crate::icons::GameUi>>),
    (mut meshes, mut images, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut faces: Query<(&crate::building::WallFace, &mut MeshMaterial3d<StandardMaterial>)>,
    (mut log, mut grid): (Option<ResMut<crate::building::LotPaint>>, Option<ResMut<crate::nav::NavGrid>>),
    mut cam: Query<&mut SimsCamera>,
    catalog: Res<crate::loading::Catalog>,
) {
    // POOL=<x0>,<z0>,<x1>,<z1>: a pool dug over those lot tiles instead (POOL_LADDER=1: with a
    // ladder at its first edge).
    let pool = std::env::var("POOL").ok();
    let Some(v) = std::env::var("FENCE").ok().or(pool.clone()) else { return };
    if *done || time.elapsed_secs() < 4.0 {
        return;
    }
    let (Some(b), Some(ui)) = (building.as_deref_mut(), ui) else { return };
    *done = true;
    let n: Vec<i32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if n.len() < 4 {
        return;
    }
    let (ops, cost) = if pool.is_some() {
        crate::build::plan_pool(b, false, b.view_level, IVec2::new(n[0], n[1]), IVec2::new(n[2], n[3]))
    } else {
        let Some(style) = ui.data.fences.get(n.get(4).copied().unwrap_or(0) as usize) else { return };
        crate::build::plan_fence(b, style, false, b.view_level, IVec2::new(n[0], n[1]), IVec2::new(n[2], n[3]))
    };
    let (mut lo, mut hi) = (IVec2::MAX, IVec2::MIN);
    for f in &b.data.floors {
        lo = lo.min(IVec2::new(f.x as i32, f.z as i32));
        hi = hi.max(IVec2::new(f.x as i32, f.z as i32));
    }
    info!("build test: {} ops, §{cost}; lot {}x{}, floors {lo}..{hi}", ops.len(), b.data.width, b.data.depth);
    if std::env::var("POOL_MAP").is_ok() {

        for z in 0..b.data.depth as u16 {
            let row: String = (0..b.data.width as u16)
                .map(|x| match (b.data.pool.iter().any(|f| f.x == x && f.z == z), b.data.floors.iter().filter(|f| f.x == x && f.z == z).map(|f| f.level).max()) {
                    (true, _) => '~',
                    (_, Some(l)) => char::from_digit(l as u32, 10).unwrap_or('#'),
                    _ => '.',
                })
                .collect();
            info!("map {z:2} {row}");
        }
    }
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut materials };
    crate::building::repaint(&mut commands, b, &mut assets, &mut ctx, &ops, &mut faces);
    match log.as_mut() {
        Some(l) => l.0.extend(ops),
        None => commands.insert_resource(crate::building::LotPaint(ops)),
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
    if pool.is_some()
        && std::env::var("POOL_LADDER").is_ok()
        && let Some(e) = catalog.entries.iter().filter(|e| e.price > 0 && e.kind == crate::interact::ObjectKind::PoolLadder).min_by_key(|e| e.price)
        // At the pool's near edge, facing into the water.
        && let Some((at, face)) = crate::build::snap_to_pool(b, b.world(n[0] as f32 + 0.5, n[1] as f32, b.levels[0]))
    {
        if let Some(o) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, e.key, at, face) {
            commands.entity(o.entity).insert(crate::save::Bought);
        }
        info!("build test: {} at the pool, {at}", e.name);
    }
    if let Ok(d) = std::env::var("FENCE_CAM")
        && let Ok(mut c) = cam.single_mut()
    {
        let mid = b.world((n[0] + n[2]) as f32 * 0.5, (n[1] + n[3]) as f32 * 0.5, 0.0);
        c.focus = Vec3::new(mid.x, c.focus.y, mid.z);
        c.distance = d.parse().unwrap_or(14.0);
    }
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
            // A painting on the back wall and a sconce on the left one, inside.
            for (name, lx, lz) in [("PaintingMission", x as f32 + w as f32 * 0.5, (z + d) as f32 - 0.3), ("LightingWallSconceCountry", x as f32 + 0.3, z as f32 + d as f32 * 0.5 + 1.0)] {
                let Some(key) = data.0.catalog.iter().find(|c| c.instance_name == name).map(|c| c.objd) else { continue };
                let Some((pos, rot, _)) = crate::build::snap_to_wall(b, down(lx, lz, b), 1) else {
                    warn!("build test: no wall for the {name}");
                    continue;
                };
                info!("build test: {name} hung at {:.2},{:.2},{:.2}", pos.x, pos.y, pos.z);
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
    // (Once the Sim asked for is the one selected.)
    if let Some(want) = &args.select
        && !sel_e.single().ok().and_then(|e| members.get(e).ok()).is_some_and(|(_, s)| s.first.eq_ignore_ascii_case(want))
    {
        return;
    }
    // (Once everyone has settled in.)
    let t0 = *since.get_or_insert(time.elapsed_secs());
    if (matches!(name.as_str(), "Meal" | "Die" | "Starve" | "Shock" | "Electrocute") || name.starts_with("Visit")) && time.elapsed_secs() - t0 < 4.0 {
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
                info!("care test: {social} queued on {target:?}");
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
    // "AllOut": every teen and grown-up heads out for four hours (the little ones stay home).
    if name == "AllOut" {
        commands.queue(|w: &mut World| {
            let now = w.resource::<crate::clock::GameClock>().minutes;
            let mut q = w.query_filtered::<(Entity, &crate::sim::Sim), With<crate::sim::HouseholdMember>>();
            let out: Vec<Entity> = q.iter(w).filter(|(_, s)| s.age == crate::sim::Age::Teen || s.age.is_grown()).map(|(e, _)| e).collect();
            for e in out {
                w.entity_mut(e).insert((
                    crate::rabbitholes::AtRabbitHole { lot: usize::MAX, activity: &crate::rabbitholes::SCHOOL, inside_from: now, until: now + 240.0, place: "town".into() },
                    Visibility::Hidden,
                ));
            }
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
    // "Adopt:<0|1|2>": the selected Sim phones to adopt a baby, toddler or child (a girl).
    if let Some(a) = name.strip_prefix("Adopt:").and_then(|a| a.parse::<u8>().ok()) {
        if let Ok(mut q) = sel.single_mut() {
            *done = true;
            q.push_player(crate::interact::Action::new("Adopt", crate::interact::ActionKind::Adopt { age: a, female: true }, false));
        }
        return;
    }
    // "Move:<lot name part>": the selected Sim phones to move house (the lot's picked below).
    if name.starts_with("Move:") {
        if let Ok(mut q) = sel.single_mut() {
            *done = true;
            q.push_player(crate::interact::Action::new("Move", crate::interact::ActionKind::MoveHouse, false));
        }
        return;
    }
    // "BuyRecipe:<recipe key>": the selected Sim goes to the bookstore for that recipe book.
    if let (Some(key), Some(ui)) = (name.strip_prefix("BuyRecipe:"), ui.as_ref()) {
        let r = ui.data.recipes.iter().position(|r| r.key == key);
        let lot = (0..world.data.lots.len()).find(|&l| crate::opportunities::lot_types(&world.data, l).contains(&"Bookstore"));
        if let (Some(r), Some(lot), Ok(mut q)) = (r, lot, sel.single_mut()) {
            *done = true;
            info!("buying the recipe book for {key} at lot {lot}");
            q.push_player(crate::interact::Action::new("Buy a Recipe Book", crate::interact::ActionKind::Visit { lot, activity: crate::meals::RECIPE_TASK + r }, false));
        }
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
        commands.insert_resource(crate::gardening::Garden { seeds: [(tomato, 2)].into_iter().collect(), ..default() });
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
    // "Repairman": a shower breaks and the selected Sim phones the repairman; "Maid": they hire
    // a maid; "Pizza": they order a pizza.
    if let Some(kind) = match name.as_str() {
        "Repairman" => Some(crate::interact::ActionKind::CallRepairman),
        "Maid" => Some(crate::interact::ActionKind::HireMaid(true)),
        "Pizza" => Some(crate::interact::ActionKind::OrderPizza),
        _ => None,
    } {
        // (A shower, a TV, a computer, a sink, a stereo and a toilet break.)
        if name == "Repairman" {
            use crate::interact::ObjectKind as K;
            for kind in [K::Shower, K::Tv, K::Computer, K::Sink, K::Stereo, K::Toilet] {
                if let Some((e, _)) = objects.iter().find(|(_, o)| o.kind == kind) {
                    commands.entity(e).insert(crate::interact::Broken);
                }
            }
        }
        // (The maid's hired straight away too, should the call be cut short.)
        if name == "Maid" {
            commands.queue(|w: &mut World| w.resource_mut::<crate::services::MaidService>().hired = true);
        }
        if let Ok(mut q) = sel.single_mut() {
            q.push_player(crate::interact::Action::new(name.clone(), kind, false));
        }
        *done = true;
        return;
    }
    // "Upgrade": the selected Sim (made handy) makes the shower unbreakable and gives it a water
    // heater.
    if name == "Upgrade" {
        if let (Ok(me), Some((shower, _))) = (sel_e.single(), objects.iter().find(|(_, o)| o.kind == crate::interact::ObjectKind::Shower)) {
            commands.queue(move |w: &mut World| {
                if let Some(mut s) = w.get_mut::<crate::interact::Skills>(me) {
                    s.0.insert("Handiness", 6.0);
                }
            });
            if let Ok(mut q) = sel.single_mut() {
                for u in crate::upgrades::Upgrade::ALL {
                    q.push_player(crate::interact::Action::new("Upgrade", crate::interact::ActionKind::Upgrade { target: shower, bit: u.bit() }, false));
                }
            }
        }
        *done = true;
        return;
    }
    // "Mail": bills for §123 go out with the mail carrier.
    if name == "Mail" {
        commands.insert_resource(crate::services::MailDue(crate::interact::Bill { amount: 123, day: 0 }));
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
    // "Call": the selected Sim phones the Sim they know best (who isn't about) for a chat.
    if name == "Call" {
        let me = sel_e.single().unwrap();
        commands.queue(move |w: &mut World| {
            let mut away = w.query_filtered::<Entity, With<crate::interact::OffLot>>();
            let away: Vec<Entity> = away.iter(w).collect();
            let Some(rels) = w.get::<crate::social::Relationships>(me) else { return };
            let Some(target) = away.into_iter().filter(|e| rels.0.contains_key(e)).max_by(|a, b| rels.friendship(*a).total_cmp(&rels.friendship(*b))) else {
                warn!("call test: nobody to call");
                return;
            };
            let name = w.get::<crate::sim::Sim>(target).map(|s| s.full_name()).unwrap_or_default();
            let before = rels.friendship(target);
            info!("call test: phoning {name} (friendship {before:.1})");
            if let Some(mut q) = w.get_mut::<crate::interact::ActionQueue>(me) {
                q.push_player(crate::interact::Action::new(&format!("Chat with {name}"), crate::interact::ActionKind::PhoneChat { target }, false));
            }
        });
        *done = true;
        return;
    }
    // "Retire": the selected Sim retires from their career, on a pension.
    if name == "Retire" {
        if let Ok(mut q) = sel.single_mut() {
            q.push_player(crate::interact::Action::new("Retire", crate::interact::ActionKind::Retire, false));
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
    // "Starve": the selected Sim dies of hunger. "Shock": an electric shock (a second one kills).
    if name == "Starve" {
        let me = sel_e.single().unwrap();
        commands.entity(me).insert(crate::death::Dying::starved());
        *done = true;
        return;
    }
    if name == "Shock" || name == "Electrocute" {
        let me = sel_e.single().unwrap();
        if name == "Shock" {
            commands.entity(me).insert(crate::death::Shocked);
        } else {
            commands.entity(me).insert(crate::death::Dying::electrocuted());
        }
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
    // ("Paint: Large Canvas": on that canvas.)
    let canvas = name.strip_prefix("Paint: ").and_then(|c| crate::paintings::CANVASES.iter().position(|x| x.eq_ignore_ascii_case(c)));
    let def_name = if canvas.is_some() { "Paint" } else { name.as_str() };
    for (e, o) in &objects {
        if let Some(i) = crate::interact::interactions_for(o.kind).iter().position(|d| d.name.eq_ignore_ascii_case(def_name)) {
            if let (Some(c), Ok(me)) = (canvas, sel_e.single()) {
                commands.entity(me).insert(crate::paintings::PaintPlan::new(c as u8));
            }
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
            // Pick the second lifetime wish on offer.
            if let Some((mut i, _)) = cas.iter_mut().filter(|(_, a)| matches!(a, crate::home::CasAction::LifetimeWish(_))).nth(1) {
                *i = Interaction::Pressed;
            }
            *stage = (69, now);
        }
        (69, AppState::CreateHousehold, _) if since > 1.0 => {
            shot(&mut commands, "2b_lifetime_wish");
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
            *stage = (86, now);
        }
        // Dressing the formal wear: its tops, and the third of them worn.
        (86, AppState::CreateHousehold, _) if since > 1.0 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| **a == crate::home::CasAction::Wear(1)) {
                *i = Interaction::Pressed;
            }
            *stage = (80, now);
        }
        (80, AppState::CreateHousehold, _) if since > 1.5 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| **a == crate::home::CasAction::Pick(2)) {
                *i = Interaction::Pressed;
            }
            *stage = (81, now);
        }
        (81, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2f_formal");
            *stage = (84, now);
        }
        (84, AppState::CreateHousehold, _) if since > 1.0 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| **a == crate::home::CasAction::Wear(4)) {
                *i = Interaction::Pressed;
            }
            *stage = (82, now);
        }
        (82, AppState::CreateHousehold, _) if since > 1.5 => {
            shot(&mut commands, "2s_swimwear");
            *stage = (85, now);
        }
        (85, AppState::CreateHousehold, _) if since > 1.0 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| **a == crate::home::CasAction::Wear(0)) {
                *i = Interaction::Pressed;
            }
            *stage = (83, now);
        }
        (83, AppState::CreateHousehold, _) if since > 0.8 => {
            // The second Sim's face: a beard, glasses and green eyes.
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Select(1))) {
                *i = Interaction::Pressed;
            }
            *stage = (59, now);
        }
        (59, AppState::CreateHousehold, _) if since > 0.8 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::Tab(crate::cas::CasTab::Face))) {
                *i = Interaction::Pressed;
            }
            *stage = (64, now);
        }
        (64, AppState::CreateHousehold, _) if since > 1.0 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::FacePart(16, 3))) {
                *i = Interaction::Pressed;
            }
            *stage = (63, now);
        }
        (63, AppState::CreateHousehold, _) if since > 1.0 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::FacePart(12, 2))) {
                *i = Interaction::Pressed;
            }
            *stage = (58, now);
        }
        (58, AppState::CreateHousehold, _) if since > 1.0 => {
            if let Some((mut i, _)) = cas.iter_mut().find(|(_, a)| matches!(a, crate::home::CasAction::EyeColor(3))) {
                *i = Interaction::Pressed;
            }
            *stage = (62, now);
        }
        (62, AppState::CreateHousehold, _) if since > 2.0 => {
            shot(&mut commands, "2f_face");
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

fn auto_speed(mut commands: Commands, args: Res<AutoArgs>, mut clock: ResMut<crate::clock::GameClock>, mut done: Local<(bool, bool)>, pending: Option<Res<crate::save::PendingLoad>>, live: Option<Res<State<crate::PlayMode>>>) {
    if !done.0 {
        done.0 = true;
        if let Some(s) = args.speed {
            clock.speed = s.min(3);
            // (Left on its own, the game answers its own questions and turns opportunities
            // down, rather than standing still waiting for an answer.)
            let opp = args.action.as_deref().is_some_and(|a| a.starts_with("Opp"));
            commands.insert_resource(crate::dialog::AutoAnswer(0));
            if !opp {
                commands.insert_resource(crate::opportunities::AutoDecline);
            }
        }
    }
    // The hour of the day (on the save's own day, once a saved game has loaded).
    if !done.1 && pending.is_none() && live.is_some_and(|s| *s.get() == crate::PlayMode::Live) {
        done.1 = true;
        if let Some(h) = args.hour {
            clock.minutes = (clock.minutes / 1440.0).floor() * 1440.0 + h * 60.0;
        }
    }
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
    if let Some((p, g)) = crate::save::list_saves().into_iter().nth(n)
        && crate::save::begin_load(&mut commands, &worlds, g, Some(p))
    {
        next.set(AppState::Loading);
    }
}
