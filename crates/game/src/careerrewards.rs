//! A career's promotion rewards, as the game's `Career.GivePromotionRewardObjects`: reaching a
//! level the first time brings what its table says (`PromotionRewardData`), once the path's
//! chosen where the career branches. An object (a Culinary Sim's food processor and fridge, a
//! Symphonic musician's stereo, a forensic analyst's laptop, a thief's "acquisitions": one
//! painting or sculpture of a few, then The Fox) goes in the family inventory, with the game's
//! words ("Congratulations! As a reward for your advancement to..."). Science gives books and
//! harvests instead (`Science.GivePromotionRewardObjects`): a Gardening, Handiness or Fishing
//! book they've not opened, of those up to their level in it; and a special plant's (a Flame
//! Fruit, a Lifefruit, or a Death Flower Bush's seed). What the game unlocks instead (a hidden
//! trait, a police car, an impressive story, raids, meetings) isn't here.

use bevy::prelude::*;
use rand::Rng;
use rand::seq::IndexedRandom;

use crate::PlayMode;
use crate::careers::Job;
use crate::interact::{Notifications, Skills};
use crate::sim::{HouseholdMember, Sim};

pub struct CareerRewardsPlugin;

impl Plugin for CareerRewardsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (reward_test, rewards).chain().run_if(in_state(PlayMode::Live)));
    }
}

/// The game's words for an object rewarded.
const OBJECT_REWARD: &str = "Gameplay/Careers/Career:ObjectReward";
/// Science's books, by their skills.
const BOOK_SKILLS: [&str; 3] = ["Gardening", "Handiness", "Fishing"];
/// What the game unlocks rather than gives.
const UNLOCKS: [&str; 3] = ["ImpressiveStorySocial", "RaidCriminals", "Burglar"];

/// A level's reward, as its table has it.
#[derive(Debug, PartialEq)]
pub enum Reward<'a> {
    /// A catalogue object, by its name.
    Object(&'a str),
    /// A skill book of that skill.
    Book(&'a str),
    /// A special plant's harvest (or its seed).
    Harvest(&'a str),
}

/// What a career's level brings: one of a list at random.
pub fn reward_of<'a>(career: &str, data: &'a str, rng: &mut impl Rng) -> Option<Reward<'a>> {
    let choices: Vec<&str> = data.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    let pick = *choices.choose(rng)?;
    if career == "Science" {
        return if choices.len() > 1 {
            Some(Reward::Harvest(pick))
        } else {
            BOOK_SKILLS.contains(&pick).then_some(Reward::Book(pick))
        };
    }
    (!pick.starts_with("Unlock") && !UNLOCKS.contains(&pick)).then_some(Reward::Object(pick))
}

/// CAREER_REWARD=<career key>:<level>[:<path>] (tests): the selected Sim just promoted to that
/// level (1-based) of that career, eight seconds in.
fn reward_test(mut commands: Commands, time: Res<Time>, sel: Query<Entity, With<crate::sim::Selected>>, mut done: Local<bool>) {
    let Ok(v) = std::env::var("CAREER_REWARD") else { return };
    if *done || time.elapsed_secs() < 8.0 {
        return;
    }
    let Ok(e) = sel.single() else { return };
    *done = true;
    let mut parts = v.split(':');
    let key = parts.next().unwrap_or("");
    let level = parts.next().and_then(|l| l.parse::<usize>().ok()).unwrap_or(1).max(1) - 1;
    let branch = parts.next().and_then(|b| b.parse::<usize>().ok()).unwrap_or(0);
    let Some(track) = crate::careers::careers().iter().position(|c| c.key.eq_ignore_ascii_case(key)) else {
        warn!("career reward test: no career {key}");
        return;
    };
    info!("career reward test: {key} level {}", level + 1);
    commands.entity(e).insert(Job { level, branch, highest: level, reward_due: true, ..Job::new(track) });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn rewards(
    mut commands: Commands,
    mut sims: Query<(Entity, &Sim, &mut Job, &Skills, Option<&crate::skillbooks::SkillBooks>), With<HouseholdMember>>,
    questions: Res<crate::dialog::Questions>,
    (mut family, mut garden): (ResMut<crate::familyinv::FamilyInventory>, ResMut<crate::gardening::Garden>),
    (baked, gu, ui): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>, Option<Res<crate::layout::UiAssets>>),
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, mut job, skills, books) in &mut sims {
        if !job.reward_due {
            continue;
        }
        // (Not till the path's chosen where the career branches.)
        if questions.queue.iter().any(|a| matches!(a.about, crate::dialog::Question::CareerPath { sim, .. } if sim == e)) {
            continue;
        }
        job.reward_due = false;
        let level = job.info();
        let Some(reward) = reward_of(job.career().key, level.reward, &mut rand::rng()) else { continue };
        let title = level.title_for(sim.female);
        match reward {
            Reward::Object(name) => {
                let Some(c) = baked.as_ref().and_then(|b| b.0.catalog.iter().find(|c| c.instance_name.eq_ignore_ascii_case(name))) else {
                    warn!("career reward: no catalogue object {name}");
                    continue;
                };
                family.0.push(crate::familyinv::stored(c.objd, None, None, 0));
                info!("career reward: {} got {} ({name}) for reaching {title}", sim.first, c.name);
                let words = ui.as_deref().and_then(|u| u.localize(OBJECT_REWARD)).map_or_else(
                    || format!("Congratulations! As a reward for your advancement to {title}, you've been allowed to keep the following \"acquisition\": {}! Look for it in your family inventory when you go home.", c.name),
                    |t| t.replace("{0.String}", title).replace("{1.ObjectName}", &c.name),
                );
                notes.push(words);
            }
            Reward::Book(skill) => {
                let Some(gu) = gu.as_deref() else { continue };
                let at = skills.level(skill);
                let open: Vec<&s3bake::gamedata::SkillBookInfo> = gu.data.skill_books.iter().filter(|b| b.skill == skill && b.min <= at && books.is_none_or(|s| s.pages(&b.key).is_none())).collect();
                let Some(b) = open.choose(&mut rand::rng()) else { continue };
                let (key, name) = (b.key.clone(), b.name.clone());
                commands.entity(e).queue(move |mut w: EntityWorldMut| {
                    if w.get::<crate::skillbooks::SkillBooks>().is_none() {
                        w.insert(crate::skillbooks::SkillBooks::default());
                    }
                    if let Some(mut s) = w.get_mut::<crate::skillbooks::SkillBooks>() {
                        s.0.push((key, 0.0));
                    }
                });
                info!("career reward: {} got the book {name}", sim.first);
                notes.push(format!("{} brought home a copy of {name} from the lab, for reaching {title}.", sim.first));
            }
            Reward::Harvest(what) => {
                let Some(gu) = gu.as_deref() else { continue };
                let plants = &gu.data.plants;
                if let Some(p) = plants.iter().find(|p| p.produce.eq_ignore_ascii_case(what)) {
                    let tier = 3u8;
                    let word = crate::gardening::QUALITIES[tier as usize].0;
                    crate::inventory::give(&mut commands, e, crate::inventory::ItemKind::Produce, p.produce.clone(), format!("{word} {}", p.produce), tier, p.price, 1);
                    notes.push(format!("{} brought home a {} from the lab, for reaching {title}.", sim.first, p.produce));
                } else if let Some(i) = plants.iter().position(|p| p.name.eq_ignore_ascii_case(what)) {
                    *garden.seeds.entry(i).or_insert(0) += 1;
                    notes.push(format!("{} brought home a {} from the lab, for reaching {title}.", sim.first, plants[i].seed));
                } else {
                    warn!("career reward: no plant {what}");
                    continue;
                }
                info!("career reward: {} got {what}", sim.first);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewards_by_career() {
        let mut rng = rand::rng();
        assert_eq!(reward_of("Culinary", "FoodProcessor", &mut rng), Some(Reward::Object("FoodProcessor")));
        assert!(matches!(reward_of("Criminal", "PaintingCars,SculptureGlobe", &mut rng), Some(Reward::Object("PaintingCars" | "SculptureGlobe"))));
        assert_eq!(reward_of("Science", "Fishing", &mut rng), Some(Reward::Book("Fishing")));
        assert!(matches!(reward_of("Science", "Flame Fruit,Lifefruit,Death Flower Bush", &mut rng), Some(Reward::Harvest(_))));
        assert_eq!(reward_of("LawEnforcement", "UnlockPoliceCar", &mut rng), None);
        assert_eq!(reward_of("Criminal", "Burglar", &mut rng), None);
        assert_eq!(reward_of("Music", "", &mut rng), None);
    }
}
