//! Saving and loading games: the household (each Sim's looks, personality, needs, skills,
//! moodlets, career and relationships), visitors they know, money, the time, and the changes
//! made to the lot in buy mode. Saves are JSON files in `saves/`.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::clock::GameClock;
use crate::interact::*;
use crate::life::{MoodletKind, Moodlets, Trait};
use crate::loading::{Catalog, CurrentWorld};
use crate::nav::Floor;
use crate::sim::*;
use crate::social::{RelStatus, Relationships};
use crate::PlayMode;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemovedLotObjects>()
            .add_message::<SaveRequest>()
            .add_systems(Update, resume_saved_lot.run_if(in_state(PlayMode::ChooseLot)))
            .add_systems(Update, (apply_loaded_game, save_game).run_if(in_state(PlayMode::Live)));
    }
}

pub const SAVE_VERSION: u32 = 1;

pub fn saves_dir() -> PathBuf {
    std::env::var_os("SIMS3_SAVES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("saves"))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedSim {
    pub id: u64,
    pub look: u64,
    pub first: String,
    pub last: String,
    pub female: bool,
    pub age: String,
    pub traits: Vec<String>,
    pub skin: [f32; 3],
    pub hair: [f32; 3],
    pub top: [f32; 3],
    pub bottom: [f32; 3],
    pub member: bool,
    pub selected: bool,
    /// "home", "visiting" or "away".
    pub whereabouts: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub floor: u8,
    pub motives: [f32; 6],
    pub skills: Vec<(String, f32)>,
    pub moodlets: Vec<(String, f64)>,
    pub job: Option<SavedJob>,
    pub relationships: Vec<SavedRel>,
    #[serde(default)]
    pub lifetime_happiness: u32,
    /// Chosen hair, top, bottom, outfit and shoes.
    #[serde(default)]
    pub outfit: Vec<Option<(u32, u32, u64)>>,
    #[serde(default)]
    pub rewards: Vec<String>,
    /// Days into the current life stage, and how long old age lasts.
    #[serde(default)]
    pub aging: Option<(f32, f32)>,
    /// Pregnant since (game minutes), with the other parent's id and the stage shown so far.
    #[serde(default)]
    pub pregnancy: Option<(f64, Option<u64>, u8)>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedJob {
    pub track: String,
    pub level: usize,
    pub performance: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedRel {
    pub with: u64,
    pub friendship: f32,
    pub romance: f32,
    pub status: String,
    pub kissed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedObject {
    pub objd: (u32, u32, u64),
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveGame {
    pub version: u32,
    pub world: String,
    pub lot_index: usize,
    pub lot_name: String,
    pub household: String,
    pub funds: i64,
    pub last_bill_day: u32,
    pub minutes: f64,
    pub sims: Vec<SavedSim>,
    /// Objects bought in buy mode.
    pub bought: Vec<SavedObject>,
    /// The lot's own furniture that was sold or moved.
    pub removed: Vec<SavedObject>,
}

impl SaveGame {
    fn sim(s: &SavedSim) -> Sim {
        let c = |v: [f32; 3]| Color::srgb(v[0], v[1], v[2]);
        let o = |i: usize| s.outfit.get(i).copied().flatten();
        Sim {
            id: s.id,
            look: s.look,
            outfit: OutfitChoice { hair: o(0), top: o(1), bottom: o(2), full: o(3), shoes: o(4) },
            first: s.first.clone(),
            last: s.last.clone(),
            female: s.female,
            age: match s.age.as_str() {
                "Baby" => Age::Baby,
                "Toddler" => Age::Toddler,
                "Child" => Age::Child,
                "Teen" => Age::Teen,
                "Adult" => Age::Adult,
                "Elder" => Age::Elder,
                _ => Age::YoungAdult,
            },
            traits: s.traits.iter().filter_map(|t| Trait::from_name(t)).collect(),
            skin: c(s.skin),
            hair: c(s.hair),
            top: c(s.top),
            bottom: c(s.bottom),
        }
    }

    pub fn members(&self) -> Vec<Sim> {
        self.sims.iter().filter(|s| s.member).map(Self::sim).collect()
    }

    /// Sims the household knows (visitors and friends living elsewhere).
    pub fn known_sims(&self) -> Vec<Sim> {
        self.sims.iter().filter(|s| !s.member).map(Self::sim).collect()
    }

    pub fn file_name(&self) -> String {
        let clean = |s: &str| s.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>();
        format!("{} - {}.json", clean(&self.household), clean(&self.world))
    }
}

/// A save being loaded: its Sims are built while loading, the rest applied once in play.
#[derive(Resource, Clone)]
pub struct PendingLoad(pub SaveGame);

/// The lot's original furniture removed in buy mode (to remove again when loading).
#[derive(Resource, Default)]
pub struct RemovedLotObjects(pub Vec<SavedObject>);

/// Marks an object the household bought.
#[derive(Component)]
pub struct Bought;

#[derive(Message)]
pub struct SaveRequest;

/// Every save on disk, newest first.
pub fn list_saves() -> Vec<(PathBuf, SaveGame)> {
    let mut out: Vec<(PathBuf, SaveGame, std::time::SystemTime)> = std::fs::read_dir(saves_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()? != "json" {
                return None;
            }
            let g: SaveGame = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            let t = e.metadata().ok()?.modified().ok()?;
            Some((p, g, t))
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2));
    out.into_iter().map(|(p, g, _)| (p, g)).collect()
}

fn rgb(c: Color) -> [f32; 3] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue]
}

fn age_name(a: Age) -> &'static str {
    match a {
        Age::Baby => "Baby",
        Age::Toddler => "Toddler",
        Age::Child => "Child",
        Age::Teen => "Teen",
        Age::YoungAdult => "YoungAdult",
        Age::Adult => "Adult",
        Age::Elder => "Elder",
    }
}

fn status_name(s: RelStatus) -> &'static str {
    match s {
        RelStatus::None => "None",
        RelStatus::Partner => "Partner",
        RelStatus::Engaged => "Engaged",
        RelStatus::Married => "Married",
        RelStatus::Ex => "Ex",
    }
}

fn status_from(s: &str) -> RelStatus {
    match s {
        "Partner" => RelStatus::Partner,
        "Engaged" => RelStatus::Engaged,
        "Married" => RelStatus::Married,
        "Ex" => RelStatus::Ex,
        _ => RelStatus::None,
    }
}

pub const SKILLS: [&str; 7] = ["Athletic", "Charisma", "Cooking", "Guitar", "Logic", "Painting", "Writing"];

fn saved_object(o: &GameObject, tf: &Transform) -> SavedObject {
    SavedObject { objd: o.objd, position: tf.translation.to_array(), rotation: tf.rotation.to_array() }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn save_game(
    mut requests: MessageReader<SaveRequest>,
    clock: Res<GameClock>,
    world: Res<CurrentWorld>,
    household: Option<Res<Household>>,
    removed: Res<RemovedLotObjects>,
    sims: Query<
        (
            Entity,
            &Sim,
            &Transform,
            &Floor,
            &Motives,
            &Skills,
            &Moodlets,
            Option<&Job>,
            &Relationships,
            Has<HouseholdMember>,
            Has<Selected>,
            Has<OffLot>,
            Has<Visitor>,
            Option<&crate::wishes::Wishes>,
            (Option<&crate::aging::Aging>, Option<&crate::little::Pregnancy>),
        ),
        Without<crate::town::Townie>,
    >,
    bought: Query<(&GameObject, &Transform), With<Bought>>,
    mut notes: ResMut<Notifications>,
) {
    if requests.read().count() == 0 {
        return;
    }
    let Some(hh) = household else { return };
    let ids: HashMap<Entity, u64> = sims.iter().map(|q| (q.0, q.1.id)).collect();
    let mut saved = Vec::new();
    for (_, sim, tf, floor, motives, skills, moodlets, job, rels, member, selected, away, visiting, wishes, (aging, pregnancy)) in &sims {
        saved.push(SavedSim {
            id: sim.id,
            look: sim.look,
            first: sim.first.clone(),
            last: sim.last.clone(),
            female: sim.female,
            age: age_name(sim.age).into(),
            traits: sim.traits.iter().map(|t| t.name().to_string()).collect(),
            skin: rgb(sim.skin),
            hair: rgb(sim.hair),
            top: rgb(sim.top),
            bottom: rgb(sim.bottom),
            member,
            selected,
            whereabouts: if member { "home" } else if away { "away" } else if visiting { "visiting" } else { "away" }.into(),
            position: tf.translation.to_array(),
            yaw: tf.rotation.to_euler(EulerRot::YXZ).0,
            floor: floor.0,
            motives: motives.0,
            skills: skills.0.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            moodlets: moodlets.0.iter().filter(|m| m.until.is_finite()).map(|m| (m.kind.def().name.to_string(), m.until)).collect(),
            job: job.map(|j| SavedJob { track: j.career().name.into(), level: j.level, performance: j.performance }),
            relationships: rels
                .0
                .iter()
                .filter_map(|(e, r)| {
                    Some(SavedRel { with: *ids.get(e)?, friendship: r.friendship, romance: r.romance, status: status_name(r.status).into(), kissed: r.kissed })
                })
                .collect(),
            lifetime_happiness: wishes.map_or(0, |w| w.points),
            outfit: vec![sim.outfit.hair, sim.outfit.top, sim.outfit.bottom, sim.outfit.full, sim.outfit.shoes],
            rewards: wishes.map(|w| w.rewards.iter().map(|r| r.name().to_string()).collect()).unwrap_or_default(),
            aging: aging.map(|a| (a.days, a.elder_span)),
            pregnancy: pregnancy.map(|p| (p.since, p.other_parent.and_then(|o| ids.get(&o).copied()), p.stage)),
        });
    }
    let game = SaveGame {
        version: SAVE_VERSION,
        world: world.name.clone(),
        lot_index: hh.lot_index,
        lot_name: world.data.lot_names.get(hh.lot_index).cloned().unwrap_or_default(),
        household: hh.name.clone(),
        funds: hh.funds,
        last_bill_day: hh.last_bill_day,
        minutes: clock.minutes,
        sims: saved,
        bought: bought.iter().map(|(o, tf)| saved_object(o, tf)).collect(),
        removed: removed.0.clone(),
    };
    let dir = saves_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(game.file_name());
    match serde_json::to_vec_pretty(&game).map_err(|e| e.to_string()).and_then(|d| std::fs::write(&path, d).map_err(|e| e.to_string())) {
        Ok(()) => notes.push(format!("Game saved: the {} household in {}.", game.household, game.world)),
        Err(e) => notes.push(format!("Couldn't save the game: {e}")),
    }
}

/// A loaded game skips the lot chooser.
fn resume_saved_lot(
    mut commands: Commands,
    pending: Option<Res<PendingLoad>>,
    mut next: ResMut<NextState<PlayMode>>,
    mut done: Local<bool>,
) {
    let Some(p) = pending else {
        *done = false;
        return;
    };
    if *done {
        return;
    }
    *done = true;
    commands.insert_resource(crate::home::MoveInRequest(p.0.lot_index));
    next.set(PlayMode::Live);
}

/// Once the household is in, restores everything the save remembers.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn apply_loaded_game(
    mut commands: Commands,
    pending: Option<Res<PendingLoad>>,
    mut clock: ResMut<GameClock>,
    mut household: Option<ResMut<Household>>,
    mut removed: ResMut<RemovedLotObjects>,
    mut sims: Query<
        (Entity, &mut Sim, &mut Transform, &mut Floor, &mut Motives, &mut Skills, &mut Moodlets, &mut Relationships),
        (Without<crate::town::Townie>, Without<GameObject>),
    >,
    objects: Query<(Entity, &GameObject, &Transform), (Without<Bought>, Without<Sim>)>,
    (data, catalog, mut assets): (Res<crate::baked::Baked>, Res<Catalog>, ResMut<crate::objects::ObjectAssets>),
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut grid: Option<ResMut<crate::nav::NavGrid>>,
    mut notes: ResMut<Notifications>,
) {
    let Some(p) = pending else { return };
    if household.is_none() || sims.is_empty() {
        return;
    }
    let game = &p.0;
    clock.minutes = game.minutes;
    if let Some(h) = household.as_mut() {
        h.funds = game.funds;
        h.last_bill_day = game.last_bill_day;
        h.name = game.household.clone();
    }
    let by_id: HashMap<u64, Entity> = sims.iter().map(|q| (q.1.id, q.0)).collect();
    for s in &game.sims {
        let Some(&e) = by_id.get(&s.id) else { continue };
        let Ok((_, _, mut tf, mut floor, mut motives, mut skills, mut moodlets, mut rels)) = sims.get_mut(e) else { continue };
        tf.translation = Vec3::from(s.position);
        tf.rotation = Quat::from_rotation_y(s.yaw);
        floor.0 = s.floor.max(1);
        motives.0 = s.motives;
        skills.0 = s.skills.iter().filter_map(|(k, v)| SKILLS.iter().find(|x| *x == k).map(|x| (*x, *v))).collect();
        moodlets.0.clear();
        for (name, until) in &s.moodlets {
            if let Some(k) = MoodletKind::from_name(name) {
                moodlets.0.push(crate::life::Moodlet { kind: k, value: k.def().value, until: *until });
            }
        }
        rels.0.clear();
        for r in &s.relationships {
            if let Some(&other) = by_id.get(&r.with) {
                let x = rels.entry(other);
                x.friendship = r.friendship;
                x.romance = r.romance;
                x.status = status_from(&r.status);
                x.kissed = r.kissed;
            }
        }
        let mut ec = commands.entity(e);
        if let Some((days, elder_span)) = s.aging {
            ec.insert(crate::aging::Aging { days, elder_span });
        }
        if let Some((since, other, stage)) = s.pregnancy {
            let other_parent = other.and_then(|o| by_id.get(&o).copied());
            ec.insert(crate::little::Pregnancy { since, other_parent, stage });
        }
        match &s.job {
            Some(j) => {
                if let Some(track) = crate::careers::careers().iter().position(|c| c.name == j.track) {
                    let mut job = crate::careers::Job::new(track);
                    job.level = j.level.min(crate::careers::careers()[track].levels.len() - 1);
                    job.performance = j.performance;
                    ec.insert(job);
                }
            }
            None => {
                ec.remove::<crate::careers::Job>();
            }
        }
        if s.member {
            ec.insert(HouseholdMember).remove::<(Visitor, OffLot)>();
            ec.insert(crate::wishes::Wishes::restored(
                s.lifetime_happiness,
                s.rewards.iter().filter_map(|r| crate::wishes::Reward::from_name(r)).collect(),
                game.minutes,
            ));
        } else if s.whereabouts == "visiting" {
            ec.insert(Visitor { leave_at: game.minutes + 180.0 });
        } else {
            ec.remove::<Visitor>().insert((OffLot, Visibility::Hidden));
        }
        if s.selected {
            ec.insert(Selected);
        } else {
            ec.remove::<Selected>();
        }
    }
    // Furniture: take away what was sold, put back what was bought.
    for r in &game.removed {
        if let Some((e, _, _)) = objects
            .iter()
            .find(|(_, o, tf)| o.objd == r.objd && tf.translation.distance(Vec3::from(r.position)) < 0.1)
        {
            commands.entity(e).despawn();
        }
    }
    removed.0 = game.removed.clone();
    let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
    for b in &game.bought {
        let rot = Quat::from_array(b.rotation);
        if let Some(o) = crate::home::spawn_game_object_rot(&mut commands, &mut assets, &mut ctx, &catalog, b.objd, Vec3::from(b.position), rot) {
            commands.entity(o.entity).insert(Bought);
        }
    }
    if let Some(g) = grid.as_mut() {
        g.dirty = true;
    }
    notes.0.clear();
    notes.push(format!("Welcome back to the {} household!", game.household));
    commands.remove_resource::<PendingLoad>();
}

/// Starts loading a saved game: the world, then the household.
pub fn begin_load(commands: &mut Commands, worlds: &crate::data::WorldList, game: SaveGame) -> bool {
    let Some(w) = worlds.0.iter().find(|w| w.path.file_stem().is_some_and(|s| s.to_string_lossy() == game.world) || w.name == game.world) else {
        return false;
    };
    commands.insert_resource(crate::data::SelectedWorld(w.clone()));
    commands.insert_resource(crate::home::PendingHousehold { last_name: game.household.clone(), members: game.members(), premade: None });
    commands.insert_resource(PendingLoad(game));
    true
}

/// Records the lot's own furniture being picked up or sold.
pub fn note_removed(removed: &mut RemovedLotObjects, o: &GameObject, tf: &Transform) {
    removed.0.push(saved_object(o, tf));
}

pub fn request_save(w: &mut MessageWriter<SaveRequest>) {
    w.write(SaveRequest);
}
