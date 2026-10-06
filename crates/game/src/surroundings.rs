//! Clean and dirty surroundings, as the game judges them: dirty dishes left about, food left
//! out until it spoils, and a trash can left full put the household's Sims nearby in Dirty
//! (and then Filthy) Surroundings. Clearing dishes away fills the nearest trash can, which
//! someone then has to empty.

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{GameObject, Notifications, ObjectKind};
use crate::life::{MoodletKind, Moodlets};
use crate::meals::Meal;
use crate::sim::{HouseholdMember, Sim};

pub struct SurroundingsPlugin;

impl Plugin for SurroundingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spoil_food, trash_cans, discard, surroundings).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// How long food keeps once served (game minutes).
const SPOIL_MINUTES: f64 = 8.0 * 60.0;
/// How many loads of scraps a trash can holds.
pub const TRASH_CAPACITY: u8 = 5;
/// How near (metres, on the same floor) mess counts against a Sim's surroundings.
const NEAR: f32 = 5.0;

/// When a meal was set out.
#[derive(Component)]
pub struct ServedAt(pub f64);

/// Food left out too long.
#[derive(Component)]
pub struct Spoiled;

/// How full a trash can is.
#[derive(Component, Default, Clone, Copy)]
pub struct TrashFill(pub u8);

/// A Sim who has just cleared dishes away: the scraps go in the nearest trash can.
#[derive(Component)]
pub struct Discarded;

fn spoil_food(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut meals: Query<(Entity, &mut GameObject, Option<&ServedAt>), (With<Meal>, Without<Spoiled>)>,
    mut notes: ResMut<Notifications>,
) {
    for (e, mut obj, served) in &mut meals {
        let Some(at) = served else {
            commands.entity(e).insert(ServedAt(clock.minutes));
            continue;
        };
        if clock.minutes - at.0 < SPOIL_MINUTES {
            continue;
        }
        // Left out too long: only fit for clearing away.
        commands.entity(e).remove::<Meal>().insert(Spoiled);
        let dish = obj.name.to_lowercase();
        let has = if dish.ends_with('s') { "have" } else { "has" };
        notes.push(format!("The {dish} left out {has} spoiled."));
        obj.kind = ObjectKind::DirtyDishes;
        obj.name = "Spoiled Food".into();
    }
}

/// Every trash can keeps count of what's in it.
fn trash_cans(mut commands: Commands, cans: Query<(Entity, &GameObject), Without<TrashFill>>) {
    for (e, o) in &cans {
        if o.kind == ObjectKind::TrashCan {
            commands.entity(e).insert(TrashFill(0));
        }
    }
}

fn discard(
    mut commands: Commands,
    sims: Query<(Entity, &Transform), With<Discarded>>,
    mut cans: Query<(&GameObject, &Transform, &mut TrashFill)>,
    mut notes: ResMut<Notifications>,
) {
    for (e, tf) in &sims {
        commands.entity(e).remove::<Discarded>();
        let Some((obj, _, mut fill)) = cans.iter_mut().min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation))) else {
            continue;
        };
        if fill.0 < TRASH_CAPACITY {
            fill.0 += 1;
            if fill.0 == TRASH_CAPACITY {
                notes.push(format!("The {} is full. Someone should empty it.", obj.name.to_lowercase()));
            }
        }
    }
}

/// Dirty and Filthy Surroundings for household Sims near mess.
#[allow(clippy::type_complexity)]
fn surroundings(
    time: Res<Time>,
    mut last: Local<f32>,
    mut sims: Query<(&Transform, &mut Moodlets, &Visibility), (With<Sim>, With<HouseholdMember>)>,
    mess: Query<(&GameObject, &Transform, Has<Spoiled>, Option<&TrashFill>)>,
) {
    let now = time.elapsed_secs();
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    // Each mess and how bad it is: dishes, spoiled food, a full trash can.
    let spots: Vec<(Vec3, u32)> = mess
        .iter()
        .filter_map(|(o, tf, spoiled, fill)| {
            let w = match o.kind {
                ObjectKind::DirtyDishes if spoiled => 3,
                ObjectKind::DirtyDishes => 1,
                ObjectKind::TrashCan if fill.is_some_and(|f| f.0 >= TRASH_CAPACITY) => 3,
                _ => return None,
            };
            Some((tf.translation, w))
        })
        .collect();
    for (tf, mut moodlets, vis) in &mut sims {
        let score: u32 = if *vis == Visibility::Hidden {
            0
        } else {
            spots.iter().filter(|(p, _)| (p.y - tf.translation.y).abs() < 1.8 && p.with_y(0.0).distance(tf.translation.with_y(0.0)) < NEAR).map(|s| s.1).sum()
        };
        let filthy = score >= 6;
        moodlets.set_while(MoodletKind::FilthySurroundings, filthy);
        moodlets.set_while(MoodletKind::DirtySurroundings, score >= 2 && !filthy);
    }
}
