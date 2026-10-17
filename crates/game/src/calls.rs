//! Calls coming in, as the game's (`Phone.ConsiderGeneratingPhoneCall`, `RandomChatCall`,
//! `RandomInvitationCall`). Every six hours or so, from seven in the morning to ten at night,
//! someone the household knows may ring: 45 times in 95 no one, 42 a friend for a chat (someone
//! liking them 25 or more), 8 someone to hang out (40 or more). The cell phone rings and the one
//! called answers it (the game's clip), and the game's question comes up: "Bella would like to
//! chat. Does Mia want to talk?" (Sure: a chat on the phone), or "Hi Mia, this is Bella calling.
//! Do you want to hang out?" (At My Place: Bella comes over; I'll Come Over: Mia drives to hers,
//! where she has a home in town). Not picked up within 25 minutes, it's a missed call.

use bevy::prelude::*;
use rand::Rng;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::interact::{Action, ActionKind, ActionQueue, Invited, Notifications, OffLot, Phase};
use crate::sim::{HouseholdMember, Sim};
use crate::social::Relationships;

pub struct CallsPlugin;

impl Plugin for CallsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PhonesSilenced>().add_systems(Update, (place_calls, answer_calls, call_answers).chain().run_if(in_state(PlayMode::Live)));
        if std::env::var("CALL").is_ok() {
            app.add_systems(Update, call_test_friends.before(place_calls).run_if(in_state(PlayMode::Live)));
        }
    }
}

/// The household's phones silenced (the game's `SilenceUnsilencePhone`): no calls come in.
#[derive(Resource, Default)]
pub struct PhonesSilenced(pub bool);

/// The phone ringing for a Sim: who's calling, what for, and till when it rings.
#[derive(Component, Clone, Copy, Debug)]
pub struct Ringing {
    pub caller: Entity,
    pub invite: bool,
    pub until: f64,
    /// The caller's home in town (asked to hang out, it's theirs that's gone to).
    pub home: Option<usize>,
    asked: bool,
}

/// The chance of each kind of call at each look (the game's `kChanceForNoCall`,
/// `kChanceForRandomChat`, `kChanceForRandomInviteOver`), the hours between looks
/// (`kMinimumCallInterval`), the liking a caller needs for each (`kLTRThresholdForSimToBeCalled
/// ChatMin`, `...InvitedOverMin`), how long it rings (`kRandomCallTimeout`, minutes), and the
/// hours calls come in (`PhoneService`: not before seven, not after ten at night).
const WEIGHTS: [f32; 3] = [45.0, 42.0, 8.0];
const INTERVAL_HOURS: f64 = 6.0;
const CHAT_LIKING: f32 = 25.0;
const INVITE_LIKING: f32 = 40.0;
const RINGS_FOR: f64 = 25.0;
const CALL_HOURS: std::ops::Range<f32> = 7.0..22.0;
/// How long a caller asked over takes to come (game minutes).
const COMING_OVER: f64 = 45.0;

/// The phone answered (the game's), then the chat.
const ANSWER: crate::anim::ActionClip = crate::anim::ActionClip::new(
    Some("a2o_phone_answer_cellPhone_x"),
    &["a2o_phone_chat_talk_a_x", "a2o_phone_chat_talk_b_x", "a2o_phone_chat_listenRespond_agree_laugh_x", "a2o_phone_chat_talk_c_x", "a2o_phone_chat_talk_d_x"],
);

/// Which kind of call a roll of `r` (0 to the weights' sum) gives: none, a chat, an invitation.
pub fn kind_of(r: f32) -> usize {
    let mut acc = 0.0;
    for (i, w) in WEIGHTS.iter().enumerate() {
        acc += w;
        if r < acc {
            return i;
        }
    }
    0
}

/// Now and then, someone rings (in the hours calls come): one of those the household knows,
/// liked well enough, the better liked the likelier, to one of the household who's about.
#[allow(clippy::type_complexity)]
fn place_calls(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut next: Local<Option<f64>>,
    mut callees: Query<
        (Entity, &Sim, &Relationships, &mut ActionQueue),
        (
            With<HouseholdMember>,
            Without<Ringing>,
            Without<crate::interact::AtWork>,
            Without<crate::rabbitholes::AtRabbitHole>,
            Without<crate::visit::OnLot>,
            Without<crate::death::Dying>,
        ),
    >,
    callers: Query<(Entity, &Sim), (Or<(With<OffLot>, With<crate::town::Townie>)>, Without<HouseholdMember>, Without<Invited>)>,
    ringing: Query<(), With<Ringing>>,
    mut play: MessageWriter<crate::sound::PlaySound>,
    world: Res<crate::loading::CurrentWorld>,
    (town, story, dormant, household): (Option<Res<crate::premade::TownPremades>>, Res<crate::story::TownStory>, Res<crate::household::Dormant>, Option<Res<crate::interact::Household>>),
    silenced: Res<PhonesSilenced>,
) {
    if silenced.0 {
        return;
    }
    let now = clock.minutes;
    let due = *next.get_or_insert(now + INTERVAL_HOURS * 60.0 * rand::rng().random_range(0.3..1.0));
    // (Tests: CALL=chat|invite rings at once.)
    let forced = std::env::var("CALL").ok().filter(|_| due.is_finite()).map(|k| if k == "invite" { 2 } else { 1 });
    if now < due && forced.is_none() {
        return;
    }
    if !ringing.is_empty() {
        return;
    }
    let mut rng = rand::rng();
    if forced.is_none() {
        *next = Some(now + INTERVAL_HOURS * 60.0);
    }
    if !CALL_HOURS.contains(&clock.hour_f()) && forced.is_none() {
        return;
    }
    let kind = forced.unwrap_or_else(|| kind_of(rng.random_range(0.0..WEIGHTS.iter().sum::<f32>())));
    if kind == 0 {
        return;
    }
    let least = if kind == 2 { INVITE_LIKING } else { CHAT_LIKING };
    // (Who could call whom: the better they get on, the likelier.)
    let mut pairs: Vec<(Entity, Entity, f32)> = Vec::new();
    for (callee, sim, rels, q) in &callees {
        if !sim.age.is_grown() && sim.age != crate::sim::Age::Child || q.0.front().is_some_and(|a| a.label == "Sleep" || !a.autonomous) {
            continue;
        }
        for (caller, csim) in &callers {
            let liking = rels.friendship(caller);
            if liking >= least && !csim.age.is_little() {
                pairs.push((callee, caller, liking));
            }
        }
    }
    let total: f32 = pairs.iter().map(|p| p.2).sum();
    if total <= 0.0 {
        return;
    }
    let mut r = rng.random_range(0.0..total);
    let Some(&(callee, caller, _)) = pairs.iter().find(|p| {
        r -= p.2;
        r < 0.0
    }) else {
        return;
    };
    let Ok((_, sim, _, mut q)) = callees.get_mut(callee) else { return };
    let (cname, cid) = callers.get(caller).map(|c| (c.1.first.clone(), c.1.id)).unwrap_or_default();
    let home = (kind == 2).then(|| caller_home(&world, town.as_deref(), &story, &dormant, household.as_ref().map(|h| h.lot_index), cid)).flatten();
    info!("calls: {cname} rings {} ({})", sim.first, if kind == 2 { "to hang out" } else { "for a chat" });
    commands.entity(callee).insert(Ringing { caller, invite: kind == 2, until: now + RINGS_FOR, home, asked: false });
    // (Picked up at once, unless they're busy with something they were told to do.)
    let answer = Action::new("Answer Phone", ActionKind::PhoneChat { target: caller }, false);
    match q.0.front() {
        Some(a) if !a.autonomous => q.0.insert(1, answer),
        _ => {
            for a in q.0.iter_mut() {
                a.cancel = true;
            }
            q.0.push_back(answer);
        }
    }
    play.write(crate::sound::PlaySound::ui("phone_ring_cela_ui"));
    // (A test's one call made.)
    if forced.is_some() {
        *next = Some(f64::INFINITY);
    }
}

/// The phone picked up: the question comes up (as the call is answered); left ringing, a missed
/// call.
#[allow(clippy::type_complexity)]
fn answer_calls(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut ringing: Query<(Entity, &Sim, &mut Ringing, &mut ActionQueue)>,
    callers: Query<&Sim, Without<Ringing>>,
    mut questions: ResMut<crate::dialog::Questions>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut notes: ResMut<Notifications>,
) {
    for (e, sim, mut r, mut q) in &mut ringing {
        let answering = q.0.front().is_some_and(|a| a.label == "Answer Phone" && matches!(a.phase, Phase::Running(_)) && !a.cancel);
        let caller = callers.get(r.caller).ok();
        let cname = caller.map(|c| (c.first.clone(), c.full_name())).unwrap_or_default();
        if !r.asked && answering {
            r.asked = true;
            commands.entity(e).insert(ANSWER);
            let words = |key: &str, plain: &str| ui.as_deref().and_then(|u| u.localize(&format!("Gameplay/Objects/Electronics/Phone:{key}"))).unwrap_or_else(|| plain.to_string());
            let fill = |t: String| crate::layout::gendered(&t, caller.is_some_and(|c| c.female)).replace("{0.SimName}", &sim.full_name()).replace("{1.SimName}", &cname.1);
            let (text, yes, no) = if r.invite {
                // (The game's: "I'll Come Over" to a caller with a home, else "At My Place".)
                let yes = if r.home.is_some() { words("InvitationQueryYesOut", "I'll Come Over") } else { words("InvitationQueryYesIn", "At My Place") };
                (fill(words("InvitationQuery", "Hi {0.SimName}, this is {1.SimName} calling. Do you want to hang out?")), yes, words("InvitationQueryNo", "No Thanks"))
            } else {
                (fill(words("ChatQuery", "{1.SimName} would like to chat. Does {0.SimName} want to talk?")), words("ChatQueryYes", "Sure"), words("ChatQueryNo", "No Thanks"))
            };
            let answer = |label: String| crate::dialog::Answer { label, detail: String::new(), icon: String::new() };
            questions.ask(crate::dialog::Ask {
                about: crate::dialog::Question::PhoneCall { sim: e, caller: r.caller, invite: r.invite, home: r.home },
                icon: String::new(),
                heading: String::new(),
                title: String::new(),
                text,
                answers: vec![answer(yes), answer(no)],
            });
            continue;
        }
        // (Not picked up in time, or put down: missed.)
        if !r.asked && (clock.minutes > r.until || !q.0.iter().any(|a| a.label == "Answer Phone")) {
            for a in q.0.iter_mut().filter(|a| a.label == "Answer Phone") {
                a.cancel = true;
            }
            notes.push(format!("{} missed a call from {}.", sim.first, cname.0));
            commands.entity(e).remove::<Ringing>();
        }
    }
}

/// The answer: a chat goes on (or the call's over); asked to hang out, the caller comes over,
/// or the one called goes to theirs.
#[allow(clippy::type_complexity)]
fn call_answers(
    mut commands: Commands,
    clock: Res<GameClock>,
    mut answers: MessageReader<crate::dialog::Answered>,
    mut sims: Query<(&Sim, &mut ActionQueue)>,
    names: Query<&Sim>,
    mut notes: ResMut<Notifications>,
) {
    for a in answers.read() {
        let crate::dialog::Question::PhoneCall { sim, caller, invite, home } = a.about else { continue };
        commands.entity(sim).remove::<Ringing>();
        let cname = names.get(caller).map(|c| c.first.clone()).unwrap_or_default();
        let Ok((me, mut q)) = sims.get_mut(sim) else { continue };
        let first = me.first.clone();
        let call = q.0.iter_mut().find(|x| x.label == "Answer Phone");
        match (a.answer, invite) {
            // (A chat: on with it.)
            (0, false) => {
                if let Some(c) = call {
                    c.label = format!("Chat with {cname}");
                }
            }
            (0, true) => {
                if let Some(c) = call {
                    c.cancel = true;
                }
                // (Off to theirs, with a home of their own in town; else they come over.)
                match home {
                    Some(lot) => {
                        if let Ok((_, mut q)) = sims.get_mut(sim) {
                            q.push_player(Action::new("Visit Lot", ActionKind::GoToLot { lot }, false));
                        }
                        notes.push(format!("{first} is off to hang out with {cname}."));
                    }
                    None => {
                        // (Someone strolling about town comes as a guest.)
                        commands.entity(caller).remove::<(crate::town::Townie, crate::nav::PathFollow)>().insert((OffLot, Invited { arrive_at: clock.minutes + COMING_OVER }));
                        notes.push(format!("{cname} is coming over to hang out with {first}."));
                    }
                }
            }
            _ => {
                if let Some(c) = call {
                    c.cancel = true;
                }
            }
        }
    }
}

/// `CALL=chat|invite` (a test): the selected Sim gets to know three of the town well, someone to
/// ring them (at once).
fn call_test_friends(
    mut done: Local<bool>,
    time: Res<Time>,
    mut start: Local<Option<f32>>,
    sel: Query<Entity, With<crate::sim::Selected>>,
    town: Query<(Entity, &Sim), (Or<(With<OffLot>, With<crate::town::Townie>)>, Without<HouseholdMember>)>,
    mut rels: Query<&mut Relationships>,
) {
    let t0 = *start.get_or_insert(time.elapsed_secs());
    if *done || time.elapsed_secs() - t0 < 4.0 {
        return;
    }
    let Ok(me) = sel.single() else { return };
    *done = true;
    for (e, _) in town.iter().filter(|t| t.1.age.is_grown() && t.1.age != crate::sim::Age::Child).take(3) {
        for (a, b) in [(me, e), (e, me)] {
            if let Ok(mut r) = rels.get_mut(a) {
                r.entry(b).friendship = 50.0;
            }
        }
    }
}

/// The lot a Sim lives on in town (their family's home), if any.
fn caller_home(
    world: &crate::loading::CurrentWorld,
    town: Option<&crate::premade::TownPremades>,
    story: &crate::story::TownStory,
    dormant: &crate::household::Dormant,
    active: Option<usize>,
    id: u64,
) -> Option<usize> {
    (0..world.data.lots.len()).find_map(|lot| {
        let f = crate::homes::family_at(world, town, story, dormant, active, lot)?;
        // (Named in messages by the family.)
        f.members.iter().any(|m| m.id == id).then(|| {
            crate::homes::name_home(lot, &f.name);
            lot
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_come_in_the_games_proportions() {
        assert_eq!(kind_of(0.0), 0);
        assert_eq!(kind_of(44.9), 0);
        assert_eq!(kind_of(45.0), 1);
        assert_eq!(kind_of(86.9), 1);
        assert_eq!(kind_of(87.0), 2);
        assert_eq!(kind_of(94.9), 2);
    }
}
