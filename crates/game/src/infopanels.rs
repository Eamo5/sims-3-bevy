//! The live HUD's Relationships, Opportunities and Lifetime Rewards panels in the game's own
//! layouts (`HUDRelationshipsPanel`, `HUDOpportunitiesPanel`, `HUDRewardTraitsPanel`), driven as
//! UI.dll's `RelationshipsPanel`, `OpportunitiesPanel` and `RewardTraitsPanel` drive them.
//!
//! Relationships: everyone the selected Sim knows, a card each (`HudRelationshipsPanelEntry`:
//! their portrait, the relationship bar from -100 to 100, its state's icon, a relative's or a
//! co-worker's mark), the last one talked to first, then visitors, then the closest; filtered by
//! all, family, friends, co-workers or visitors, scrolled sideways with the wheel. Opportunities:
//! the three slots (career, skill, special) with their icons, the chosen one's name, task,
//! deadline and description; a right click cancels one. Lifetime Rewards: the happiness to
//! spend, the rewards bought, and Purchase.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::layout::{Spawned, UiAssets, UiButton, UiFillBar};
use crate::livehud::{InfoPanel, pressed, set_text, set_visible};
use crate::sim::{HouseholdMember, Relationships, Selected, Sim};
use crate::{AppState, PlayMode};

pub struct InfoPanelsPlugin;

impl Plugin for InfoPanelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PlayMode::Live), spawn_panels).add_systems(
            Update,
            (keys, show, relationships_panel, opportunities_panel, rewards_panel).chain().run_if(in_state(PlayMode::Live)).run_if(resource_exists::<InfoPanels>),
        );
    }
}

/// Whether the game's own panels are up (the old windows' keys and buttons stand aside).
pub static GAME_PANELS: AtomicBool = AtomicBool::new(false);

// Relationships (`RelationshipsPanel.ControlIDs`).
const REL_GRID: u32 = 0x968b_450a;
const REL_ALL: u32 = 0x968b_4505;
const REL_FAMILY: u32 = 0x968b_4506;
const REL_FRIENDS: u32 = 0x968b_4507;
const REL_COWORKERS: u32 = 0x968b_4508;
const REL_VISITORS: u32 = 0x968b_4509;
/// An entry's portrait, words, bar, visitor mark, relative's, co-worker's, state icon.
const ENTRY_THUMB: u32 = 2;
const ENTRY_TEXT: u32 = 3;
const ENTRY_BAR: u32 = 5;
const ENTRY_VISITOR: u32 = 9;
const ENTRY_RELATIVE: u32 = 0x21;
const ENTRY_COWORKER: u32 = 0x22;
const ENTRY_LTR: u32 = 0x24;
// Opportunities (`OpportunitiesPanel.ControlIDs`).
const OPP_BUTTON: u32 = 0x06ef_61c0;
const OPP_ICON: u32 = 2;
const OPP_NAME: u32 = 0x06ef_61b0;
const OPP_TYPE: u32 = 1;
const OPP_PROGRESS: u32 = 0x06ef_61b1;
const OPP_DESCRIPTION: u32 = 0x06ef_61b3;
const OPP_SCROLLING: u32 = 0x06ef_61b4;
const OPP_TABS: u32 = 0x06ef_61d0;
// Lifetime rewards (`RewardTraitsPanel.ControlIDs`).
const REW_GRID: u32 = 0x2fa5_1c01;
const REW_POINTS: u32 = 0x2fa5_1c05;
const REW_PURCHASE: u32 = 0x2fa5_1c07;
const REW_NONE: u32 = 0x2fa5_1c08;
const REW_CLOSE: u32 = 0x5d4e_1c01;
const REW_ICON: u32 = 0x2fa5_1c11;
const REW_MASK: u32 = 0x2fa5_1c10;

/// The empty slots' pictures and words (`mPlaceholderOppIcons`, `Opportunity1..3`).
const OPP_PLACEHOLDERS: [(&str, &str, &str); 3] = [
    ("opp_generic_career", "Ui/Caption/Opportunities:Opportunity1", "Ui/Caption/Opportunities:Opp1Desc"),
    ("opp_generic_skill", "Ui/Caption/Opportunities:Opportunity2", "Ui/Caption/Opportunities:Opp2Dec"),
    ("opp_generic", "Ui/Caption/Opportunities:Opportunity3", "Ui/Caption/Opportunities:Opp3Desc"),
];

/// The relationships' filters (`RelationshipCategory`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Filter {
    #[default]
    All,
    Family,
    Friends,
    Coworkers,
    Visitors,
}

/// The panels on screen, and what they show.
#[derive(Resource)]
pub struct InfoPanels {
    rel: Spawned,
    opp: Spawned,
    rew: Spawned,
    filter: Filter,
    /// The first card shown (scrolled sideways), the cards' holder, and what they show.
    scroll: usize,
    cards: Option<Entity>,
    shown: Vec<(Entity, i32, u8, u8)>,
    /// The opportunity slot chosen, and what the slots show.
    opp_slot: usize,
    opp_shown: Vec<Option<usize>>,
    /// The rewards' holder, and what it shows.
    rewards: Option<Entity>,
    rew_shown: (u32, Vec<String>),
    /// The one last talked to (first in the list, as `sLastInteractedWith`).
    last_talked: Option<Entity>,
}

/// A relationship card's portrait (clicked: the camera goes to them).
#[derive(Component)]
struct Card(Entity);

fn spawn_panels(mut commands: Commands, ui: Option<ResMut<UiAssets>>, (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>), old: Option<Res<InfoPanels>>) {
    let Some(mut ui) = ui else { return };
    if !crate::livehud::active(Some(&ui)) {
        return;
    }
    if let Some(o) = old {
        for s in [&o.rel, &o.opp, &o.rew] {
            if let Some(r) = s.root {
                commands.entity(r).try_despawn();
            }
        }
    }
    let mut spawn = |name: &str| -> Spawned {
        let s = ui.spawn(&mut commands, &mut images, &mut fonts, name).unwrap_or_default();
        if let Some(r) = s.root {
            commands.entity(r).insert((DespawnOnExit(AppState::InGame), GlobalZIndex(5), Visibility::Hidden));
        }
        s
    };
    let rel = spawn("HUDRelationshipsPanel");
    let opp = spawn("HUDOpportunitiesPanel");
    let rew = spawn("HUDRewardTraitsPanel");
    if rel.root.is_none() || opp.root.is_none() || rew.root.is_none() {
        return;
    }
    // (The grid and the opportunity's words take the wheel; Purchase opens the rewards.)
    for e in [rel.id(REL_GRID), opp.id(OPP_SCROLLING)].into_iter().flatten() {
        commands.entity(e).remove::<Pickable>().insert((Interaction::default(), crate::hud::BlocksWorld));
    }
    if let Some(e) = rew.id(REW_PURCHASE) {
        commands.entity(e).insert(crate::hud::RewardsButton);
    }
    for (id, tip) in [(REL_ALL, "All"), (REL_FAMILY, "Family"), (REL_FRIENDS, "Friends"), (REL_COWORKERS, "Co-workers"), (REL_VISITORS, "Visitors")] {
        if let Some(e) = rel.id(id) {
            commands.entity(e).insert(crate::icons::Tooltip(tip.into()));
        }
    }
    // (Its expansion tabs and its own close button are for the packs' journals.)
    for (s, id) in [(&opp, OPP_TABS), (&rew, REW_CLOSE)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    GAME_PANELS.store(true, Ordering::Relaxed);
    commands.insert_resource(InfoPanels {
        rel,
        opp,
        rew,
        filter: Filter::All,
        scroll: 0,
        cards: None,
        shown: Vec::new(),
        opp_slot: 0,
        opp_shown: Vec::new(),
        rewards: None,
        rew_shown: (u32::MAX, Vec::new()),
        last_talked: None,
    });
}

/// R and O open the relationships and the opportunities, as before.
fn keys(keys: Res<ButtonInput<KeyCode>>, buy: Res<crate::buy::BuyMode>, mut panel: ResMut<InfoPanel>) {
    if buy.active {
        return;
    }
    for (k, t) in [(KeyCode::KeyR, InfoPanel::Relationships), (KeyCode::KeyO, InfoPanel::Opportunities)] {
        if keys.just_pressed(k) {
            *panel = if *panel == t { InfoPanel::None } else { t };
        }
    }
}

/// Each panel shows under its tab, in live mode.
fn show(p: Res<InfoPanels>, buy: Res<crate::buy::BuyMode>, panel: Res<InfoPanel>, mut vis: Query<&mut Visibility>) {
    if !buy.is_changed() && !panel.is_changed() && !p.is_added() {
        return;
    }
    let live = !buy.active;
    set_visible(&mut vis, p.rel.root, live && *panel == InfoPanel::Relationships);
    set_visible(&mut vis, p.opp.root, live && *panel == InfoPanel::Opportunities);
    set_visible(&mut vis, p.rew.root, live && *panel == InfoPanel::RewardTraits);
}

/// A holder for fresh children under a window (cleared).
fn holder(commands: &mut Commands, parent: Entity, slot: &mut Option<Entity>, vis: &Query<&mut Visibility>) -> Entity {
    match slot.filter(|h| vis.contains(*h)) {
        Some(h) => {
            commands.entity(h).despawn_children();
            h
        }
        None => {
            let h = commands
                .spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, Visibility::Inherited, Pickable::IGNORE, ChildOf(parent)))
                .id();
            *slot = Some(h);
            h
        }
    }
}

/// A relationship's state as the game names its icon (`GetLTRRelationshipImageKey`).
fn ltr_icon(r: &crate::social::Relationship) -> &'static str {
    use crate::social::RelStatus;
    match r.status {
        RelStatus::Married => "relationships_state_spouse",
        RelStatus::Engaged => "relationships_state_fiancee",
        RelStatus::Partner => "relationships_state_partners",
        RelStatus::Ex => "relationships_state_ex_romance",
        RelStatus::None if r.romance >= 20.0 => "relationships_state_date",
        RelStatus::None => match r.friendship {
            v if v < -60.0 => "relationships_state_enemy",
            v if v < -20.0 => "relationships_state_disliked",
            v if v < crate::social::FRIEND_THRESHOLD => "relationships_state_aquaintance",
            v if v < 40.0 => "relationships_state_friend",
            v if v < 75.0 => "relationships_state_friend_good",
            _ => "relationships_state_friend_best",
        },
    }
}

/// The relationships panel.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn relationships_panel(
    mut commands: Commands,
    mut p: ResMut<InfoPanels>,
    panel: Res<InfoPanel>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    selected: Query<(Entity, &Sim, &Relationships, Option<&crate::careers::Job>), With<Selected>>,
    people: Query<(&Sim, Option<&crate::careers::Job>, Has<HouseholdMember>, Has<crate::interact::Visitor>, &InheritedVisibility, &GlobalTransform)>,
    (family, mut portraits): (Res<crate::family::Genealogy>, ResMut<crate::portraits::Portraits>),
    (clicks, cards, hovered): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &Card), Changed<Interaction>>, Query<&Interaction>),
    (vis, mut buttons): (Query<&mut Visibility>, Query<&mut UiButton>),
    mut cam: Query<&mut crate::camera::SimsCamera>,
    (mut wheel, mut events): (MessageReader<MouseWheel>, MessageReader<crate::life::LifeEvent>),
) {
    let scrolled: f32 = wheel.read().map(|w| w.y.signum()).sum();
    let Ok((me, my_sim, rels, my_job)) = selected.single() else { return };
    for ev in events.read() {
        if ev.sim == me
            && let crate::life::LifeEventKind::Socialized { other, .. } = ev.kind
        {
            p.last_talked = Some(other);
        }
    }
    if *panel != InfoPanel::Relationships {
        return;
    }
    let Some(mut ui) = ui else { return };
    let p = &mut *p;
    // The filters.
    for (id, f) in [(REL_ALL, Filter::All), (REL_FAMILY, Filter::Family), (REL_FRIENDS, Filter::Friends), (REL_COWORKERS, Filter::Coworkers), (REL_VISITORS, Filter::Visitors)] {
        if pressed(&clicks, p.rel.id(id)) {
            p.filter = f;
            p.scroll = 0;
            p.shown.clear();
        }
        if let Some(e) = p.rel.id(id)
            && let Ok(mut b) = buttons.get_mut(e)
            && b.selected != (p.filter == f)
        {
            b.selected = p.filter == f;
        }
    }
    // A click on a card: the camera goes to them, if they're about.
    for (i, c) in &cards {
        if *i == Interaction::Pressed
            && let Ok((_, _, _, _, v, tf)) = people.get(c.0)
            && v.get()
            && let Ok(mut c) = cam.single_mut()
        {
            c.look_at(tf.translation());
        }
    }
    // Who's listed, in the game's order.
    let mut known: Vec<(Entity, &crate::social::Relationship, &Sim, bool, bool, bool)> = rels
        .0
        .iter()
        .filter_map(|(e, r)| {
            let (sim, job, member, visiting, v, _) = people.get(*e).ok()?;
            let relative = member || family.word(my_sim.id, sim.id).is_some();
            let coworker = matches!((job, my_job), (Some(a), Some(b)) if a.track == b.track);
            // (Visitors: those over at the household's lot now.)
            Some((*e, r, sim, relative, coworker, visiting && v.get()))
        })
        .filter(|(_, r, _, relative, coworker, visitor)| match p.filter {
            Filter::All => true,
            Filter::Family => *relative,
            Filter::Friends => r.friendship >= crate::social::FRIEND_THRESHOLD,
            Filter::Coworkers => *coworker,
            Filter::Visitors => *visitor,
        })
        .collect();
    let last = p.last_talked;
    known.sort_by(|a, b| {
        (b.0 == last.unwrap_or(Entity::PLACEHOLDER))
            .cmp(&(a.0 == last.unwrap_or(Entity::PLACEHOLDER)))
            .then(b.5.cmp(&a.5))
            .then(b.1.friendship.total_cmp(&a.1.friendship))
    });
    let Some(grid) = p.rel.id(REL_GRID) else { return };
    let Some(g) = ui.find("HUDRelationshipsPanel", REL_GRID).and_then(|w| w.grid) else { return };
    let columns = g.columns.max(1) as usize;
    // Sideways with the wheel over it.
    let over = hovered.get(grid).is_ok_and(|i| *i != Interaction::None);
    if over && scrolled != 0.0 {
        let most = known.len().saturating_sub(columns);
        p.scroll = if scrolled > 0.0 { p.scroll.saturating_sub(1) } else { (p.scroll + 1).min(most) };
    }
    p.scroll = p.scroll.min(known.len().saturating_sub(columns));
    let mut shown: Vec<(Entity, i32, u8, u8)> = known.iter().skip(p.scroll).take(columns).map(|k| (k.0, k.1.friendship.round() as i32, k.1.status as u8, k.3 as u8 | (k.4 as u8) << 1 | (k.5 as u8) << 2)).collect();
    shown.push((me, p.scroll as i32, p.filter as u8, 0));
    if shown == p.shown {
        return;
    }
    p.shown = shown;
    let Some(template) = ui.layout("HudRelationshipsPanelEntry").cloned() else { return };
    let h = holder(&mut commands, grid, &mut p.cards, &vis);
    commands.entity(grid).insert(Node { overflow: Overflow::clip(), ..ui_node(&ui, "HUDRelationshipsPanel", REL_GRID) });
    let step = g.cell[0] + g.cell_padding[0] + g.cell_padding[2];
    for (n, (e, r, sim, relative, coworker, visitor)) in known.iter().skip(p.scroll).take(columns).enumerate() {
        let x = g.padding[0] + n as f32 * step + g.cell_padding[0];
        let mut w = template.clone();
        let (cw, ch) = (w.area[2] - w.area[0], w.area[3] - w.area[1]);
        w.area = [x, g.padding[1] + g.cell_padding[1], x + cw, g.padding[1] + g.cell_padding[1] + ch];
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, h);
        let label = match family.word(my_sim.id, sim.id) {
            Some(word) => format!("{word} · {}", r.label_for(sim.female)),
            None => r.label_for(sim.female),
        };
        if let Some(t) = c.id(ENTRY_THUMB) {
            commands.entity(t).despawn_children();
            let pic = portraits.portrait(&mut images, *e);
            commands.entity(t).with_children(|c| {
                c.spawn((ImageNode::new(pic), crate::portraits::PortraitOf(*e), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Pickable::IGNORE));
            });
            commands.entity(t).remove::<Pickable>().insert((Card(*e), Interaction::default(), crate::hud::BlocksWorld, crate::icons::Tooltip(format!("{}\n{label}", sim.full_name()))));
        }
        if let Some(t) = c.text(ENTRY_TEXT) {
            commands.entity(t).insert(Text::new(label.clone()));
        }
        // The bar: -100 to 100, out from the middle.
        if let Some(b) = c.id(ENTRY_BAR) {
            let v = (r.friendship.clamp(-100.0, 100.0) + 100.0) / 200.0;
            commands.entity(b).queue(move |mut e: EntityWorldMut| {
                if let Some(mut f) = e.get_mut::<UiFillBar>() {
                    f.value = v;
                }
            });
        }
        if let (Some(win), Some((icon, _))) = (c.id(ENTRY_LTR), ui.image(&mut images, s3pkg::fnv64(ltr_icon(r)))) {
            crate::hudpanels::picture(&mut commands, win, icon, Color::WHITE);
            commands.entity(win).insert(Visibility::Inherited);
            crate::hudpanels::tip(&mut commands, win, r.label_for(sim.female));
        }
        for (id, on, tip) in [(ENTRY_RELATIVE, *relative, family.word(my_sim.id, sim.id).unwrap_or("Household")), (ENTRY_COWORKER, *coworker && !*relative, "Co-worker"), (ENTRY_VISITOR, *visitor, "Visitor")] {
            if let Some(win) = c.id(id) {
                commands.entity(win).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
                if on {
                    crate::hudpanels::tip(&mut commands, win, tip.to_string());
                }
            }
        }
    }
}

/// A window's node as designed (to change one property of it).
fn ui_node(ui: &UiAssets, layout: &str, id: u32) -> Node {
    let a = ui.find(layout, id).map(|w| w.area).unwrap_or_default();
    Node { position_type: PositionType::Absolute, left: Val::Px(a[0]), top: Val::Px(a[1]), width: Val::Px(a[2] - a[0]), height: Val::Px(a[3] - a[1]), ..default() }
}

/// The opportunities panel.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn opportunities_panel(
    mut commands: Commands,
    mut p: ResMut<InfoPanels>,
    panel: Res<InfoPanel>,
    ui: Option<ResMut<UiAssets>>,
    mut images: ResMut<Assets<Image>>,
    mut selected: Query<(&Sim, Option<&mut crate::opportunities::SimOpportunities>), With<Selected>>,
    (clock, mut game_ui, mut notes): (Res<crate::clock::GameClock>, Option<ResMut<crate::icons::GameUi>>, ResMut<crate::interact::Notifications>),
    (clicks, interactions, mouse): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<&Interaction>, Res<ButtonInput<MouseButton>>),
    (mut vis, mut texts, mut buttons): (Query<&mut Visibility>, Query<&mut Text>, Query<&mut UiButton>),
) {
    if *panel != InfoPanel::Opportunities {
        return;
    }
    let (Some(mut ui), Some(gu), Ok((sim, mut opps))) = (ui, game_ui.as_deref_mut(), selected.single_mut()) else { return };
    let data = gu.data.clone();
    let p = &mut *p;
    // The slots: a career's, a skill's and a special one, each in its own place, as the game's.
    let active: Vec<crate::opportunities::ActiveOpp> = opps.as_ref().map(|o| o.active.clone()).unwrap_or_default();
    let mut slots: [Option<usize>; 3] = [None; 3];
    let kind = |i: usize| data.opportunities.get(i).map_or(2, |o| if !o.career.is_empty() { 0 } else if !o.skill.is_empty() { 1 } else { 2 });
    let mut spare = Vec::new();
    for (n, a) in active.iter().enumerate() {
        let k = kind(a.index);
        if slots[k].is_none() { slots[k] = Some(n) } else { spare.push(n) }
    }
    for n in spare {
        if let Some(s) = slots.iter_mut().find(|s| s.is_none()) {
            *s = Some(n);
        }
    }
    // A left click shows one; a right click cancels it (`OnOpportunityMouseDown`).
    for k in 0..3u32 {
        let Some(b) = p.opp.id(OPP_BUTTON + k) else { continue };
        if pressed(&clicks, Some(b)) {
            p.opp_slot = k as usize;
            p.opp_shown.clear();
        }
        if mouse.just_pressed(MouseButton::Right)
            && interactions.get(b).is_ok_and(|i| *i != Interaction::None)
            && let (Some(n), Some(o)) = (slots[k as usize], opps.as_mut())
            && n < o.active.len()
        {
            let gone = o.active.remove(n);
            if let Some(info) = data.opportunities.get(gone.index) {
                notes.push(format!("{} cancelled the opportunity: {}.", sim.first, info.name));
            }
            p.opp_shown.clear();
        }
    }
    let shown: Vec<Option<usize>> = slots.iter().map(|s| s.map(|n| active[n].index)).chain([Some(p.opp_slot), Some((clock.minutes / 60.0) as usize)]).collect();
    if shown == p.opp_shown {
        return;
    }
    p.opp_shown = shown;
    for k in 0..3 {
        let b = p.opp.id(OPP_BUTTON + k as u32);
        if let Some(e) = b
            && let Ok(mut btn) = buttons.get_mut(e)
            && btn.selected != (p.opp_slot == k)
        {
            btn.selected = p.opp_slot == k;
        }
        let icon = match slots[k].and_then(|n| data.opportunities.get(active[n].index)) {
            Some(o) => gu.icon(&mut images, &o.icon),
            None => ui.image(&mut images, s3pkg::fnv64(OPP_PLACEHOLDERS[k].0)).map(|i| i.0),
        };
        if let (Some(b), Some(icon)) = (b, icon)
            && let Some(win) = p.opp.within(b, OPP_ICON)
        {
            crate::hudpanels::picture(&mut commands, win, icon, Color::WHITE);
        }
    }
    let k = p.opp_slot.min(2);
    let type_text = p.opp.within(p.opp.id(OPP_NAME).unwrap_or(Entity::PLACEHOLDER), OPP_TYPE);
    match slots[k].and_then(|n| Some((data.opportunities.get(active[n].index)?, &active[n]))) {
        Some((o, a)) => {
            set_text(&mut texts, p.opp.text(OPP_NAME), &o.name);
            set_visible(&mut vis, type_text, false);
            let mut progress = format!("{} at the {} between {} and {}", a.activity.name, crate::opportunities::venue_word(&o.rabbit_hole), crate::interact::hour_label(o.open), crate::interact::hour_label(o.close));
            if let Some(d) = a.deadline {
                let hours = ((d - clock.minutes) / 60.0).max(0.0);
                let (days, h) = ((hours / 24.0).floor(), (hours % 24.0).floor());
                progress.push_str(&format!("\nTime Left: {}", if days >= 1.0 { format!("{days:.0} day{}, {h:.0} hour{}", if days == 1.0 { "" } else { "s" }, if h == 1.0 { "" } else { "s" }) } else { format!("{h:.0} hour{}", if h == 1.0 { "" } else { "s" }) }));
            }
            set_text(&mut texts, p.opp.text(OPP_PROGRESS), &progress);
            let place = crate::opportunities::venue_word(&o.rabbit_hole);
            set_text(&mut texts, p.opp.text(OPP_DESCRIPTION), &crate::opportunities::fill(&o.desc, o, place).replace("{0.SimFirstName}", &sim.first));
        }
        None => {
            set_text(&mut texts, p.opp.text(OPP_NAME), "");
            set_visible(&mut vis, type_text, true);
            let (_, title, desc) = OPP_PLACEHOLDERS[k];
            if let Some(t) = type_text.and_then(|w| p.opp.text_of(w)) {
                set_text(&mut texts, Some(t), &ui.localize(title).unwrap_or_default());
            }
            set_text(&mut texts, p.opp.text(OPP_PROGRESS), &ui.localize(desc).unwrap_or_default());
            set_text(&mut texts, p.opp.text(OPP_DESCRIPTION), "");
        }
    }
}

/// The lifetime rewards panel.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn rewards_panel(
    mut commands: Commands,
    mut p: ResMut<InfoPanels>,
    panel: Res<InfoPanel>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    selected: Query<(&Sim, &crate::wishes::Wishes), With<Selected>>,
    mut game_ui: Option<ResMut<crate::icons::GameUi>>,
    (mut vis, mut texts): (Query<&mut Visibility>, Query<&mut Text>),
) {
    if *panel != InfoPanel::RewardTraits {
        return;
    }
    let (Some(mut ui), Some(gu), Ok((sim, w))) = (ui, game_ui.as_deref_mut(), selected.single()) else { return };
    let p = &mut *p;
    let shown = (w.points, w.rewards.clone());
    if shown == p.rew_shown {
        return;
    }
    p.rew_shown = shown;
    set_text(&mut texts, p.rew.text(REW_POINTS), &crate::lifetime::group(w.points as i64));
    set_visible(&mut vis, p.rew.id(REW_NONE), w.rewards.is_empty());
    if let Some(t) = p.rew.text(REW_NONE) {
        set_text(&mut texts, Some(t), &ui.localize("Ui/Caption/HUD/RewardTraitsPanel:NoRewards").unwrap_or_else(|| format!("{} has no lifetime rewards yet.", sim.first)));
    }
    let (Some(grid), Some(g), Some(template)) = (p.rew.id(REW_GRID), ui.find("HUDRewardTraitsPanel", REW_GRID).and_then(|w| w.grid), ui.layout("HUDRewardTraitsPanelEntry").cloned()) else { return };
    let h = holder(&mut commands, grid, &mut p.rewards, &vis);
    let data = gu.data.clone();
    let step = Vec2::new(g.cell[0] + g.cell_padding[0] + g.cell_padding[2], g.cell[1] + g.cell_padding[1] + g.cell_padding[3]);
    let cols = g.columns.max(1) as usize;
    for (n, r) in w.rewards.iter().enumerate() {
        let Some(t) = data.traits.iter().find(|t| t.hex == *r) else { continue };
        let (col, row) = ((n % cols) as f32, (n / cols) as f32);
        let (x, y) = (g.padding[0] + col * step.x + g.cell_padding[0], g.padding[1] + row * step.y + g.cell_padding[1]);
        let mut cell = template.clone();
        cell.area = [x, y, x + g.cell[0], y + g.cell[1]];
        let c = ui.spawn_under(&mut commands, &mut images, &mut fonts, &cell, h);
        if let (Some(win), Some(icon)) = (c.id(REW_ICON), gu.icon(&mut images, &t.icon)) {
            crate::hudpanels::picture(&mut commands, win, icon, Color::WHITE);
            crate::hudpanels::tip(&mut commands, win, format!("{}\n{}", t.name, t.desc.replace("{0.SimFirstName}", &sim.first)));
        }
        if let Some(m) = c.id(REW_MASK) {
            commands.entity(m).insert(Visibility::Hidden);
        }
    }
}
