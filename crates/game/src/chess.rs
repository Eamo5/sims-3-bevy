//! Ranked chess, by the game's Logic tuning: a Sim plays ranked matches at a chess table
//! against an opponent of their own rank (whose chess skill rises with it, `1, 2, 3, 4, 5, 6`),
//! winning by Logic skill and a little luck, and climbs from Unranked to Grand Master after
//! `2, 4, 8, 12, 15` wins. Their record shows under Logic in the Skills tab, is saved, and
//! a Grand Master who has mastered Logic is a Chess Legend.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::interact::{Notifications, Skills};
use crate::sim::Sim;

pub struct ChessPlugin;

impl Plugin for ChessPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ranked_matches.run_if(in_state(PlayMode::Live)));
    }
}

/// The ranks (the game's names), and the wins that reach each one after the first.
pub const RANKS: [&str; 6] = ["Unranked", "Apprentice (Rank 1)", "Tenderfoot (Rank 2)", "Journeyman (Rank 3)", "Instructor (Rank 4)", "Grand Master (Rank 5)"];
const WINS_FOR_RANK: [u32; 5] = [2, 4, 8, 12, 15];
/// The chess skill of an opponent at each rank.
const OPPONENT_SKILL: [f32; 6] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];

/// A Sim's ranked chess record.
#[derive(Component, Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ChessRecord {
    pub wins: u32,
    pub losses: u32,
    pub rank: u8,
}

impl ChessRecord {
    pub fn rank_name(&self) -> &'static str {
        RANKS[(self.rank as usize).min(RANKS.len() - 1)]
    }
}

/// A ranked match just finished.
#[derive(Component)]
pub struct MatchPlayed;

fn ranked_matches(mut commands: Commands, mut sims: Query<(Entity, &Sim, &Skills, Option<&mut ChessRecord>), With<MatchPlayed>>, mut notes: ResMut<Notifications>) {
    let mut rng = rand::rng();
    for (e, sim, skills, record) in &mut sims {
        commands.entity(e).remove::<MatchPlayed>();
        let mut r = record.as_deref().copied().unwrap_or_default();
        // Logic against the opponent's chess skill (a Genius plays a little better).
        let opponent = OPPONENT_SKILL[(r.rank as usize).min(5)] * 1.6;
        let mine = skills.level("Logic") as f32 + if sim.traits.contains(&crate::life::Trait::Genius) { 1.5 } else { 0.0 };
        let p = (0.5 + (mine - opponent) * 0.1).clamp(0.05, 0.95);
        if rng.random_bool(p as f64) {
            r.wins += 1;
            let mut text = format!("{} won a ranked chess match", sim.first);
            if (r.rank as usize) < WINS_FOR_RANK.len() && r.wins >= WINS_FOR_RANK[r.rank as usize] {
                r.rank += 1;
                text += &format!(" and is now ranked {}!", r.rank_name());
            } else {
                text += "!";
            }
            notes.push(text);
        } else {
            r.losses += 1;
            notes.push(format!("{} lost a ranked chess match. Practice makes perfect.", sim.first));
        }
        match record {
            Some(mut rec) => *rec = r,
            None => {
                commands.entity(e).insert(r);
            }
        }
    }
}
