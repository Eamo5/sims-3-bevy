//! Snake charming, World Adventures' skill (`SnakeCharming`), at the snake charming basket
//! (`SnakeCharmingBasket`): teens and up "Charm" (Snake Charming 650 an hour, Fun 40), sat before
//! it cross-legged with the pungi (`a2o_snakeBasket_start`), the lid off (`_openLid`), playing as
//! their skill has them (the beginner's tunes below level four, the middling ones below eight, the
//! expert's after: `kNormalAnimLevel`, `kExpertAnimLevel`; the better ones to the game's own pungi
//! loops, `snake_pungi_med_lp`, `snake_pungi_exp_lp`). Every seven to twelve minutes
//! (`kMin/MaxTimeToPlayInitially`) the snake may rise (`_trans_snakeappear`: one time in ten for a
//! beginner, a third of the time middling, always for an expert), and once out it sways to the
//! tune, the basket's garter snake or King Cobra its own animated model. Else they reach in to see
//! where it's got to (`_beg_touchSnake_shrug`), bitten one time in ten (`_bite`: Hurt Hand!, and
//! they're done). With the snake out the skilled try a kiss atop its head now and then (a fifth of
//! the time; `_kissSnake_succeed` four times in ten middling, seven expert: Snake Kiss), or miss
//! (`_fail`), and a King Cobra may bite their face (`_epicFail`: Ouch! My Face!). An expert with the
//! snake away now and then makes magic of it, a swirl over the basket. At level eight the basket's
//! garter snake can be traded for a King Cobra ("Purchase King Cobra (§1000)", `PurchaseKingCobra`:
//! "The basket now has a King Cobra in it."). Kept in saves.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::anim::{ActionClip, ClipPlayer};
use crate::interact::{ActionKind, ActionQueue, GameObject, ObjectKind, Phase, Skills};
use crate::sim::Sim;

pub struct SnakeCharmPlugin;

impl Plugin for SnakeCharmPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (charm, snake_moves).chain().after(crate::interact::run_actions).before(crate::anim::drive_skeletons).run_if(in_state(PlayMode::Live)))
            .add_systems(Update, (lids_on, charm_test).run_if(in_state(PlayMode::Live)));
    }
}

/// A basket at rest, its lid on (the last of `closeLid`'s own half).
const LID_ON: &str = "a2o_snakeBasket_closeLid_snakeBasket";

/// Baskets keep their lids on while nobody's charming.
fn lids_on(mut commands: Commands, baskets: Query<(Entity, &GameObject), Without<crate::objanim::HeldPose>>) {
    for (e, o) in &baskets {
        if o.kind == ObjectKind::SnakeBasket {
            commands.entity(e).insert(crate::objanim::HeldPose(LID_ON));
        }
    }
}

/// The game's words.
pub const CHARM: &str = "Charm";
pub const BUY_COBRA: &str = "Purchase King Cobra";

/// The game's tuning.
const NORMAL_LEVEL: f32 = 4.0;
const EXPERT_LEVEL: f32 = 8.0;
const PLAY_MIN: f64 = 7.0;
const PLAY_MAX: f64 = 12.0;
const SNAKE_LOW: f64 = 0.1;
const SNAKE_MED: f64 = 0.35;
const HURT_HAND: f64 = 0.1;
const KISS: f64 = 0.2;
const KISS_WON_MED: f64 = 0.4;
const KISS_WON_HIGH: f64 = 0.7;
const KISS_BITE_MED: f64 = 0.1;
const KISS_BITE_HIGH: f64 = 0.02;
const MAGIC: f64 = 0.2;
pub const COBRA_LEVEL: f32 = 8.0;
pub const COBRA_COST: i64 = 1000;

/// How skilled they play (`GetSkillLevelParameterForJazzGraph`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Band {
    Beginner,
    Middling,
    Expert,
}

impl Band {
    pub fn of(level: f32) -> Band {
        if level >= EXPERT_LEVEL {
            Band::Expert
        } else if level >= NORMAL_LEVEL {
            Band::Middling
        } else {
            Band::Beginner
        }
    }

    /// Its tunes with the snake away, and with it out.
    fn playing(self, snake_out: bool) -> &'static [&'static str] {
        match (self, snake_out) {
            (Band::Beginner, false) => &["a2o_snakeBasket_beg_loopPlay_x"],
            (Band::Beginner, true) => &["a2o_snakeBasket_beg_loop1_x", "a2o_snakeBasket_beg_loop2_x"],
            (Band::Middling, _) => &["a2o_snakeBasket_med_loop1_x", "a2o_snakeBasket_med_play_loop2_x"],
            (Band::Expert, _) => &["a2o_snakeBasket_exp_play_loop1_x"],
        }
    }

    /// Its pungi's own loop (the beginner's tunes are their clips' own).
    fn sound(self) -> Option<&'static str> {
        match self {
            Band::Beginner => None,
            Band::Middling => Some("snake_pungi_med_lp"),
            Band::Expert => Some("snake_pungi_exp_lp"),
        }
    }

    /// How likely the snake's to rise.
    fn snake_chance(self) -> f64 {
        match self {
            Band::Beginner => SNAKE_LOW,
            Band::Middling => SNAKE_MED,
            Band::Expert => 1.0,
        }
    }
}

/// How a kiss goes (`DoSnakeLoop`): none tried, kissed, bitten (a cobra's), missed. Rolls 0 to 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kiss {
    None,
    Won,
    Bitten,
    Missed,
}

pub fn kiss(band: Band, cobra: bool, try_roll: f64, won_roll: f64, bite_roll: f64) -> Kiss {
    if band == Band::Beginner || try_roll >= KISS {
        return Kiss::None;
    }
    let (won, bite) = if band == Band::Expert { (KISS_WON_HIGH, KISS_BITE_HIGH) } else { (KISS_WON_MED, KISS_BITE_MED) };
    if won_roll < won {
        Kiss::Won
    } else if cobra && bite_roll < bite {
        Kiss::Bitten
    } else {
        Kiss::Missed
    }
}

/// A basket with a King Cobra in it.
#[derive(Component, Clone, Copy, Debug)]
pub struct KingCobra;

/// The King Cobra bought (`PurchaseKingCobra`): the money paid, the snake swapped.
pub fn buy_cobra(w: &mut World, sim: Entity, basket: Entity) {
    let Some(mut house) = w.get_resource_mut::<crate::interact::Household>() else { return };
    if house.funds < COBRA_COST {
        return;
    }
    house.funds -= COBRA_COST;
    w.entity_mut(basket).insert(KingCobra);
    let first = w.get::<Sim>(sim).map(|s| s.first.clone()).unwrap_or_default();
    if let Some(mut n) = w.get_resource_mut::<crate::interact::Notifications>() {
        n.push("The basket now has a King Cobra in it.".to_string());
    }
    info!("snake charming: {first} bought a King Cobra for the basket");
}

/// Whether the King Cobra's to be had, and why not (`PurchaseKingCobra`'s test).
pub fn cobra_offer(level: f32, has_cobra: bool, funds: i64, first: &str) -> Option<Result<String, String>> {
    if level < COBRA_LEVEL || has_cobra {
        return None;
    }
    let label = format!("{BUY_COBRA} (§{COBRA_COST})");
    Some(if funds < COBRA_COST { Err(format!("{first} does not have enough money to purchase a cobra.")) } else { Ok(label) })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    /// Playing, the snake away, till the next try.
    Playing,
    /// Reaching in after it.
    Reaching,
    /// The snake rising.
    Rising,
    /// The snake out, swaying.
    Swaying,
    /// A kiss tried.
    Kissing,
}

/// At a basket: how they play, how far along, till when, the snake (its model) and whether it's
/// out.
#[derive(Component)]
pub struct Charming {
    basket: Entity,
    band: Band,
    stage: Stage,
    t: f32,
    len: f32,
    until: f64,
    snake: Option<Entity>,
    out: bool,
    settled: bool,
    quit: bool,
}

/// A basket's snake: its model's own clip, as its charmer's.
#[derive(Component)]
struct Snake;

fn clip_len(lib: &mut crate::anim::ClipLibrary, data: &crate::baked::Baked, name: &str) -> f32 {
    lib.get(data, name).map_or(2.0, |c| c.duration)
}

/// Charming the snake, step by step.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn charm(
    mut commands: Commands,
    (time, clock): (Res<Time>, Res<crate::clock::GameClock>),
    (data, mut lib): (Res<crate::baked::Baked>, ResMut<crate::anim::ClipLibrary>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut sims: Query<(Entity, &Sim, &mut ActionQueue, &Skills, Option<&mut Charming>, Option<&mut crate::life::Moodlets>, Option<&ClipPlayer>)>,
    baskets: Query<(&GameObject, &GlobalTransform, Has<KingCobra>)>,
    (mut sparkles, mut sounds): (MessageWriter<crate::effects::SpinSparkle>, MessageWriter<crate::sound::PlaySound>),
) {
    let dt = time.delta_secs().min(0.1) * crate::clock::SPEED_RATES[clock.speed];
    let now = clock.minutes;
    let mut rng = rand::rng();
    let forced = std::env::var("CHARM_FORCE").ok();
    for (e, sim, mut q, skills, charming, moodlets, player) in &mut sims {
        let stopping = q.0.front().is_some_and(|a| a.cancel);
        let at = q.0.front().filter(|a| matches!(a.phase, Phase::Running(_))).and_then(|a| match a.kind {
            ActionKind::Object { target, def } => baskets
                .get(target)
                .ok()
                .filter(|b| b.0.kind == ObjectKind::SnakeBasket && crate::interact::interactions_for(ObjectKind::SnakeBasket).get(def).is_some_and(|d| d.name == CHARM))
                .map(|_| target),
            _ => None,
        });
        let Some(basket) = at else {
            // (Putting it away, the snake sinking as the lid goes back on: kept till that's done.)
            let finishing = charming.as_ref().is_some_and(|c| q.0.front().is_some_and(|a| matches!(a.kind, ActionKind::Outro { target, .. } if target == c.basket)));
            if let Some(c) = charming.as_ref().filter(|_| finishing) {
                commands.entity(c.basket).remove::<crate::sound::AmbientSound>();
                continue;
            }
            // (Done: the snake back in its basket, the pungi's tune stopped.)
            if let Some(c) = charming {
                if let Some(s) = c.snake {
                    commands.entity(s).try_despawn();
                }
                commands.entity(c.basket).remove::<crate::sound::AmbientSound>();
                commands.entity(e).remove::<Charming>();
            }
            continue;
        };
        let Ok((_, _, cobra)) = baskets.get(basket) else { continue };
        let level = skills.0.get("SnakeCharming").copied().unwrap_or(0.0);
        let band = Band::of(level);
        // (Putting it away: the lid back on over the snake, the tune stopped.)
        if stopping {
            if charming.is_some() {
                commands.entity(basket).remove::<crate::sound::AmbientSound>();
            }
            continue;
        }
        let Some(mut c) = charming else {
            // (The snake, the basket's own, waiting in it.)
            let model = if cobra { "SnakeCobra" } else { "SnakeGarter" };
            let snake = data.0.catalog.iter().find(|x| x.instance_name.eq_ignore_ascii_case(model)).map(|x| x.objd).and_then(|objd| {
                let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let parts = assets.object(&mut ctx, objd);
                (!parts.is_empty()).then(|| {
                    let s = crate::objects::spawn_parts(&mut commands, &parts, Transform::IDENTITY);
                    commands.entity(s).insert((Snake, Visibility::Hidden));
                    commands.entity(basket).add_child(s);
                    s
                })
            });
            commands.entity(e).insert((
                Charming { basket, band, stage: Stage::Playing, t: 0.0, len: 0.0, until: f64::MAX, snake, out: false, settled: false, quit: false },
                ActionClip::steps("a2o_snakeBasket_start_x", &["a2o_snakeBasket_openLid_x"], band.playing(false)).ending(&["a2o_snakeBasket_closeLid_x", "a2o_snakeBasket_stop_x"], 4.3),
            ));
            if let Some(s) = band.sound() {
                commands.entity(basket).insert(crate::sound::AmbientSound(s.into()));
            }
            info!("snake charming: {} charms ({band:?})", sim.first);
            continue;
        };
        // (A level gained into another band: its tunes.)
        if c.band != band && matches!(c.stage, Stage::Playing | Stage::Swaying) {
            c.band = band;
            commands.entity(e).insert(ActionClip::new(None, band.playing(c.out)).ending(&["a2o_snakeBasket_closeLid_x", "a2o_snakeBasket_stop_x"], 4.3));
            match band.sound() {
                Some(s) => commands.entity(basket).insert(crate::sound::AmbientSound(s.into())),
                None => commands.entity(basket).remove::<crate::sound::AmbientSound>(),
            };
        }
        c.t += dt;
        let then = |c: &mut Charming, stage: Stage, clip: ActionClip, len: f32, commands: &mut Commands| {
            c.stage = stage;
            c.t = 0.0;
            c.len = len;
            commands.entity(e).insert(clip.ending(&["a2o_snakeBasket_closeLid_x", "a2o_snakeBasket_stop_x"], 4.3));
        };
        // (Sat down and the lid off, playing: the first try's time from then.)
        if !c.settled {
            if player.is_some_and(|p| band.playing(false).contains(&p.name.as_str())) {
                c.settled = true;
                c.until = now + rng.random_range(PLAY_MIN..PLAY_MAX);
            }
            continue;
        }
        match c.stage {
            Stage::Playing => {
                if now < c.until {
                    continue;
                }
                // (An expert with the snake away: magic of it, now and then.)
                if band == Band::Expert && (forced.as_deref() == Some("magic") || (forced.is_none() && rng.random_bool(MAGIC))) {
                    let at = baskets.get(basket).map(|b| b.1.translation()).unwrap_or_default();
                    sparkles.write(crate::effects::SpinSparkle { sim: basket, height: 1.4, at: Some(at), spread: 1.4 });
                    c.until = now + rng.random_range(PLAY_MIN..PLAY_MAX);
                    info!("snake charming: {} makes magic", sim.first);
                    continue;
                }
                let rises = match forced.as_deref() {
                    Some("snake") | Some("kiss") | Some("bitten") => true,
                    Some("none") | Some("bite") => false,
                    _ => rng.random_bool(band.snake_chance()),
                };
                if rises {
                    let l = clip_len(&mut lib, &data, "a2o_snakeBasket_trans_snakeappear_x");
                    then(&mut c, Stage::Rising, ActionClip::steps("a2o_snakeBasket_trans_snakeappear_x", &[], band.playing(true)), l, &mut commands);
                    c.out = true;
                    let at = baskets.get(basket).map(|b| b.1.translation()).ok();
                    sounds.write(crate::sound::PlaySound { name: "snake_vox_trans_appear".into(), at, volume: 1.0, speed: 1.0 });
                    info!("snake charming: the snake rises for {}", sim.first);
                } else {
                    let bitten = forced.as_deref() == Some("bite") || (forced.is_none() && rng.random_bool(HURT_HAND));
                    let clip = if bitten { "a2o_snakeBasket_beg_touchSnake_bite_x" } else { "a2o_snakeBasket_beg_touchSnake_shrug_x" };
                    let l = clip_len(&mut lib, &data, clip);
                    then(&mut c, Stage::Reaching, ActionClip::steps(clip, &[], band.playing(false)), l, &mut commands);
                    if bitten {
                        if let Some(mut m) = moodlets {
                            m.add(crate::life::MoodletKind::HurtHand, now);
                        }
                        // (Done, a hurt hand: they put it away once they've shaken it off.)
                        c.quit = true;
                    }
                    info!("snake charming: {} reaches in for the snake{}", sim.first, if bitten { ", and is bitten" } else { "" });
                }
            }
            Stage::Reaching | Stage::Kissing if c.quit => {
                if c.t >= c.len
                    && let Some(a) = q.0.front_mut()
                {
                    a.cancel = true;
                }
            }
            Stage::Reaching => {
                if c.t >= c.len {
                    c.stage = Stage::Playing;
                    c.until = now + rng.random_range(PLAY_MIN..PLAY_MAX);
                }
            }
            Stage::Rising => {
                if c.t >= c.len {
                    c.stage = Stage::Swaying;
                    c.until = now + rng.random_range(PLAY_MIN..PLAY_MAX);
                }
            }
            Stage::Swaying => {
                if now < c.until {
                    continue;
                }
                c.until = now + rng.random_range(PLAY_MIN..PLAY_MAX);
                let how = match forced.as_deref() {
                    Some("kiss") => Kiss::Won,
                    Some("bitten") => Kiss::Bitten,
                    _ => kiss(band, cobra, rng.random(), rng.random(), rng.random()),
                };
                let clip = match how {
                    Kiss::None => continue,
                    Kiss::Won => "a2o_snakeBasket_kissSnake_succeed_x",
                    Kiss::Bitten => "a2o_snakeBasket_kissSnake_epicFail_x",
                    Kiss::Missed => "a2o_snakeBasket_kissSnake_fail_x",
                };
                let l = clip_len(&mut lib, &data, clip);
                then(&mut c, Stage::Kissing, ActionClip::steps(clip, &[], band.playing(true)), l, &mut commands);
                if let Some(mut m) = moodlets {
                    match how {
                        Kiss::Won => m.add(crate::life::MoodletKind::SnakeKiss, now),
                        Kiss::Bitten => m.add(crate::life::MoodletKind::OuchMyFace, now),
                        _ => {}
                    }
                }
                // (Bitten in the face: done once it's over.)
                c.quit = how == Kiss::Bitten;
                info!("snake charming: {} tries a kiss: {how:?}", sim.first);
            }
            Stage::Kissing => {
                if c.t >= c.len {
                    c.stage = Stage::Swaying;
                }
            }
        }
    }
}

/// The snakes, as their charmers' clips have them: their own half of the clip in step, shown
/// once out (and while a clip of the basket's has it about).
fn snake_moves(
    mut commands: Commands,
    data: Res<crate::baked::Baked>,
    sims: Query<(&Charming, &ClipPlayer)>,
    mut snakes: Query<&mut Visibility, With<Snake>>,
) {
    for (c, player) in &sims {
        let Some(s) = c.snake else { continue };
        let Ok(mut vis) = snakes.get_mut(s) else { continue };
        let stem = player.name.strip_suffix("_x").unwrap_or(&player.name);
        let half = format!("{stem}_snake");
        let has = data.0.clip_names.iter().any(|n| n.eq_ignore_ascii_case(&half));
        let shown = c.out && has;
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if shown {
            commands.entity(s).insert(crate::objanim::PlayingClip { name: crate::pets::intern(half), time: player.time, looping: false });
        }
    }
}

/// `CHARM_TEST=1` (tests): ten seconds in, a snake charming basket put down near the selected
/// Sim, who charms it (at `CHARM_LEVEL=<n>`; `CHARM_COBRA=1`: a King Cobra in it;
/// `CHARM_FORCE=snake|none|bite|kiss|bitten|magic`: how each try goes; `CHARM_STOP=<seconds>`:
/// told to stop; `CHARM_PIE=1`: its menu).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn charm_test(
    mut commands: Commands,
    time: Res<Time>,
    mut sims: Query<(&Transform, &mut ActionQueue, &mut Skills), With<crate::sim::Selected>>,
    baskets: Query<(Entity, &GameObject, &Transform), Without<crate::sim::Selected>>,
    (catalog, data, grid): (Res<crate::loading::Catalog>, Res<crate::baked::Baked>, Option<Res<crate::nav::NavGrid>>),
    mut assets: ResMut<crate::objects::ObjectAssets>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
    mut cams: Query<&mut crate::camera::SimsCamera>,
    mut stage: Local<(u8, f32)>,
    (world, building): (Res<crate::loading::CurrentWorld>, Option<Res<crate::building::ActiveBuilding>>),
) {
    if std::env::var_os("CHARM_TEST").is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if now < 10.0 {
        return;
    }
    let basket = baskets.iter().find(|b| b.1.kind == ObjectKind::SnakeBasket);
    if let (Ok(mut c), Some((_, _, btf))) = (cams.single_mut(), basket) {
        let f = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        c.look_at(btf.translation);
        c.height_offset = f("CHARM_LOOK_Y", 0.5);
        c.distance = f("CHARM_DIST", 3.5);
        c.pitch = f("CHARM_PITCH", 0.3);
        let fwd = btf.rotation * Vec3::Z;
        c.yaw = fwd.x.atan2(fwd.z) + f("CHARM_YAW", 0.9);
    }
    let Ok((tf, mut q, mut skills)) = sims.single_mut() else { return };
    match stage.0 {
        0 => {
            *stage = (1, now);
            let Some(key) = data.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case("SnakeCharmingBasket")).map(|c| c.objd) else { return };
            let free = |p: Vec2| grid.as_deref().and_then(|g| g.cell_of(p).map(|(x, z)| !g.is_blocked(x, z))).unwrap_or(true);
            let here = tf.translation.xz();
            // (Somewhere open, to be seen from about.)
            let spot = (3..30).flat_map(|r| (0..12).map(move |k| here + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 12.0) * r as f32 * 0.5)).find(|c| (-7..=7).all(|i| (-7..=7).all(|j| free(*c + Vec2::new(i as f32, j as f32) * 0.4)))).unwrap_or(here + Vec2::X * 2.0);
            let mut ctx = crate::objects::AssetCtx { baked: &data.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
            // (On the ground there, whatever step they stood on.)
            let y = crate::building::walk_height(&world.data, building.as_deref(), Vec3::new(spot.x, tf.translation.y, spot.y));
            if let Some(o) = crate::home::spawn_game_object(&mut commands, &mut assets, &mut ctx, &catalog, key, Vec3::new(spot.x, y, spot.y), 0.0) {
                commands.entity(o.entity).insert(crate::save::Bought);
                if std::env::var_os("CHARM_COBRA").is_some() {
                    commands.entity(o.entity).insert(KingCobra);
                }
                info!("charm test: put down");
            }
        }
        1 if now - stage.1 > 2.0 => {
            let Some((b, ..)) = basket else { return };
            *stage = (2, now);
            if let Some(l) = std::env::var("CHARM_LEVEL").ok().and_then(|v| v.parse::<f32>().ok()) {
                skills.0.insert("SnakeCharming", l);
            }
            // (CHARM_PIE=1: its menu instead.)
            if std::env::var_os("CHARM_PIE").is_some() {
                commands.insert_resource(crate::hud::PieFor(b));
                return;
            }
            let def = crate::interact::interactions_for(ObjectKind::SnakeBasket).iter().position(|d| d.name == CHARM).unwrap_or(0);
            q.push_player(crate::interact::Action::new(CHARM, ActionKind::Object { target: b, def }, false));
            info!("charm test: charming");
        }
        // (CHARM_STOP=<seconds>: told to stop then.)
        2 if std::env::var("CHARM_STOP").ok().and_then(|v| v.parse::<f32>().ok()).is_some_and(|s| now - stage.1 > s) => {
            *stage = (3, now);
            if let Some(a) = q.0.front_mut() {
                a.cancel = true;
                info!("charm test: told to stop");
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charming_goes_as_the_game_tunes_it() {
        assert_eq!(Band::of(3.9), Band::Beginner);
        assert_eq!(Band::of(4.0), Band::Middling);
        assert_eq!(Band::of(8.0), Band::Expert);
        assert_eq!(Band::Beginner.snake_chance(), 0.1);
        assert_eq!(Band::Expert.sound(), Some("snake_pungi_exp_lp"));
        assert_eq!(kiss(Band::Beginner, false, 0.0, 0.0, 0.0), Kiss::None, "beginners don't try");
        assert_eq!(kiss(Band::Middling, false, 0.5, 0.0, 0.0), Kiss::None);
        assert_eq!(kiss(Band::Middling, false, 0.1, 0.3, 0.0), Kiss::Won);
        assert_eq!(kiss(Band::Middling, false, 0.1, 0.5, 0.0), Kiss::Missed, "a garter snake never bites");
        assert_eq!(kiss(Band::Middling, true, 0.1, 0.5, 0.05), Kiss::Bitten);
        assert_eq!(kiss(Band::Expert, true, 0.1, 0.65, 0.05), Kiss::Won);
        assert_eq!(cobra_offer(7.0, false, 5000, "Jack"), None);
        assert_eq!(cobra_offer(8.0, false, 5000, "Jack"), Some(Ok("Purchase King Cobra (§1000)".into())));
        assert_eq!(cobra_offer(8.0, false, 500, "Jack"), Some(Err("Jack does not have enough money to purchase a cobra.".into())));
        assert_eq!(cobra_offer(9.0, true, 5000, "Jack"), None);
    }
}
