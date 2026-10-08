//! Supernaturals (the Supernatural pack, and Late Night's vampires): vampires, werewolves,
//! witches, fairies and zombies. Each looks the part (vampires pale with red eyes, zombies
//! grey-green, fairies with their glowing wings on their backs) and moves the part (a zombie's
//! shamble, a vampire's run). The moon waxes and wanes over the game's eight-day lunar cycle;
//! under the full moon werewolves turn (the game's transformation, then their wolf form:
//! shaggy hair and beard, amber eyes, darker and furrier, prowling on all fours' gait), and turn
//! back at dawn. Vampires out in the daytime sun feel it. Their socials (hypnotic gazes,
//! spells, fairy tricks and frolics, werewolf sparring, howling at the moon) are in `social`.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::sim::{Occult, Sim};

pub struct SupernaturalPlugin;

impl Plugin for SupernaturalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (test_occults, werewolves, fairy_wings, flutter, vampire_sun).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// `OCCULTS=Vampire,Fairy,...` (tests): the household's Sims, in order, are these (a body
/// rebuilt for each).
fn test_occults(mut commands: Commands, mut sims: Query<(Entity, &mut Sim), With<crate::sim::HouseholdMember>>, mut done: Local<bool>) {
    let Ok(v) = std::env::var("OCCULTS") else { return };
    if *done || sims.is_empty() {
        return;
    }
    *done = true;
    let mut list: Vec<(Entity, Mut<Sim>)> = sims.iter_mut().collect();
    list.sort_by_key(|(_, s)| s.id);
    for ((e, mut sim), name) in list.into_iter().zip(v.split(',')) {
        sim.occult = Occult::from_name(name.trim());
        commands.entity(e).insert(crate::aging::NeedsNewBody);
        info!("{} is a {}", sim.first, name);
    }
}

/// The lunar cycle (days): the full moon on its fifth night (`kLunarCycleLength`, as the game
/// sets it by default).
pub const LUNAR_CYCLE: u32 = 8;
const FULL_MOON_DAY: u32 = 4;

/// Whether the moon is full tonight (the evening of `day` and the small hours after it).
pub fn full_moon_night(minutes: f64) -> bool {
    // (FULL_MOON=1, for tests: every night.)
    if std::env::var("FULL_MOON").is_ok() {
        let h = ((minutes / 60.0) % 24.0) as f32;
        return !(6.0..20.0).contains(&h);
    }
    let day = (minutes / 1440.0) as u32;
    let h = ((minutes / 60.0) % 24.0) as f32;
    (h >= 20.0 && day % LUNAR_CYCLE == FULL_MOON_DAY) || (h < 6.0 && day >= 1 && (day - 1) % LUNAR_CYCLE == FULL_MOON_DAY)
}

/// The moon tonight, as the clock shows it: new, waxing, full or waning.
pub fn moon_name(minutes: f64) -> &'static str {
    match (minutes / 1440.0) as u32 % LUNAR_CYCLE {
        0 => "New Moon",
        d if d < FULL_MOON_DAY => "Waxing Moon",
        FULL_MOON_DAY => "Full Moon",
        _ => "Waning Moon",
    }
}

/// A werewolf in their wolf form.
#[derive(Component)]
pub struct WolfForm;

/// The look a Sim's life state gives them: a tint over their skin, and their eyes' colour.
pub fn occult_look(occult: Option<Occult>, wolf: bool) -> (Vec3, Option<Color>) {
    match occult {
        Some(Occult::Vampire) => (Vec3::new(0.9, 0.9, 0.98), Some(Color::srgb(0.85, 0.08, 0.08))),
        Some(Occult::Zombie) => (Vec3::new(0.62, 0.74, 0.55), Some(Color::srgb(0.85, 0.8, 0.4))),
        Some(Occult::Werewolf) if wolf => (Vec3::new(0.72, 0.6, 0.5), Some(Color::srgb(0.95, 0.65, 0.1))),
        _ => (Vec3::ONE, None),
    }
}

/// A werewolf's wolf form: the pack's werewolf hairdo (and beard, for men).
pub fn wolf_outfit(cas: &crate::simbody::CasData, sim: &Sim, outfit: &mut crate::simbody::Outfit) {
    let (age, gender) = (crate::simbody::age_flag(sim.age), if sim.female { s3formats::sim::GENDER_FEMALE } else { s3formats::sim::GENDER_MALE });
    let find = |t: u32, part: &str| {
        cas.parts.iter().find(|p| p.baked && p.clothing_type == t && p.age_gender & age != 0 && p.age_gender & gender != 0 && p.name.contains(part)).cloned()
    };
    if let Some(h) = find(s3formats::sim::CT_HAIR, "EP7Werewolf") {
        outfit.hair = Some(h);
    }
    if !sim.female
        && let Some(b) = cas.parts.iter().find(|p| p.clothing_type == s3formats::sim::CT_BEARD && p.name.contains("EP7Werewolf") && p.age_gender & age != 0).cloned()
    {
        outfit.beard = Some(b);
    }
    outfit.wolf = true;
}

/// Werewolves turn under the full moon (the game's transformation, a notice for the
/// household's), and back at dawn.
fn werewolves(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut sims: Query<(Entity, &Sim, Has<WolfForm>, Option<&mut crate::interact::ActionQueue>, Has<crate::sim::HouseholdMember>)>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let night = full_moon_night(clock.minutes);
    for (e, sim, wolf, queue, member) in &mut sims {
        if sim.occult != Some(Occult::Werewolf) || sim.age.is_little() || night == wolf {
            continue;
        }
        let clip: &'static [&'static str] = if night { &["a_werewolf_trans2wolf"] } else { &["a_werewolf_trans2sim"] };
        if night {
            commands.entity(e).insert((WolfForm, crate::aging::NeedsNewBody));
            if member {
                notes.push(format!("The moon is full: {} has turned into a werewolf!", sim.first));
            }
        } else {
            commands.entity(e).remove::<WolfForm>().insert(crate::aging::NeedsNewBody);
        }
        if let Some(mut q) = queue {
            q.0.push_front(crate::interact::Action::new("Transform", crate::interact::ActionKind::Outro { clips: clip, then: None, secs: 4.0, stand_at: None, target: e }, true));
        }
    }
}

/// A fairy's wings: two glowing pictures of them on their back, fluttering.
#[derive(Component)]
struct Wings {
    joint: Entity,
}

#[derive(Component)]
struct Wing {
    side: f32,
    phase: f32,
}

/// The wing pictures (the game's twelve, `FairyWings01`..`12`), as textures.
#[derive(Default)]
struct WingLooks {
    mats: Vec<Handle<StandardMaterial>>,
    quad: Option<Handle<Mesh>>,
}

fn wing_image(ui: &crate::icons::GameUi, name: &str) -> Option<Image> {
    let png = ui.png(name)?;
    let (w, h, px) = s3bake::gamedata::decode_icon(&png)?;
    Some(Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        px,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

#[allow(clippy::too_many_arguments)]
fn fairy_wings(
    mut commands: Commands,
    ui: Option<Res<crate::icons::GameUi>>,
    fairies: Query<(Entity, &Sim, &crate::simbody::Skeleton, Option<&Wings>)>,
    joints: Query<(), With<Transform>>,
    mut looks: Local<WingLooks>,
    (mut meshes, mut images, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<Image>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(ui) = ui else { return };
    if looks.mats.is_empty() {
        for i in 1..=12 {
            let Some(img) = wing_image(&ui, &format!("FairyWings{i:02}")) else { continue };
            let tex = images.add(img);
            // (Light: the wings glow, added over what's behind them.)
            looks.mats.push(mats.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::new(1.8, 1.7, 1.4, 1.0)),
                base_color_texture: Some(tex),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            }));
        }
        looks.quad = Some(meshes.add(Rectangle::new(0.85, 0.85)));
        if looks.mats.is_empty() {
            return;
        }
    }
    let Some(quad) = looks.quad.clone() else { return };
    for (e, sim, skel, wings) in &fairies {
        if sim.occult != Some(Occult::Fairy) || sim.age.is_little() {
            continue;
        }
        // (Again after the body's been rebuilt: the old skeleton went, wings and all.)
        if wings.is_some_and(|w| joints.contains(w.joint)) {
            continue;
        }
        let Some(spine) = skel.rig.bones.iter().position(|b| b.name == "b__Spine2__") else { continue };
        let joint = skel.joints[spine];
        let mat = looks.mats[(sim.look % looks.mats.len() as u64) as usize].clone();
        for side in [-1.0f32, 1.0] {
            let w = commands
                .spawn((
                    Mesh3d(quad.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_xyz(side * 0.3, 0.05, -0.16).with_scale(Vec3::new(-side, 1.0, 1.0)),
                    Wing { side, phase: (sim.look % 7) as f32 },
                    bevy::light::NotShadowCaster,
                ))
                .id();
            commands.entity(joint).add_child(w);
        }
        commands.entity(e).insert(Wings { joint });
    }
}

/// Wings flutter, slowly.
fn flutter(time: Res<Time>, mut wings: Query<(&Wing, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (w, mut tf) in &mut wings {
        let beat = (t * 2.6 + w.phase).sin() * 0.22;
        tf.rotation = Quat::from_rotation_y(w.side * (0.45 + beat));
    }
}

/// Vampires out of doors in the daytime feel the sun (the game's Too Much Sun).
fn vampire_sun(
    clock: Res<GameClock>,
    weather: Res<crate::weather::Weather>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut sims: Query<(&Sim, &Transform, &mut crate::life::Moodlets)>,
) {
    let h = clock.hour_f();
    let (rise, set) = weather.daylight(clock.minutes);
    let sunny = (rise..set).contains(&h) && weather.overcast() < 0.6;
    for (sim, tf, mut m) in &mut sims {
        if sim.occult != Some(Occult::Vampire) {
            continue;
        }
        let out = sunny && !crate::weather::sheltered(building.as_deref(), tf.translation);
        m.set_while(crate::life::MoodletKind::TooMuchSun, out);
    }
}
