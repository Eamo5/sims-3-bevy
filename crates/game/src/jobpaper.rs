//! Finding a job, as the game's newspaper and computer have it: the day's few openings (three
//! in the paper, five on the computer, the same all day), shown one at a time in the game's
//! `CareerSelectDialog` as UI.dll's `CareerSelectionDialog` fills it: the career and the job
//! offered, its pay and hours, the days it's worked (today marked), what the listing says, the
//! career's icon and the Sim's face, arrows through the openings, and Accept or Cancel.

use bevy::prelude::*;
use rand::SeedableRng;
use rand::seq::SliceRandom;

use crate::layout::UiAssets;

pub struct JobPaperPlugin;

impl Plugin for JobPaperPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (open_listings, listing_buttons).chain().run_if(in_state(crate::PlayMode::Live)));
    }
}

/// A Sim done looking through the paper (or on the computer) for work: the listings go up.
#[derive(Resource)]
pub struct JobHunt {
    pub sim: Entity,
    pub computer: bool,
}

// `CareerSelectionDialog`'s windows.
const ACCEPT: u32 = 0x03e4_e800;
const SIM_IMAGE: u32 = 0x0669_ef04;
const CAREER_NAME: u32 = 0x0669_ef05;
const CAREER_TITLE: u32 = 0x0669_ef06;
const PAY: u32 = 0x0669_ef07;
const HOURS: u32 = 0x0669_ef08;
const DESCRIPTION: u32 = 0x0669_ef09;
/// Sunday's letter; Monday's to Saturday's follow.
const DAY_SUNDAY: u32 = 0x0669_ef20;
/// The arrow under Sunday; the rest follow.
const ARROW_SUNDAY: u32 = 0x0669_ef30;
const PREVIOUS: u32 = 0x06d5_2d00;
const NEXT: u32 = 0x06d5_2d01;
const JOB_ICON: u32 = 0x06d5_2d02;
const SEE_LOCATION: u32 = 0x06d5_2d03;
const CANCEL: u32 = 0x06d5_2d04;
/// The days' letters: worked, and not (`kDayTextWorkingColor`, `kDayTextNotWorkingColor`).
const DAY_WORKED: u32 = 0xff00_4040;
const DAY_OFF: u32 = 0x8080_8080;
/// `Newspaper.kFindJobNumJobsOpportunitiesPerDay` (the game's tuning), and the computer's.
const PAPER_OPENINGS: usize = 3;
const COMPUTER_OPENINGS: usize = 5;

/// The listings up: whose, the openings (career tracks), the one shown, and the windows
/// filled for each.
#[derive(Resource)]
struct Listings {
    root: Entity,
    sim: Entity,
    offers: Vec<usize>,
    at: usize,
    texts: [Option<Entity>; 5],
    days: [Option<Entity>; 7],
    icon: Option<Entity>,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum ListingButton {
    Previous,
    Next,
    Accept,
    Cancel,
}

/// The day's openings for a Sim: careers for their age (after-school jobs for teens, the rest
/// for grown-ups), not the one they have, chosen by the day (the same all day, as the game's
/// are).
fn openings(sim: &crate::sim::Sim, current: Option<usize>, day: u32, n: usize) -> Vec<usize> {
    use crate::sim::Age;
    let teen = sim.age == Age::Teen;
    if !(teen || sim.age.is_grown()) {
        return Vec::new();
    }
    let mut all: Vec<usize> = crate::careers::careers().iter().enumerate().filter(|(i, c)| c.part_time == teen && Some(*i) != current).map(|(i, _)| i).collect();
    let mut rng = rand::rngs::StdRng::seed_from_u64(day as u64 * 7919 + n as u64);
    all.shuffle(&mut rng);
    all.truncate(n);
    all
}

/// "9:00 am" (the listings' hours).
fn clock_label(h: f32) -> String {
    let h = h.rem_euclid(24.0);
    let (hour, minute) = (h.floor() as u32, ((h.fract() * 60.0).round() as u32).min(59));
    let twelve = if hour % 12 == 0 { 12 } else { hour % 12 };
    format!("{twelve}:{minute:02} {}", if hour < 12 { "am" } else { "pm" })
}

#[allow(clippy::too_many_arguments)]
fn open_listings(
    mut commands: Commands,
    hunt: Option<Res<JobHunt>>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    sims: Query<(&crate::sim::Sim, Option<&crate::careers::Job>)>,
    clock: Res<crate::clock::GameClock>,
    mut portraits: ResMut<crate::portraits::Portraits>,
    mut notes: ResMut<crate::interact::Notifications>,
    (shown, mut game_ui): (Option<Res<Listings>>, Option<ResMut<crate::icons::GameUi>>),
) {
    let Some(h) = hunt else { return };
    if shown.is_some() {
        return;
    }
    commands.remove_resource::<JobHunt>();
    let Ok((sim, job)) = sims.get(h.sim) else { return };
    let offers = openings(sim, job.map(|j| j.track), clock.day(), if h.computer { COMPUTER_OPENINGS } else { PAPER_OPENINGS });
    if offers.is_empty() {
        notes.push(format!("There are no jobs listed for {} today.", sim.first));
        return;
    }
    let Some(mut ui) = ui else { return };
    let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "CareerSelectDialog") else { return };
    let Some(root) = s.root else { return };
    // (No map here to see the place on: the button's away.)
    if let Some(e) = s.id(SEE_LOCATION) {
        commands.entity(e).insert(Visibility::Hidden);
    }
    // One opening: no arrows through them.
    for (id, b) in [(PREVIOUS, ListingButton::Previous), (NEXT, ListingButton::Next), (ACCEPT, ListingButton::Accept), (CANCEL, ListingButton::Cancel)] {
        if let Some(e) = s.id(id) {
            commands.entity(e).insert(b);
            if offers.len() < 2 && matches!(b, ListingButton::Previous | ListingButton::Next) {
                commands.entity(e).insert(Visibility::Hidden);
            }
        }
    }
    // Today, marked under its letter.
    let today = (clock.weekday() + 1) % 7;
    if let Some(e) = s.id(ARROW_SUNDAY + today as u32) {
        commands.entity(e).insert(Visibility::Inherited);
    }
    // The Sim's face.
    if let Some(w) = s.id(SIM_IMAGE) {
        let pic = portraits.portrait(&mut images, h.sim);
        commands.entity(w).insert((ImageNode::new(pic), crate::portraits::PortraitOf(h.sim)));
    }
    let texts = [CAREER_NAME, CAREER_TITLE, PAY, HOURS, DESCRIPTION].map(|id| s.text(id));
    let days = [0, 1, 2, 3, 4, 5, 6].map(|d| s.text(DAY_SUNDAY + d));
    let listings = Listings { root, sim: h.sim, offers, at: 0, texts, days, icon: s.id(JOB_ICON) };
    fill(&mut commands, &ui, game_ui.as_deref_mut(), &mut images, &listings, sim);
    commands.insert_resource(listings);
}

/// Shows the opening at hand.
fn fill(commands: &mut Commands, ui: &UiAssets, game_ui: Option<&mut crate::icons::GameUi>, images: &mut Assets<Image>, l: &Listings, sim: &crate::sim::Sim) {
    let Some(track) = l.offers.get(l.at).and_then(|&t| crate::careers::careers().get(t)) else { return };
    let level = &track.levels()[0];
    let pay = ui.localize("Ui/Caption/CareerSelection:Pay").unwrap_or_else(|| "{0.Money}/hour".into()).replace("{0.Money}", &format!("§{}", level.hourly));
    let desc = track.offer.replace("{0.SimFirstName}", &sim.first);
    let words = [track.name.to_string(), level.title_for(sim.female).to_string(), pay, format!("{} - {}", clock_label(level.start), clock_label(level.end)), desc];
    for (e, t) in l.texts.iter().zip(words) {
        if let Some(e) = e {
            commands.entity(*e).insert(Text::new(t));
        }
    }
    // (Our days: bit 0 Monday; the letters: Sunday first.)
    for (d, e) in l.days.iter().enumerate() {
        let bit = (d + 6) % 7;
        if let Some(e) = e {
            commands.entity(*e).insert(TextColor(crate::layout::color(if level.days & (1 << bit) != 0 { DAY_WORKED } else { DAY_OFF })));
        }
    }
    if let (Some(w), Some(gu)) = (l.icon, game_ui)
        && let Some(h) = gu.icon(images, track.icon)
    {
        commands.entity(w).insert(ImageNode::new(h));
    }
}

#[allow(clippy::too_many_arguments)]
fn listing_buttons(
    mut commands: Commands,
    buttons: Query<(&Interaction, &ListingButton), Changed<Interaction>>,
    listings: Option<ResMut<Listings>>,
    (ui, mut game_ui): (Option<Res<UiAssets>>, Option<ResMut<crate::icons::GameUi>>),
    mut images: ResMut<Assets<Image>>,
    sims: Query<&crate::sim::Sim>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut life: MessageWriter<crate::life::LifeEvent>,
) {
    let Some(mut l) = listings else { return };
    let Some(&(_, b)) = buttons.iter().find(|(i, _)| **i == Interaction::Pressed).as_ref() else { return };
    let Ok(sim) = sims.get(l.sim) else { return };
    match b {
        ListingButton::Previous | ListingButton::Next => {
            let n = l.offers.len();
            l.at = if *b == ListingButton::Next { (l.at + 1) % n } else { (l.at + n - 1) % n };
            if let Some(ui) = ui {
                fill(&mut commands, &ui, game_ui.as_deref_mut(), &mut images, &l, sim);
            }
        }
        ListingButton::Accept | ListingButton::Cancel => {
            commands.entity(l.root).try_despawn();
            if *b == ListingButton::Accept
                && let Some(&track) = l.offers.get(l.at)
            {
                let (j, note) = crate::careers::new_job(sim, track);
                notes.push(note);
                commands.entity(l.sim).insert(j);
                life.write(crate::life::LifeEvent::new(l.sim, crate::life::LifeEventKind::NewJob));
            }
            commands.remove_resource::<Listings>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_hours_read_as_the_game_writes_them() {
        assert_eq!(clock_label(9.0), "9:00 am");
        assert_eq!(clock_label(17.5), "5:30 pm");
        assert_eq!(clock_label(0.0), "12:00 am");
        assert_eq!(clock_label(12.0), "12:00 pm");
    }
}
