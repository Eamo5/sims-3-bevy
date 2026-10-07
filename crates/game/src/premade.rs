//! The town's own families (the Goths, the Landgraabs, …, read from the world file at bake
//! time): play one of them, or meet them around town. Their names, ages, personalities, natural
//! hair colour and skin tone, careers, skills, marriages and friendships come from the game;
//! their clothes are picked from the CAS catalogue.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use s3bake::{HouseholdBaked, PremadesBaked};
use s3formats::premade::{self as pm, PremadeSim};

use crate::PlayMode;
use crate::careers::{Job, careers};
use crate::interact::{Household, Skills};
use crate::life::Trait;
use crate::sim::{Age, HouseholdMember, OutfitChoice, SKINS, Sim};
use crate::social::{RelStatus, Relationships};

pub struct PremadePlugin;

impl Plugin for PremadePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply_premade.run_if(in_state(PlayMode::Live)));
    }
}

/// The world's premade households.
#[derive(Resource, Clone)]
pub struct TownPremades(pub Arc<PremadesBaked>);

/// The premade household being played, whose careers, skills and relationships are applied
/// once they've moved in.
#[derive(Resource, Clone)]
pub struct PremadeChoice(pub HouseholdBaked);

pub fn age_of(flags: u32) -> Age {
    match flags {
        f if f & pm::AGE_ELDER != 0 => Age::Elder,
        f if f & pm::AGE_ADULT != 0 => Age::Adult,
        f if f & pm::AGE_YOUNG_ADULT != 0 => Age::YoungAdult,
        f if f & pm::AGE_TEEN != 0 => Age::Teen,
        f if f & pm::AGE_CHILD != 0 => Age::Child,
        f if f & pm::AGE_TODDLER != 0 => Age::Toddler,
        _ => Age::Baby,
    }
}

/// The game's trait name (`AbsentMinded`, `BookWorm`) as one of ours.
pub fn trait_of(name: &str) -> Option<Trait> {
    Trait::ALL.into_iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(name))
}

/// Our career track for the game's career class (`LawEnforcement`, `Political`, …).
pub fn career_of(class: &str) -> Option<usize> {
    let want: String = class.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_ascii_lowercase();
    careers().iter().position(|c| {
        let n: String = c.name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_ascii_lowercase();
        n == want || want.starts_with(&n)
    })
}

fn argb(c: u32) -> Color {
    Color::srgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Skin colour along the light-to-dark ramp for the game's skin shade (0 = lightest).
fn skin_of(shade: f32) -> Color {
    let t = shade.clamp(0.0, 1.0) * (SKINS.len() - 1) as f32;
    let i = (t.floor() as usize).min(SKINS.len() - 2);
    let f = t - i as f32;
    let (a, b) = (SKINS[i], SKINS[i + 1]);
    Color::srgb(a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f)
}

pub fn to_sim(p: &PremadeSim) -> Sim {
    let age = age_of(p.age);
    let mut traits: Vec<Trait> = p.traits.iter().filter_map(|t| trait_of(t)).collect();
    traits.dedup();
    traits.truncate(crate::life::trait_slots(age));
    // Clothes: deterministic per Sim; colours lean on their favourite colour.
    let hue = p.favourite_color.map(|c| Hsla::from(argb(c)).hue).unwrap_or((p.id % 360) as f32);
    let look = p.id.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15;
    Sim {
        id: p.id,
        outfit: OutfitChoice::default(),
        look,
        first: p.first_name.clone(),
        last: p.last_name.clone(),
        female: p.female,
        age,
        traits,
        favorites: crate::sim::Favorites::by_look(look),
        skin: skin_of(p.skin_shade),
        hair: p.hair_color.map(argb).unwrap_or(Color::srgb(0.3, 0.2, 0.1)),
        // (The town's Sims' eye colours are in their outfits, which the install doesn't have.)
        eyes: crate::sim::eyes_by_look(look),
        voice: (p.voice % 3) as u8,
        top: Color::hsl(hue, 0.5, 0.5),
        bottom: Color::hsl((hue + 180.0) % 360.0, 0.3, 0.3),
        weight: (p.fat - p.thin).clamp(-1.0, 1.0),
        fitness: p.fit.clamp(0.0, 1.0),
    }
}

impl TownPremades {
    /// Sims of the town who aren't in `household`, grown-ups first, for neighbours and townies.
    pub fn others(&self, household: Option<u64>) -> Vec<&PremadeSim> {
        let mut v: Vec<&PremadeSim> = self
            .0
            .households
            .iter()
            .filter(|h| Some(h.id) != household && !h.name.to_ascii_lowercase().contains("ghost"))
            .flat_map(|h| &h.members)
            .filter(|s| !s.first_name.is_empty())
            .collect();
        v.sort_by_key(|s| (s.age < pm::AGE_YOUNG_ADULT, s.id.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        v
    }

    pub fn relationship(&self, a: u64, b: u64) -> Option<&pm::PremadeRelationship> {
        self.0.relationships.iter().find(|r| (r.a == a && r.b == b) || (r.a == b && r.b == a))
    }
}

/// Once a premade family has moved in: their money, careers, skills, marriages and family ties,
/// and what the town's Sims already think of each other.
#[allow(clippy::type_complexity)]
fn apply_premade(
    mut commands: Commands,
    choice: Option<Res<PremadeChoice>>,
    town: Option<Res<TownPremades>>,
    mut household: Option<ResMut<Household>>,
    mut sims: Query<(Entity, &Sim, &mut Skills, &mut Relationships, Has<HouseholdMember>)>,
) {
    let Some(choice) = choice else { return };
    if household.is_none() || sims.iter().all(|q| !q.4) {
        return;
    }
    let h = &choice.0;
    if let Some(hh) = household.as_mut() {
        hh.funds = h.funds.max(0);
        hh.name = h.name.clone();
    }
    let by_id: HashMap<u64, Entity> = sims.iter().map(|q| (q.1.id, q.0)).collect();
    let premade: HashMap<u64, &PremadeSim> = h.members.iter().map(|m| (m.id, m)).collect();
    let mut links: Vec<(Entity, Entity, f32, f32, RelStatus)> = Vec::new();
    for (e, sim, mut skills, _, _) in &mut sims {
        let Some(p) = premade.get(&sim.id) else { continue };
        for (name, level) in &p.skills {
            if let Some(k) = crate::save::SKILLS.iter().find(|k| k.eq_ignore_ascii_case(name)) {
                skills.0.insert(k, (*level).clamp(0, 10) as f32);
            }
        }
        if let Some((class, level)) = &p.career
            && matches!(sim.age, Age::YoungAdult | Age::Adult | Age::Elder)
            && let Some(track) = career_of(class)
        {
            let mut job = Job::new(track);
            job.level = ((*level).max(1) as usize - 1).min(careers()[track].levels().len() - 1);
            commands.entity(e).insert(job);
        }
        let spouse = p.spouse.or(p.partner);
        if let Some(&other) = spouse.and_then(|s| by_id.get(&s)) {
            let status = if p.spouse.is_some() { RelStatus::Married } else { RelStatus::Partner };
            links.push((e, other, 80.0, 80.0, status));
        }
        for kin in p.parents.iter().chain(&p.children) {
            if let Some(&other) = by_id.get(kin) {
                links.push((e, other, 70.0, 0.0, RelStatus::None));
            }
        }
    }
    // Everyone else the town remembers.
    if let Some(town) = town {
        for r in &town.0.relationships {
            let (Some(&a), Some(&b)) = (by_id.get(&r.a), by_id.get(&r.b)) else { continue };
            let (romance, status) = match r.state.as_str() {
                "Spouse" => (80.0, RelStatus::Married),
                "Fiancee" => (70.0, RelStatus::Engaged),
                "Partner" => (60.0, RelStatus::Partner),
                "Romantic Interest" => (40.0, RelStatus::None),
                "Ex" | "Ex Spouse" => (0.0, RelStatus::Ex),
                _ => (0.0, RelStatus::None),
            };
            links.push((a, b, r.liking, romance, status));
        }
    }
    for (a, b, friendship, romance, status) in links {
        for (x, y) in [(a, b), (b, a)] {
            if let Ok((_, _, _, mut rels, _)) = sims.get_mut(x) {
                let r = rels.entry(y);
                r.friendship = r.friendship.max(friendship);
                r.romance = r.romance.max(romance);
                if status != RelStatus::None {
                    r.status = status;
                    r.kissed |= romance > 0.0;
                }
            }
        }
    }
    info!("premade: the {} household is home", h.name);
    commands.remove_resource::<PremadeChoice>();
}
