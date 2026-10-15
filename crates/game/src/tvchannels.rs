//! The TV's channels, as the game's `TV` table has them: what's on each, the TVs that get it (a
//! cheap set the first four, a moderate one six, an expensive one nine, the best all ten), the
//! traits whose Sims like it, and what watching it does: Cookin' Cable teaches Cooking (and now
//! and then a recipe), Got Garden? Gardening, Fishing Fracas Fishing; Terror TV frightens the
//! timid off; the channel a Sim's traits like is more fun to them.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::interact::{GameObject, ObjectKind};
use crate::life::Trait;

pub struct TvChannelsPlugin;

impl Plugin for TvChannelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tv_levels, tune, channel_effects).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// One of the channels (the game's `TVChannel` rows).
pub struct Channel {
    /// The game's name for it (its row's `Channel_Name`).
    #[allow(dead_code)]
    pub key: &'static str,
    pub name: &'static str,
    /// The TVs with this level of channels or better get it.
    pub level: u8,
    /// Sims with these traits like it.
    pub traits: &'static [Trait],
    /// The skill it teaches.
    pub skill: Option<&'static str>,
    /// Sims with these traits are frightened by it.
    pub scares: &'static [Trait],
    /// It teaches recipes now and then.
    pub recipes: bool,
}

const fn ch(key: &'static str, name: &'static str, level: u8, traits: &'static [Trait], skill: Option<&'static str>, scares: &'static [Trait], recipes: bool) -> Channel {
    Channel { key, name, level, traits, skill, scares, recipes }
}

pub static CHANNELS: [Channel; 10] = [
    ch("ActionPlanet", "Action World", 1, &[Trait::ComputerWhiz, Trait::Brave, Trait::Excitable, Trait::Daredevil, Trait::PartyAnimal], None, &[], false),
    ch("SportsStation", "Sports Universe", 1, &[Trait::Athletic, Trait::Daredevil, Trait::PartyAnimal], None, &[], false),
    ch("RomanticRendezvous", "Romantic Rendezvous", 1, &[Trait::Artistic, Trait::Charismatic, Trait::Flirty, Trait::OverEmotional], None, &[], false),
    ch("KidZone", "KidZ Zone", 1, &[Trait::Childish, Trait::Absentminded], None, &[], false),
    ch("SimCooking", "Cookin' Cable", 2, &[Trait::CouchPotato, Trait::NaturalCook], Some("Cooking"), &[], true),
    ch("HouseOfHorrors", "Terror TV", 2, &[], None, &[Trait::Neurotic, Trait::Coward, Trait::OverEmotional], false),
    ch("GotGarden?", "Got Garden?", 2, &[Trait::GreenThumb, Trait::Vegetarian, Trait::LovesTheOutdoors], Some("Gardening"), &[], false),
    ch("TheHallOfHistory", "Halls of History", 3, &[Trait::Artistic, Trait::Bookworm, Trait::Genius, Trait::Workaholic], None, &[], false),
    ch("MasterFisher", "Fishing Fracas", 3, &[Trait::LovesTheOutdoors], Some("Fishing"), &[], false),
    ch("SuperShoppingNetwork", "Super Shopping Network", 4, &[Trait::Snob, Trait::CouchPotato], None, &[], false),
];

/// What a TV skill channel teaches an hour (the game's `SkillGain`: 5 points a minute).
const SKILL_PER_HOUR: f32 = 300.0;
/// The chance an hour of Cookin' Cable teaches a recipe (the game's 1% a watching loop, about
/// one every ten minutes).
const RECIPE_CHANCE_PER_HOUR: f64 = 0.06;

/// The channels a TV gets (the game's per-class `mCurrenMaxSupportedChannelLevel`).
#[derive(Component, Clone, Copy, Debug)]
pub struct TvLevel(pub u8);

/// The channel a TV's on.
#[derive(Component, Clone, Copy, Debug)]
pub struct OnChannel(pub usize);

/// A TV just turned on or over: to this channel (none: the one the Sim likes best).
#[derive(Component, Clone, Copy, Debug)]
pub struct Tune {
    pub channel: Option<usize>,
    pub by: Entity,
}

/// A TV's channels by its class: a cheap set's first, an expensive one's third.
pub fn level_of(script: &str) -> u8 {
    if script.contains("TVCheap") {
        1
    } else if script.contains("TVExpensive") {
        3
    } else if script.contains("TVUltra") || script.contains("Holographic") {
        4
    } else {
        2
    }
}

/// The channels on a TV of this level.
pub fn channels_for(level: u8) -> impl Iterator<Item = (usize, &'static Channel)> {
    CHANNELS.iter().enumerate().filter(move |(_, c)| c.level <= level)
}

/// How much a Sim cares for a channel (the game's `GetInterestInTvChannel`: one a trait of
/// theirs it suits).
pub fn interest(traits: &[Trait], c: &Channel) -> usize {
    c.traits.iter().filter(|t| traits.contains(t)).count()
}

/// The channel a Sim puts on (the game's `ChoosePreferedTVChannel`): the one their traits like
/// most, else any; never the frights for the timid.
pub fn preferred(traits: &[Trait], level: u8, rng: &mut impl Rng) -> usize {
    let open: Vec<(usize, &Channel)> = channels_for(level).filter(|(_, c)| !c.scares.iter().any(|t| traits.contains(t))).collect();
    let best = open.iter().map(|(_, c)| interest(traits, c)).max().unwrap_or(0);
    let picks: Vec<usize> = open.iter().filter(|(_, c)| interest(traits, c) == best).map(|(i, _)| *i).collect();
    picks.get(rng.random_range(0..picks.len().max(1))).copied().unwrap_or(0)
}

/// Each TV put down gets its channels (by its class in the catalogue).
fn tv_levels(mut commands: Commands, added: Query<(Entity, &GameObject), Added<GameObject>>, data: Option<Res<crate::baked::Baked>>) {
    let Some(data) = data else { return };
    for (e, o) in &added {
        if o.kind != ObjectKind::Tv {
            continue;
        }
        let script = data.0.catalog.iter().find(|c| c.objd == o.objd).map_or("", |c| c.script.as_str());
        commands.entity(e).insert(TvLevel(level_of(script)));
    }
}

/// A TV turned on (or over): the channel chosen, else the one the Sim likes.
fn tune(mut commands: Commands, tvs: Query<(Entity, &Tune, Option<&TvLevel>, Option<&OnChannel>)>, sims: Query<&crate::sim::Sim>) {
    let mut rng = rand::rng();
    for (e, t, level, on) in &tvs {
        let level = level.map_or(2, |l| l.0);
        let channel = t.channel.filter(|c| CHANNELS.get(*c).is_some_and(|ch| ch.level <= level)).or_else(|| {
            // (Watching on with what's on, unless it's not to their taste.)
            let traits = sims.get(t.by).map(|s| s.traits.clone()).unwrap_or_default();
            match on {
                Some(c) if !CHANNELS[c.0].scares.iter().any(|x| traits.contains(x)) => Some(c.0),
                _ => Some(preferred(&traits, level, &mut rng)),
            }
        });
        commands.entity(e).remove::<Tune>();
        if let Some(c) = channel {
            commands.entity(e).insert(OnChannel(c));
        }
    }
}

/// What's on, for those watching it: the skill it teaches, a recipe now and then, more fun for
/// those whose traits like it, and a fright that sends the timid off.
#[allow(clippy::type_complexity)]
fn channel_effects(
    delta: Res<crate::clock::SimDelta>,
    clock: Res<crate::clock::GameClock>,
    mut watchers: Query<(
        Entity,
        &crate::sim::Sim,
        &crate::surroundings::WatchingTv,
        &mut crate::interact::ActionQueue,
        &mut crate::interact::Skills,
        &mut crate::sim::Motives,
        &mut crate::life::Moodlets,
        Option<&mut crate::meals::KnownRecipes>,
        Option<&crate::wishes::Wishes>,
        &crate::life::Mood,
    )>,
    tvs: Query<&OnChannel>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut life: MessageWriter<crate::life::LifeEvent>,
) {
    let hours = delta.0 / 60.0;
    if hours <= 0.0 {
        return;
    }
    let mut rng = rand::rng();
    for (e, sim, watching, mut queue, mut skills, mut motives, mut moodlets, known, wishes, mood) in &mut watchers {
        let Ok(on) = tvs.get(watching.0) else { continue };
        let c = &CHANNELS[on.0];
        let Some(front) = queue.0.front_mut() else { continue };
        if front.label != "Watch the TV" || front.cancel {
            continue;
        }
        // (Terror TV: the timid frightened, and off.)
        if c.scares.iter().any(|t| sim.traits.contains(t)) {
            moodlets.add(crate::life::MoodletKind::Scared, clock.minutes);
            front.cancel = true;
            notes.push(format!("{} was too scared to keep watching {}!", sim.first, c.name));
            continue;
        }
        // (The channel their traits like: more fun, a quarter again a trait.)
        let liked = interest(&sim.traits, c);
        if liked > 0 {
            motives.add(crate::sim::FUN, 55.0 * 0.25 * liked as f32 * hours);
        }
        if let Some(sk) = c.skill {
            let rate = crate::life::skill_rate(&sim.traits, sk) * crate::wishes::reward_skill_rate(wishes) * mood.skill_rate();
            if let Some(level) = skills.add_points(sk, SKILL_PER_HOUR * hours * rate) {
                notes.push(format!("{} reached level {level} in {sk}!", sim.first));
                life.write(crate::life::LifeEvent::new(e, crate::life::LifeEventKind::SkillUp { skill: sk, level }));
            }
        }
        if c.recipes
            && rng.random_bool((RECIPE_CHANCE_PER_HOUR * hours as f64).min(1.0))
            && let (Some(mut known), Some(ui)) = (known, ui.as_ref())
        {
            let cooking = skills.level("Cooking");
            let unknown: Vec<&s3bake::gamedata::RecipeInfo> = ui.data.recipes.iter().filter(|r| !r.snack && u32::from(r.level) <= cooking && !known.0.contains(&r.key)).collect();
            if let Some(r) = unknown.get(rng.random_range(0..unknown.len().max(1))) {
                known.0.push(r.key.clone());
                notes.push(format!("{} learned to make {} from {}.", sim.first, r.name, c.name));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_come_with_the_tv_and_suit_the_traits() {
        assert_eq!(channels_for(1).count(), 4);
        assert_eq!(channels_for(2).count(), 7);
        assert_eq!(channels_for(4).count(), 10);
        assert_eq!(level_of("Sims3.Gameplay.Objects.Electronics.TVCheap"), 1);
        assert_eq!(level_of("Sims3.Gameplay.Objects.Electronics.TVExpensive"), 3);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(1);
        // (A green thumb puts the gardening on; the athletic, the sports.)
        assert_eq!(CHANNELS[preferred(&[Trait::GreenThumb], 2, &mut rng)].key, "GotGarden?");
        assert_eq!(CHANNELS[preferred(&[Trait::Athletic], 1, &mut rng)].key, "SportsStation");
        // (A coward never chooses the frights.)
        for _ in 0..50 {
            assert_ne!(CHANNELS[preferred(&[Trait::Coward], 4, &mut rng)].key, "HouseOfHorrors");
        }
    }
}
