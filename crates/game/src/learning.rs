//! Learning about each other, as the game's (`InformationLearnedAboutSim`): a Sim learns another's
//! trait when the way a social went turned on it (the game's `ConsiderLearningAboutTargetSim`:
//! the rule's trait); a Charismatic one, getting to know someone, half the time learns one
//! (`kPercentChanceOfLearningRandomTraitIfCharismatic`). The household's learning is told in
//! the game's words ("Mia learned that Bella is Neurotic."); the traits known show in the
//! Relationships panel.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::life::{LifeEvent, LifeEventKind, Trait};
use crate::sim::{HouseholdMember, Sim};
use crate::social::Relationships;

pub struct LearningPlugin;

impl Plugin for LearningPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Learned>().add_systems(Update, learn.run_if(in_state(PlayMode::Live)));
    }
}

/// What a social taught `learner` of `about`: the trait it turned on, and
/// whether it may have taught another besides (its `LearnRandomTrait`).
#[derive(Message, Clone, Copy, Debug)]
pub struct Learned {
    pub learner: Entity,
    pub about: Entity,
    pub what: Option<Trait>,
    pub random: bool,
    /// The traits the social teaches if they have them: the learner's (to `about`) and
    /// `about`'s (to the learner), comma lists of the game's names.
    pub teaches: [&'static str; 2],
}

/// The chance a Charismatic Sim learns one getting to know someone, and anyone one of those
/// conversations that may teach one (`kPercentChanceOfLearningRandomTrait`).
const CHARISMATIC_CHANCE: f64 = 0.5;
const RANDOM_CHANCE: f64 = 0.25;

/// How the game says it ("{0.SimFirstName} learned that {2.String}.", the trait's predicate in it).
pub fn learned_words(ui: Option<&crate::layout::UiAssets>, data: Option<&s3bake::gamedata::GameDataBaked>, learner: &Sim, about: &Sim, t: Trait) -> String {
    let predicate = data
        .and_then(|d| d.traits.iter().find(|x| x.hex.eq_ignore_ascii_case(&t.game_id())))
        .map(|x| x.predicate.clone())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| format!("{{0.SimFirstName}} is {}", t.name()));
    let predicate = crate::layout::gendered(&predicate, about.female).replace("{0.SimFirstName}", &about.first).replace("{0.SimName}", &about.full_name());
    let line = ui.and_then(|u| u.localize("Gameplay/Socializing/InformationLearnedAboutSim:LearnTraitText")).unwrap_or_else(|| "{0.SimFirstName} learned that {2.String}.".into());
    crate::layout::gendered(&line, learner.female).replace("{0.SimFirstName}", &learner.first).replace("{2.String}", &predicate)
}

#[allow(clippy::too_many_arguments)]
fn learn(
    mut learned: MessageReader<Learned>,
    mut life: MessageReader<LifeEvent>,
    mut sims: Query<(&Sim, &mut Relationships, Has<HouseholdMember>)>,
    ui: Option<Res<crate::layout::UiAssets>>,
    game: Option<Res<crate::icons::GameUi>>,
    mut notes: ResMut<crate::interact::Notifications>,
) {
    let mut news: Vec<(Entity, Entity, Trait)> = Vec::new();
    let mut rng = rand::rng();
    for l in learned.read() {
        debug!("learning: {:?}", l);
        if let Some(t) = l.what {
            news.push((l.learner, l.about, t));
        }
        // (Those it teaches: each learns the other's, if they've them.)
        for (k, (who, of)) in [(l.about, l.learner), (l.learner, l.about)].into_iter().enumerate() {
            for n in l.teaches[k].split(',').map(str::trim).filter(|n| !n.is_empty()) {
                if let Some(t) = Trait::ALL.iter().find(|t| t.game_id().eq_ignore_ascii_case(n)) {
                    news.push((who, of, *t));
                }
            }
        }
        // (A conversation that may teach one: a quarter of the time, one not yet known.)
        if l.random
            && rng.random_bool(RANDOM_CHANCE)
            && let (Ok((_, rels, _)), Ok((them, ..))) = (sims.get(l.learner), sims.get(l.about))
        {
            let unknown: Vec<Trait> = them.traits.iter().copied().filter(|t| !rels.get(l.about).knows(*t) && !news.iter().any(|n| n.2 == *t)).collect();
            if let Some(t) = unknown.choose(&mut rng) {
                news.push((l.learner, l.about, *t));
            }
        }
    }
    for ev in life.read() {
        let LifeEventKind::Socialized { other, social } = ev.kind else { continue };
        let (Ok((me, rels, _)), Ok((them, ..))) = (sims.get(ev.sim), sims.get(other)) else { continue };
        let unknown: Vec<Trait> = them.traits.iter().copied().filter(|t| !rels.get(other).knows(*t)).collect();
        if social == "Get to Know" && me.traits.contains(&Trait::Charismatic) && rng.random_bool(CHARISMATIC_CHANCE)
            && let Some(t) = unknown.choose(&mut rng)
        {
            news.push((ev.sim, other, *t));
        }
    }
    for (learner, about, t) in news {
        // (The household know their own already.)
        let Ok((them, _, family)) = sims.get(about) else { continue };
        if !them.traits.contains(&t) {
            continue;
        }
        let ours_too = family;
        let them = them.clone();
        let Ok((me, mut rels, ours)) = sims.get_mut(learner) else { continue };
        if rels.entry(about).learn(t) && ours && !ours_too {
            notes.push(learned_words(ui.as_deref(), game.as_ref().map(|g| &*g.data), me, &them, t));
        }
    }
}
