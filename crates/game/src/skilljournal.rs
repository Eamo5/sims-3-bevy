//! A skill's journal in the game's own dialog (UI.dll's `SkillJournalDialog` over its
//! `HUDSkillsJournalDialog` layout), opened from the skills panel's journal buttons: the Sim's
//! picture and name, a tab for each skill they have (the game's skewer tabs with the skill's
//! icon), the skill's name and level in bubbles ("Mastered" at the top), its description, its
//! statistics, and its challenges in the game's opportunity rows (each with its description and
//! how far along, a star when earned). Accept closes it.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

use crate::interact::Skills;
use crate::layout::{SetIcon, Spawned, UiAssets, UiButton, edit_windows};
use crate::sim::{Selected, Sim};
use crate::{AppState, PlayMode};

pub struct SkillJournalPlugin;

impl Plugin for SkillJournalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JournalDialog>()
            .add_systems(Update, journal_dialog.run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(AppState::InGame), |mut d: ResMut<JournalDialog>| *d = JournalDialog::default());
    }
}

/// Whether the game's own journal dialog shows a skill's journal (the interim one stands aside).
pub static GAME_JOURNAL: AtomicBool = AtomicBool::new(false);

// `HUDSkillsJournalDialog`'s windows.
const SIM_THUMB: u32 = 0x05ef_6bd2;
const TITLE: u32 = 0x05ef_6bd4;
const SKILL_NAME: u32 = 0x05ef_6bd5;
const BUBBLES: u32 = 0x05ef_6bdb;
const MASTERED: u32 = 0x05ef_6bdf;
const TABS: u32 = 0x05ef_6bd6;
const DESCRIPTION: u32 = 0x05ef_6be0;
const STATISTICS: u32 = 0x05ef_6be1;
const OPPORTUNITIES: u32 = 0x05ef_6be3;
const OPPORTUNITY_HOLDER: u32 = 0x05ef_6be2;
const NO_SKILLS: u32 = 0x05ef_6bda;
const ACCEPT: u32 = 0x05ef_6bd1;
/// What this journal hasn't: the skill's table.
const NOT_HERE: [u32; 2] = [0x05ef_6bd0, 0x05ef_6be4];
// `HUDSkillsJournalOppEntry`'s.
const OPP_ICON: u32 = 0x05ef_6bf0;
const OPP_NAME: u32 = 0x05ef_6bf1;
const OPP_DESCRIPTION: u32 = 0x05ef_6bf2;
/// A bubble meter's first bubble; in each, 2 lit and 3 unlit.
const BUBBLE_FIRST: u32 = 0x000b_0101;
/// The tabs (the game's `SkewerTabControl`, as `SkewerTabControlInstance` makes them), a step
/// apart.
const TAB_STEP: f32 = 54.0;
/// A line of the journal's words.
const LINE: f32 = 17.0;
/// A challenge's row height.
const ROW: f32 = 64.0;

/// The dialog up, its skill tabs, and what it shows.
#[derive(Resource, Default)]
pub struct JournalDialog {
    s: Option<Spawned>,
    tabs: Vec<(Entity, &'static str)>,
    rows: Option<Entity>,
    shown: Option<String>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn journal_dialog(
    mut commands: Commands,
    mut d: ResMut<JournalDialog>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    mut open: ResMut<crate::simpanel::OpenJournal>,
    sel: Query<
        (Entity, &Sim, &Skills, (Option<&crate::journal::SkillJournal>, &crate::social::Relationships, Option<&crate::meals::KnownRecipes>), Option<&crate::chess::ChessRecord>, Option<&crate::writing::Author>),
        With<Selected>,
    >,
    mut gu: Option<ResMut<crate::icons::GameUi>>,
    mut portraits: ResMut<crate::portraits::Portraits>,
    clicks: Query<(Entity, &Interaction), Changed<Interaction>>,
) {
    let Some(mut ui) = ui else { return };
    let ready = ui.layout("HUDSkillsJournalDialog").is_some() && ui.layout("HUDSkillsJournalOppEntry").is_some() && ui.layout("SkewerTabControl").is_some();
    GAME_JOURNAL.store(ready, Ordering::Relaxed);
    if !ready {
        return;
    }
    let close = |commands: &mut Commands, d: &mut JournalDialog| {
        if let Some(r) = d.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        *d = JournalDialog::default();
    };
    let (Some(skill), Ok((e, sim, skills, (journal, rels, recipes), chess, author))) = (open.0, sel.single()) else {
        close(&mut commands, &mut d);
        return;
    };
    // Accept closes it; a tab shows its skill.
    if let Some(s) = &d.s {
        if crate::livehud::pressed(&clicks, s.id(ACCEPT)) {
            open.0 = None;
            close(&mut commands, &mut d);
            return;
        }
        if let Some((_, k)) = d.tabs.iter().find(|(t, _)| crate::livehud::pressed(&clicks, Some(*t))) {
            open.0 = Some(k);
            return;
        }
    }
    let mut list: Vec<(&'static str, f32)> = skills.0.iter().filter(|(_, v)| **v > 0.01).map(|(k, v)| (*k, *v)).collect();
    list.sort_by(|a, b| b.1.total_cmp(&a.1));
    let measures = crate::journal::Measures { journal, rels, cooking: skills.level("Cooking"), recipes, chess, author, data: gu.as_deref().map(|g| &*g.data), sim: Some((sim, skills)) };
    let stats: Vec<String> = crate::journal::statistics(skill).iter().map(|(label, m)| format!("{label}: {}", crate::journal::shown(*m, measures.value(*m)))).collect();
    let challenges: Vec<(String, String, bool, f64, f64)> =
        crate::journal::CHALLENGES.iter().filter(|c| c.skill == skill).map(|c| (c.name.to_string(), c.description(), crate::journal::earned(journal, c.name), measures.value(c.measure), c.need)).collect();
    let key = format!("{e:?} {skill} {:?} {stats:?} {:?} {}", list.iter().map(|(k, v)| (*k, (*v * 20.0) as i32)).collect::<Vec<_>>(), challenges.iter().map(|c| (c.2, (c.3 * 10.0) as i64)).collect::<Vec<_>>(), gu.is_some());
    if d.shown.as_ref() == Some(&key) {
        return;
    }
    // (Another Sim or skill: the dialog afresh, its parts as tall as their words.)
    let fresh = d.shown.as_ref().is_none_or(|k| !k.starts_with(&format!("{e:?} {skill} ")));
    // The dialog, put up in the middle of the screen (as the game centres it), for this Sim.
    if fresh || d.s.is_none() {
        close(&mut commands, &mut d);
        let Some(mut w) = ui.layout("HUDSkillsJournalDialog").cloned() else { return };
        let info = gu.as_deref().and_then(|g| g.data.skill(skill).cloned());
        let desc_lines = info.as_ref().map_or(1, |i| i.desc.chars().count() / 84 + 1) as f32;
        let stat_lines = stats.len().max(1) as f32;
        edit_windows(&mut w, DESCRIPTION, &mut |t| t.area[3] = t.area[1] + desc_lines * LINE);
        edit_windows(&mut w, STATISTICS, &mut |t| t.area[3] = t.area[1] + stat_lines * LINE);
        // (The challenges under the statistics, which hang under the description: the
        // statistics' words start 31 down their block, 8 below the description.)
        let top = desc_lines * LINE + 8.0 + 31.0 + stat_lines * LINE + 6.0;
        edit_windows(&mut w, OPPORTUNITIES, &mut |t| {
            let h = t.area[3] - t.area[1];
            (t.area[1], t.area[3]) = (top, top + h);
        });
        let (wd, ht) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
        w.area = [0.0, 0.0, wd, ht];
        w.place = s3bake::ui::UiPlace::Center(0.5);
        w.flags |= s3bake::ui::WIN_VISIBLE;
        let s = ui.spawn_root(&mut commands, &mut images, &mut fonts, &w);
        let Some(root) = s.root else { return };
        commands.entity(root).insert((GlobalZIndex(21), crate::hud::BlocksWorld, Interaction::default(), DespawnOnExit(AppState::InGame)));
        for id in NOT_HERE {
            for x in s.all_with(id) {
                commands.entity(x).insert(Visibility::Hidden);
            }
        }
        if let Some(t) = s.id(SIM_THUMB) {
            let h = portraits.portrait(&mut images, e);
            commands.entity(t).despawn_children();
            commands.entity(t).with_children(|c| {
                c.spawn((ImageNode::new(h), crate::portraits::PortraitOf(e), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
            });
        }
        let title = ui.localize("Ui/Caption/HUD/SkillJournal:Title").map(|t| crate::layout::gendered(&t.replace("{0.SimFirstName}", &sim.first), sim.female)).unwrap_or_else(|| format!("{}'s Skill Journal", sim.first));
        if let Some(t) = s.text(TITLE) {
            commands.entity(t).insert(Text::new(title));
        }
        // A tab for each skill they have, its icon on it.
        if let (Some(row), Some(tab)) = (s.id(TABS), ui.layout("SkewerTabControl").cloned()) {
            for (n, (k, _)) in list.iter().enumerate() {
                let mut t = tab.clone();
                let x = n as f32 * TAB_STEP;
                t.area = [x + 1.0, 0.0, x + 55.0, 58.0];
                t.place = s3bake::ui::UiPlace::Fixed;
                t.flags |= s3bake::ui::WIN_VISIBLE;
                let ts = ui.spawn_under(&mut commands, &mut images, &mut fonts, &t, row);
                let Some(te) = ts.root else { continue };
                let info = gu.as_deref().and_then(|g| g.data.skill(k).cloned());
                commands.entity(te).insert(crate::icons::Tooltip(info.as_ref().map_or(k.to_string(), |i| i.name.clone())));
                if let Some(h) = info.as_ref().and_then(|i| gu.as_deref_mut().and_then(|g| g.icon(&mut images, &i.icon))) {
                    commands.entity(te).insert(SetIcon(h));
                }
                d.tabs.push((te, k));
            }
        }
        d.s = Some(s);
    }
    d.shown = Some(key);
    let Some(s) = d.s.clone() else { return };
    // (Through commands: on opening, the windows are only now being made.)
    for (t, k) in &d.tabs {
        let on = *k == skill;
        commands.entity(*t).queue(move |mut e: EntityWorldMut| {
            if let Some(mut b) = e.get_mut::<UiButton>()
                && b.selected != on
            {
                b.selected = on;
            }
        });
    }
    let say = |commands: &mut Commands, id: u32, t: String| {
        if let Some(x) = s.text(id) {
            commands.entity(x).insert(Text::new(t));
        }
    };
    // The skill: its name, level in bubbles, description and statistics.
    let info = gu.as_deref().and_then(|g| g.data.skill(skill).cloned());
    let max = info.as_ref().map_or(10, |i| i.max_level.max(1));
    let v = skills.0.get(skill).copied().unwrap_or(0.0);
    let level = (v as u32).min(max);
    say(&mut commands, SKILL_NAME, info.as_ref().map_or(skill.to_string(), |i| i.name.clone()));
    say(&mut commands, DESCRIPTION, info.as_ref().map(|i| i.desc_for(sim.female).replace("{0.SimFirstName}", &sim.first)).unwrap_or_default());
    say(&mut commands, STATISTICS, if stats.is_empty() { "—".to_string() } else { stats.join("\n") });
    if let Some(m) = s.id(MASTERED) {
        commands.entity(m).insert(if level >= max { Visibility::Inherited } else { Visibility::Hidden });
    }
    if let Some(meter) = s.id(BUBBLES) {
        let fill = if max < 10 { (level * (10 / max.max(1))) as usize } else { (level / (max / 10).max(1)) as usize };
        for b in 0..10u32 {
            let Some(bubble) = s.within(meter, BUBBLE_FIRST + b) else { continue };
            for (id, on) in [(2, (b as usize) < fill), (3, (b as usize) >= fill), (4, false), (5, false)] {
                if let Some(x) = s.within(bubble, id) {
                    commands.entity(x).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
        }
    }
    if let Some(x) = s.id(NO_SKILLS) {
        commands.entity(x).insert(if list.is_empty() { Visibility::Inherited } else { Visibility::Hidden });
    }
    // The challenges, in the game's opportunity rows, under the statistics (the section's own
    // heading: the game's "Opportunities", renamed).
    if let Some(t) = s.id(OPPORTUNITIES).and_then(|w| s.comment_within(w, "Opportunities")).and_then(|w| s.text_of(w)) {
        commands.entity(t).insert(Text::new(ui.localize("Ui/Caption/HUD/SkillJournal:Challenges").unwrap_or_else(|| "Skill Challenges".into())));
    }
    if let Some(old) = d.rows.take() {
        commands.entity(old).try_despawn();
    }
    let (Some(holder), Some(template)) = (s.id(OPPORTUNITY_HOLDER), ui.layout("HUDSkillsJournalOppEntry").cloned()) else { return };
    let rows = commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(30.0), right: Val::Px(0.0), height: Val::Px(challenges.len() as f32 * ROW), ..default() }, Pickable::IGNORE, ChildOf(holder))).id();
    d.rows = Some(rows);
    let star = gu.as_deref_mut().and_then(|g| g.icon(&mut images, "w_lifetime_happiness"));
    for (n, (name, desc, earned, have, need)) in challenges.iter().enumerate() {
        let mut w = template.clone();
        let y = n as f32 * ROW;
        w.area = [0.0, y, 480.0, y + ROW - 8.0];
        w.place = s3bake::ui::UiPlace::Fixed;
        let progress = if *earned { "Earned".to_string() } else { format!("{:.0} of {:.0}", have.min(*need), need) };
        edit_windows(&mut w, OPP_NAME, &mut |t| t.caption = format!("{name} ({progress})"));
        edit_windows(&mut w, OPP_DESCRIPTION, &mut |t| {
            t.caption = desc.clone();
            // (Room for a few lines: the challenges say more than an opportunity's line.)
            t.area[3] = t.area[1] + ROW - 18.0;
        });
        let r = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, rows);
        if let Some(icon) = r.id(OPP_ICON) {
            match (&star, earned) {
                (Some(h), true) => crate::hudpanels::picture(&mut commands, icon, h.clone(), Color::WHITE),
                _ => {
                    commands.entity(icon).insert(Visibility::Hidden);
                }
            }
        }
    }
}
