//! Pets' toys, as the Pets pack has them: a cat bats its ball or catnip mouse about
//! (`CatToy.PlayWith`: its start, rounds of play and its end) or hides in the kitty bag (`KittyBag`:
//! in, peeking out a while, out); a dog chases its ball about the yard (`DogBall.Chase`: the ball
//! pushed with its nose, rolling off, run after and pushed again, now and then a miss,
//! `kChanceToFail`) and chews a cat's toy when it's no chew toy (`CatToy.Chew`); a horse plays with
//! its ball (`HorseBall`: nudged, bitten and thrown, sat on). Each is the pack's clip for the pet,
//! the toy playing its half; a ball pushed rolls off as its half has it, where there's room, and
//! stays where it stops. A grungy cat or dog walks into the pet hygienator, is washed and dried
//! and walks out clean (`PetHygienator.Wash`: Hygiene 200 at once).

use bevy::prelude::*;

use crate::interact::ObjectKind;

/// What a pet does with a toy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Toy {
    CatToy,
    KittyBag,
    /// A dog chewing a cat's toy (its urge to chew).
    ChewCatToy,
    DogBall,
    HorseBall,
    /// Washed in the pet hygienator.
    Hygienator,
    /// A cat playing with the wall dancer on its own (`walldancer`).
    WallDancer,
}

/// What a pet of a kind plays at with a toy of a kind (none: it doesn't).
pub fn toy_for(kind: &str, obj: ObjectKind) -> Option<Toy> {
    match (kind, obj) {
        ("ac" | "cc", ObjectKind::CatToy) => Some(Toy::CatToy),
        ("ac" | "cc", ObjectKind::KittyBag) => Some(Toy::KittyBag),
        ("ad" | "al", ObjectKind::DogBall) => Some(Toy::DogBall),
        ("ah", ObjectKind::HorseBall) => Some(Toy::HorseBall),
        ("ac" | "ad" | "al", ObjectKind::PetHygienator) => Some(Toy::Hygienator),
        ("ac", ObjectKind::CatWallToy) => Some(Toy::WallDancer),
        _ => None,
    }
}

/// What a dog chews when it's no chew toy: a cat's toy.
pub fn chew_for(kind: &str, obj: ObjectKind) -> Option<Toy> {
    matches!((kind, obj), ("ad" | "al", ObjectKind::CatToy)).then_some(Toy::ChewCatToy)
}

/// A pet at play with a toy: the part it's playing (its clip, and the toy's half of it), whether
/// that's its end, whether it's begun, and the side a ball's pushed from.
#[derive(Clone, Debug, Default)]
pub struct Playing {
    pub clip: String,
    pub half: Option<String>,
    pub ending: bool,
    pub started: bool,
    pub side: Option<Vec2>,
    /// Where the pet stood as the part began (a part that moves it, its `world_offset`, from
    /// there: a cat into the kitty bag).
    pub base: Option<(Vec3, Quat)>,
}

/// The chance a dog misses the ball (`DogBall.kChanceToFail`).
pub const MISS_CHANCE: f64 = 0.1;

impl Toy {
    /// Its clips for a pet of a kind: its start, its rounds, its end.
    pub fn parts(self, k: &str) -> (Option<String>, Vec<String>, Option<String>) {
        let s = |n: &str| Some(n.to_string());
        match self {
            Toy::CatToy => (s(&format!("{k}2o_catToy_playWith_start_x")), (1..=3).map(|i| format!("{k}2o_catToy_playWith_loop{i}_x")).collect(), s(&format!("{k}2o_catToy_playWith_stop_x"))),
            Toy::KittyBag => (
                s(&format!("{k}2o_kittyBag_hideIn_getIn_x")),
                ["loop1_breathe", "loop2_breatheBlink", "loop3", "loop4", "loop5"].iter().map(|l| format!("{k}2o_kittyBag_hideIn_{l}_x")).collect(),
                s(&format!("{k}2o_kittyBag_hideIn_getOut_x")),
            ),
            Toy::ChewCatToy => (s(&format!("{k}2o_catToy_chew_chewStart_x")), vec![format!("{k}2o_catToy_chew_chewLoop_x")], s(&format!("{k}2o_catToy_chew_chewStop_x"))),
            Toy::DogBall => (None, vec![format!("{k}2o_dogBall_pushBall_x")], None),
            Toy::HorseBall => (
                s("ah2o_horseBall_neutral2play_x"),
                ["push_nudge", "push_biteAndThrow", "push_sitOnBall"].iter().map(|l| format!("ah2o_horseBall_{l}_x")).collect(),
                s("ah2o_horseBall_play2Neutral_x"),
            ),
            Toy::Hygienator => (s(&format!("{k}2o_hygienator_getIn_x")), vec![format!("{k}2o_hygienator_use_x")], s(&format!("{k}2o_hygienator_getOut_x"))),
            // (The cat's half of the pack's clips: `CatWallToyPlay` plays them alone.)
            Toy::WallDancer => (s("a2ac_wallDancerToy_start_y"), (1..=4).map(|i| format!("a2ac_wallDancerToy_loop{i}_y")).collect(), s("a2ac_wallDancerToy_stop_y")),
        }
    }

    /// A miss at the ball (the dog's `fail`), played in place of a push.
    pub fn miss(self, k: &str) -> Option<String> {
        (self == Toy::DogBall).then(|| format!("{k}2o_dogBall_fail_x"))
    }

    /// A ball: it rolls off when pushed.
    pub fn rolls(self) -> bool {
        matches!(self, Toy::DogBall | Toy::HorseBall)
    }

    /// The need it's for: a dog's chewing its urge to, a horse's ball its exercise
    /// (`HorseExercise`), the rest fun.
    pub fn need(self) -> usize {
        match self {
            Toy::ChewCatToy | Toy::HorseBall => crate::petneeds::URGE,
            Toy::Hygienator => crate::petneeds::HYGIENE,
            _ => crate::petneeds::FUN,
        }
    }

    /// How fast it fills its need, and fun besides (a game hour: the interactions' tuning, `Fun`
    /// and `DogDestruction` and `HorseExercise`; a cat in the kitty bag `kFunDelta` on top).
    pub fn rates(self) -> (f32, f32) {
        match self {
            Toy::CatToy => (25.0, 0.0),
            Toy::KittyBag => (15.0, 0.0),
            Toy::ChewCatToy => (100.0, 15.0),
            Toy::DogBall => (100.0, 0.0),
            Toy::HorseBall => (100.0, 0.0),
            // (Its 200 over the wash, a fifth of an hour.)
            Toy::Hygienator => (1000.0, 0.0),
            Toy::WallDancer => (30.0, 0.0),
        }
    }

    /// How far from the toy the pet stands to play, facing it (its jig's routing slot: a cat's
    /// toy's `petToyPlayJig`, a ball's `dogBallLargeDogJig`, and so on).
    pub fn stand_off(self, kind: &str) -> f32 {
        match (self, kind) {
            (Toy::CatToy, "cc") => 0.175,
            (Toy::CatToy, _) => 0.378,
            (Toy::KittyBag, _) => 1.0,
            (Toy::ChewCatToy, "al") => 0.3,
            (Toy::ChewCatToy, _) => 0.4,
            (Toy::DogBall, "al") => 0.315,
            (Toy::DogBall, _) => 0.671,
            (Toy::HorseBall, _) => 1.192,
            (Toy::Hygienator, _) => 1.3,
            (Toy::WallDancer, _) => crate::walldancer::CAT_SLOT.y,
        }
    }

    /// How far a push sends it (its jig's end slot).
    pub fn roll(self) -> f32 {
        match self {
            Toy::DogBall => 5.0,
            Toy::HorseBall => 8.9,
            _ => 0.0,
        }
    }
}

/// The pose a toy's held in while it's not in use (the pet hygienator's arms folded away:
/// `o_petHygienator_pose`).
pub fn idle_pose(toy: Toy) -> Option<&'static str> {
    (toy == Toy::Hygienator).then_some("o_petHygienator_pose_petHygienator")
}

/// Toys new in the world take their idle pose.
pub fn pose_new_toys(mut commands: Commands, added: Query<(Entity, &crate::interact::GameObject), Added<crate::interact::GameObject>>) {
    for (e, o) in &added {
        if o.kind == ObjectKind::PetHygienator
            && let Some(p) = idle_pose(Toy::Hygienator)
        {
            commands.entity(e).insert(crate::objanim::HeldPose(p));
        }
    }
}

/// The actors a toy's half of a pet's clip is named for.
const TOY_ACTORS: [&str; 5] = ["catToy", "kittyBag", "dogBall", "horseBall", "petHygienator"];

/// The toy's half of a pet's clip (`ad2o_dogBall_pushBall_x` → `ad2o_dogBall_pushBall_dogBall`).
pub fn half_of(names: &[String], clip: &str) -> Option<String> {
    if let Some(h) = crate::walldancer::half_of(names, clip) {
        return Some(h);
    }
    let stem = clip.strip_suffix("_x")?;
    TOY_ACTORS.iter().find_map(|a| {
        let want = format!("{stem}_{a}");
        names.iter().find(|n| n.eq_ignore_ascii_case(&want)).cloned()
    })
}

/// Which way a ball's pushed from (the side the pet comes up to it on, the ball rolling off the
/// other way): toward `from` if there's room for it to roll that way, else the nearest way round
/// that has it (none: nowhere to roll).
pub fn push_side(ball: Vec2, from: Vec2, roll: f32, room: impl Fn(Vec2, Vec2) -> bool) -> Option<Vec2> {
    let first = (from - ball).normalize_or(Vec2::Y);
    (0..16).map(|i| {
        let k = (i + 1) / 2;
        let a = k as f32 * std::f32::consts::TAU / 16.0 * if i % 2 == 0 { 1.0 } else { -1.0 };
        Vec2::from_angle(a).rotate(first)
    })
    .find(|f| room(ball, ball - *f * roll))
}

/// Where a pet stands to play with a toy at `toy` (turned `facing`), coming from `from`: and the
/// way it faces. A cat's toy's from where it comes; a kitty bag's at its slot; a ball's from the
/// side it'll push it from.
pub fn stand_at(toy: Toy, kind: &str, at: Vec2, facing: Quat, from: Vec2, side: Option<Vec2>) -> (Vec2, Vec2) {
    match toy {
        Toy::KittyBag => {
            let f = (facing * Vec3::Z).xz().normalize_or(Vec2::Y);
            (at - f * toy.stand_off(kind), f)
        }
        // (Before its door, its routing slot in front of it, facing in; before the wall dancer,
        // facing the wall.)
        Toy::Hygienator | Toy::WallDancer => {
            let f = (facing * Vec3::Z).xz().normalize_or(Vec2::Y);
            (at + f * toy.stand_off(kind), -f)
        }
        _ => {
            let f = side.unwrap_or_else(|| (from - at).normalize_or(Vec2::Y));
            (at + f * toy.stand_off(kind), -f)
        }
    }
}

/// Where a ball pushed comes to rest: its half's transform bone at the end, turned and placed as
/// the ball is (none: it doesn't move).
pub fn rolled_to(clip: &s3formats::sim::Clip, ball: &Transform) -> Option<Vec3> {
    let track = clip.tracks.get(&crate::props::TRANSFORM_BONE)?;
    let end = crate::anim::sample_track_vec(&track.translation, clip.duration)?;
    let off = Vec3::new(end.x, 0.0, end.z);
    (off.length() > 0.05).then(|| ball.translation + ball.rotation * off)
}

/// Whether a ball can roll from `a` to `b`: on the lot, on open ground (the ball's own cells
/// aside), through no wall or fence.
pub fn room_to_roll(grid: &crate::nav::NavGrid, on_lot: impl Fn(Vec2) -> bool, a: Vec2, b: Vec2) -> bool {
    if grid.crosses(a, b) {
        return false;
    }
    let n = (a.distance(b) / 0.4).ceil().max(1.0) as usize;
    (1..=n).map(|i| a.lerp(b, i as f32 / n as f32)).filter(|p| p.distance(a) > 0.6).all(|p| on_lot(p) && grid.cell_of(p).is_some_and(|(x, z)| !grid.is_blocked(x, z)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pets_play_with_their_own_kinds_toys() {
        assert_eq!(toy_for("ac", ObjectKind::CatToy), Some(Toy::CatToy));
        assert_eq!(toy_for("cc", ObjectKind::KittyBag), Some(Toy::KittyBag));
        assert_eq!(toy_for("ad", ObjectKind::DogBall), Some(Toy::DogBall));
        assert_eq!(toy_for("ah", ObjectKind::HorseBall), Some(Toy::HorseBall));
        // (Not a puppy's ball, nor a horse's for a dog.)
        assert_eq!(toy_for("cd", ObjectKind::DogBall), None);
        assert_eq!(toy_for("ad", ObjectKind::HorseBall), None);
        // (A dog chews a cat's toy, for its urge.)
        assert_eq!(chew_for("al", ObjectKind::CatToy), Some(Toy::ChewCatToy));
        assert_eq!(Toy::ChewCatToy.need(), crate::petneeds::URGE);
        let (start, rounds, end) = Toy::KittyBag.parts("ac");
        assert_eq!((start.as_deref(), rounds.len(), end.as_deref()), (Some("ac2o_kittyBag_hideIn_getIn_x"), 5, Some("ac2o_kittyBag_hideIn_getOut_x")));
        assert!(Toy::DogBall.rolls() && Toy::DogBall.miss("ad").is_some() && !Toy::CatToy.rolls());
        // (A horse's ball is its exercise.)
        assert_eq!(Toy::HorseBall.need(), crate::petneeds::URGE);
        let names: Vec<String> = ["ad2o_dogBall_pushBall_x", "ad2o_dogBall_pushBall_dogBall", "ah2o_horseBall_push_nudge_horseball"].map(String::from).to_vec();
        assert_eq!(half_of(&names, "ad2o_dogBall_pushBall_x").as_deref(), Some("ad2o_dogBall_pushBall_dogBall"));
        assert_eq!(half_of(&names, "ah2o_horseBall_push_nudge_x").as_deref(), Some("ah2o_horseBall_push_nudge_horseball"));
    }

    #[test]
    fn a_ball_is_pushed_where_it_can_roll() {
        let ball = Vec2::new(10.0, 10.0);
        // (From the pet's side, rolling away from it.)
        let side = push_side(ball, Vec2::new(10.0, 14.0), 5.0, |_, _| true).unwrap();
        assert!((side - Vec2::Y).length() < 1e-4);
        let (stand, face) = stand_at(Toy::DogBall, "ad", ball, Quat::IDENTITY, Vec2::ZERO, Some(side));
        assert!((stand - Vec2::new(10.0, 10.671)).length() < 1e-4 && (face + Vec2::Y).length() < 1e-4);
        // (A wall that way: the nearest way round with room.)
        let side = push_side(ball, Vec2::new(10.0, 14.0), 5.0, |_, to| to.y > 8.0).unwrap();
        assert!(side.y < 0.5, "{side}");
        assert_eq!(push_side(ball, Vec2::new(10.0, 14.0), 5.0, |_, _| false), None);
    }

    #[test]
    fn a_pushed_ball_rolls_as_its_half_has_it() {
        let mut clip = s3formats::sim::Clip { duration: 3.0, ..Default::default() };
        clip.tracks.insert(crate::props::TRANSFORM_BONE, s3formats::sim::Track { translation: vec![(0.0, [0.0; 3]), (3.0, [0.0, 0.0, -5.0])], rotation: Vec::new() });
        let ball = Transform::from_xyz(10.0, 0.0, 10.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let to = rolled_to(&clip, &ball).unwrap();
        assert!((to - Vec3::new(5.0, 0.0, 10.0)).length() < 1e-4, "{to}");
    }
}
