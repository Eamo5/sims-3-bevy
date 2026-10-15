//! Jealousy and betrayal, as the game's (`SocialComponent.CheckCheating`, `OnIWasCheatedOn`,
//! the `Betrayed` buff). Romance in front of others (each social's `JealousyLevel`: a flirt
//! low, a kiss medium, WooHoo high) is seen by those within ten metres:
//!
//! - someone romantically involved with either of the two, as the one taken: a flirt earns a
//!   jealous look and their liking (10, and an insult to their conversation); a kiss or more,
//!   they go and have it out with them (*Accuse of Cheating*), and then the tantrum, liking
//!   down 60 and the Betrayed moodlet. Caught again while they still feel betrayed, it's over;
//! - family or a friend of the cheater's partner: the same reaction and liking lost, and for a
//!   kiss or more, the Witnessed Betrayal moodlet.
//!
//! The betrayed give their cheater time to make it up to them (five days for a spouse or
//! fiancé(e), three for a partner, two for a romantic interest): liking up 25 by then, they
//! stay together; else they break it off. Owning up (*Confess Cheating*) is a medium betrayal.

use bevy::prelude::*;

use crate::PlayMode;
use crate::clock::GameClock;
use crate::conversation::Commodity;
use crate::life::{LifeEvent, LifeEventKind, MoodletKind, Moodlets};
use crate::sim::{HouseholdMember, Sim};
use crate::social::{FRIEND_THRESHOLD, RelStatus, Relationship, Relationships};
use crate::visit::OnLot;

pub struct JealousyPlugin;

impl Plugin for JealousyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Romanced>().add_message::<CheatedOn>().add_systems(Update, (witness, cheated_on, made_up).chain().run_if(in_state(PlayMode::Live)));
        if std::env::var("CHEAT").is_ok() {
            app.add_systems(Update, cheat_test.run_if(in_state(PlayMode::Live)));
        }
    }
}

/// A social that can make a partner jealous, done (the game's `ReactToSocialCallback`): how
/// much (1 low to 3 high), and whether it was taken well (a rebuffed one's only the actor's).
#[derive(Message, Clone, Copy, Debug)]
pub struct Romanced {
    pub actor: Entity,
    pub target: Entity,
    pub level: u8,
    pub accepted: bool,
}

/// A Sim finding out they've been cheated on (the game's `OnIWasCheatedOn`): by whom, with
/// whom if it's known, and how badly (1 low to 3 high).
#[derive(Message, Clone, Copy, Debug)]
pub struct CheatedOn {
    pub victim: Entity,
    pub naughty: Entity,
    pub other: Option<Entity>,
    pub level: u8,
}

/// How far off it's seen (the game's `kReactToSocialParams`: a 10 m pulse, through walls).
const SEE_RANGE: f32 = 10.0;
/// What being cheated on does, by level, low to high (the game's
/// `PlayReactionAndUpdateRelationshipOnJealousy`): the reaction, the liking lost and the insult
/// to their conversation.
const REACTIONS: [&str; 3] = ["JealousMild", "JealousMiddle", "JealousIntense"];
const LIKING: [f32; 3] = [-10.0, -30.0, -60.0];
const INSULT: [f32; 3] = [50.0, 150.0, 300.0];
/// How long the betrayed give their cheater to make it up (the game's
/// `kBetrayedBuffTimeOutPerLtr`, hours): a spouse or fiancé(e), a partner, a romantic interest.
const BETRAYED_HOURS: [f64; 3] = [120.0, 72.0, 48.0];
/// How much better they must come to like them by then (`kLtrLikingDeltaToFixBetrayedRelationship`).
const MAKE_UP_LIKING: f32 = 25.0;
/// How long a reaction waits for its Sim to be free (game minutes).
const REACT_WITHIN: f64 = 30.0;

/// What someone who sees a romantic social makes of it, as one of the two's doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seen {
    Nothing,
    /// Their romance flirting with another: a jealous look, and liking lost.
    Jealous,
    /// Their romance kissing another, or more: they have it out with them.
    Accuse,
    /// ...when they already feel betrayed by them: it's over.
    BreakUp,
    /// The cheater's partner's family or friend: upset for them.
    Witness,
}

/// What `rel` (the observer's with the one doing it; none: strangers) makes of a social of
/// `level` with `with`, the one doing it having `partner`, whom the observer `cares` for (the
/// game's `CheckCheating`).
pub fn judge(rel: Option<&Relationship>, level: u8, partner: Option<Entity>, with: Entity, cares: bool) -> Seen {
    let Some(rel) = rel.filter(|_| level > 0) else { return Seen::Nothing };
    if rel.is_romance() {
        return match (level, rel.betrayed) {
            (1, _) => Seen::Jealous,
            (_, Some(_)) => Seen::BreakUp,
            _ => Seen::Accuse,
        };
    }
    if cares && partner.is_some_and(|p| p != with) { Seen::Witness } else { Seen::Nothing }
}

/// How long a betrayal's to be made up in (game minutes), by what they were to each other.
pub fn betrayed_minutes(status: RelStatus) -> f64 {
    60.0 * match status {
        RelStatus::Married | RelStatus::Engaged => BETRAYED_HOURS[0],
        RelStatus::Partner => BETRAYED_HOURS[1],
        _ => BETRAYED_HOURS[2],
    }
}

/// Whether a betrayal was made up: liking up enough since.
pub fn made_up_for(rel: &Relationship, liking_then: f32) -> bool {
    rel.friendship - liking_then >= MAKE_UP_LIKING
}

type SimQuery<'a> = (Entity, &'a Sim, &'a Transform, Option<&'a OnLot>, &'a InheritedVisibility, &'a mut Relationships, &'a mut Moodlets, &'a mut crate::interact::ActionQueue, Has<HouseholdMember>);

/// Liking lost between two Sims, both ways, and their conversation soured.
fn sour(sims: &mut Query<SimQuery>, a: Entity, b: Entity, level: u8) {
    let i = (level.clamp(1, 3) - 1) as usize;
    let Ok(mut q) = sims.get_mut(a) else { return };
    let r = q.5.entry(b);
    r.friendship = (r.friendship + LIKING[i]).clamp(-100.0, 100.0);
    r.stc.update(Commodity::Insulting, INSULT[i]);
    let (liking, stc) = (r.friendship, r.stc);
    if let Ok(mut q) = sims.get_mut(b) {
        let r = q.5.entry(a);
        r.friendship = liking;
        r.stc = stc;
    }
}

/// A reaction toward someone (by kind of the game's `Reactions`).
fn react(commands: &mut Commands, sims: &Query<SimQuery>, who: Entity, at: Entity, kind: &'static str, now: f64) {
    let toward = sims.get(at).ok().map(|q| q.2.translation);
    commands.entity(who).insert(crate::reactions::Pending::new(&[kind], toward, now + REACT_WITHIN));
}

/// The game's words for a betrayal's end (`Gameplay/Reputation:...`), the cheater and the one
/// betrayed in them.
fn betrayal_words(ui: Option<&crate::layout::UiAssets>, key: &str, naughty: &Sim, victim: &Sim, fallback: String) -> String {
    ui.and_then(|u| u.localize(&format!("Gameplay/Reputation:{key}")))
        .map(|t| crate::layout::gendered(&t, victim.female).replace("{0.SimName}", &naughty.full_name()).replace("{1.SimName}", &victim.full_name()))
        .unwrap_or(fallback)
}

/// The betrayed breaking it off with their cheater (the game's `BreakUpFromBetrayal`): going
/// steady, engaged or married, they're exes; and romantic no more.
fn break_off(sims: &mut Query<SimQuery>, victim: Entity, naughty: Entity, life: &mut MessageWriter<LifeEvent>) {
    let mut were = false;
    for (x, y) in [(victim, naughty), (naughty, victim)] {
        if let Ok(mut q) = sims.get_mut(x) {
            let r = q.5.entry(y);
            if matches!(r.status, RelStatus::Partner | RelStatus::Engaged | RelStatus::Married) {
                r.status = RelStatus::Ex;
                r.romance = (r.romance - 60.0).max(-100.0);
                were = true;
            }
            r.romantic = false;
            r.betrayed = None;
        }
    }
    if were {
        life.write(LifeEvent::new(victim, LifeEventKind::BrokeUp));
        life.write(LifeEvent::new(naughty, LifeEventKind::BrokeUp));
    }
    // (The moodlet goes with the last of their betrayals.)
    if let Ok(mut q) = sims.get_mut(victim)
        && !q.5.0.values().any(|r| r.betrayed.is_some())
    {
        q.6.remove(MoodletKind::Betrayed);
    }
}

/// Whether either of the two is the household's (their story's told).
fn ours(sims: &Query<SimQuery>, a: Entity, b: Entity) -> bool {
    [a, b].iter().any(|e| sims.get(*e).is_ok_and(|q| q.8))
}

/// Those about a romantic social, as it's done: each sees it as one of the two's doing, then
/// (taken well) the other's (the game's `ReactToSocialCallback`).
#[allow(clippy::too_many_arguments)]
fn witness(
    mut commands: Commands,
    clock: Res<GameClock>,
    family: Res<crate::family::Genealogy>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut seen: MessageReader<Romanced>,
    mut cheated: MessageWriter<CheatedOn>,
    mut life: MessageWriter<LifeEvent>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut sims: Query<SimQuery>,
) {
    for ev in seen.read() {
        let Ok(at) = sims.get(ev.actor).map(|q| (q.2.translation, q.3.map(|l| l.0))) else { continue };
        // (What each of the two is doing, as the observers see it.)
        let mut plans: Vec<(Entity, Seen, Entity, Entity)> = Vec::new();
        for (o, osim, tf, lot, shown, rels, ..) in sims.iter() {
            if o == ev.actor || o == ev.target || !shown.get() || osim.age.is_little() || lot.map(|l| l.0) != at.1 || tf.translation.distance(at.0) > SEE_RANGE {
                continue;
            }
            let look = |doer: Entity, with: Entity| -> Seen {
                let partner = sims.get(doer).ok().and_then(|q| q.5.partner()).map(|p| p.0);
                let cares = partner.and_then(|p| sims.get(p).ok()).is_some_and(|p| family.kin(osim.id, p.1.id).is_some() || rels.friendship(p.0) >= FRIEND_THRESHOLD);
                judge(rels.0.get(&doer), ev.level, partner, with, cares)
            };
            let mut s = (look(ev.actor, ev.target), ev.actor, ev.target);
            if s.0 == Seen::Nothing && ev.accepted {
                s = (look(ev.target, ev.actor), ev.target, ev.actor);
            }
            if s.0 != Seen::Nothing {
                plans.push((o, s.0, s.1, s.2));
            }
        }
        for (o, s, naughty, with) in plans {
            let i = (ev.level.clamp(1, 3) - 1) as usize;
            let name = |e: Entity| sims.get(e).map(|q| q.1.first.clone()).unwrap_or_default();
            let names = (name(o), name(naughty), name(with));
            debug!("jealousy: {} sees {} with {}: {s:?}", names.0, names.1, names.2);
            match s {
                Seen::Nothing => {}
                Seen::Jealous => {
                    react(&mut commands, &sims, o, naughty, REACTIONS[0], clock.minutes);
                    sour(&mut sims, o, naughty, 1);
                    sour(&mut sims, o, with, 1);
                }
                Seen::Accuse => {
                    // (They go and have it out with them; too young to, it's simply found out.)
                    let grown = sims.get(o).is_ok_and(|q| !matches!(q.1.age, crate::sim::Age::Child | crate::sim::Age::Teen));
                    let accuse = crate::social::all_socials().find(|(_, d)| d.effect == crate::social::SocialEffect::Accuse).map(|(i, _)| i);
                    match accuse.filter(|_| grown) {
                        Some(si) => {
                            if let Ok(mut q) = sims.get_mut(o) {
                                q.7.0.retain(|a| !matches!(a.kind, crate::interact::ActionKind::Social { social, .. } if social == si));
                                q.7.push_player(crate::interact::Action::new(crate::social::social(si).name, crate::interact::ActionKind::Social { target: naughty, social: si }, false));
                            }
                        }
                        None => {
                            cheated.write(CheatedOn { victim: o, naughty, other: Some(with), level: ev.level });
                        }
                    }
                    if ours(&sims, o, naughty) {
                        notes.push(format!("{} caught {} cheating with {}!", names.0, names.1, names.2));
                    }
                }
                Seen::BreakUp => {
                    react(&mut commands, &sims, o, naughty, REACTIONS[2], clock.minutes);
                    if ours(&sims, o, naughty)
                        && let (Ok(n), Ok(v)) = (sims.get(naughty), sims.get(o))
                    {
                        let fallback = format!("{} caught {} cheating again and has broken it off.", v.1.first, n.1.first);
                        notes.push(betrayal_words(ui.as_deref(), "BetrayalFail", n.1, v.1, fallback));
                    }
                    break_off(&mut sims, o, naughty, &mut life);
                }
                Seen::Witness => {
                    if sims.get(o).is_ok_and(|q| q.6.has(MoodletKind::WitnessedBetrayal)) {
                        continue;
                    }
                    react(&mut commands, &sims, o, naughty, REACTIONS[i], clock.minutes);
                    sour(&mut sims, o, naughty, ev.level);
                    sour(&mut sims, o, with, ev.level);
                    if ev.level >= 2
                        && let Ok(mut q) = sims.get_mut(o)
                    {
                        q.6.add(MoodletKind::WitnessedBetrayal, clock.minutes);
                    }
                }
            }
        }
    }
}

/// Finding out (having it out with them, or their owning up): the reaction, liking lost with
/// the cheater and whoever it was with, and for a kiss or more, betrayed: given time to make it
/// up, unless they already were.
#[allow(clippy::too_many_arguments)]
fn cheated_on(
    mut commands: Commands,
    clock: Res<GameClock>,
    ui: Option<Res<crate::layout::UiAssets>>,
    mut found: MessageReader<CheatedOn>,
    mut life: MessageWriter<LifeEvent>,
    mut notes: ResMut<crate::interact::Notifications>,
    mut sims: Query<SimQuery>,
) {
    for ev in found.read() {
        let i = (ev.level.clamp(1, 3) - 1) as usize;
        let Ok(before) = sims.get(ev.victim).map(|q| q.5.get(ev.naughty)) else { continue };
        react(&mut commands, &sims, ev.victim, ev.naughty, REACTIONS[i], clock.minutes);
        sour(&mut sims, ev.victim, ev.naughty, ev.level);
        if let Some(other) = ev.other {
            sour(&mut sims, ev.victim, other, ev.level);
        }
        if ev.level < 2 {
            continue;
        }
        if before.betrayed.is_some() {
            if ours(&sims, ev.victim, ev.naughty)
                && let (Ok(n), Ok(v)) = (sims.get(ev.naughty), sims.get(ev.victim))
            {
                let fallback = format!("{} has broken it off with {}.", v.1.first, n.1.first);
                notes.push(betrayal_words(ui.as_deref(), "BetrayalFail", n.1, v.1, fallback));
            }
            break_off(&mut sims, ev.victim, ev.naughty, &mut life);
            continue;
        }
        let until = clock.minutes + betrayed_minutes(before.status);
        if let Ok(mut q) = sims.get_mut(ev.victim) {
            let r = q.5.entry(ev.naughty);
            r.betrayed = Some((until, r.friendship));
            let m = &mut q.6;
            m.add(MoodletKind::Betrayed, clock.minutes);
            if let Some(b) = m.0.iter_mut().find(|m| m.kind == MoodletKind::Betrayed) {
                b.until = b.until.max(until);
            }
            debug!("jealousy: {} feels betrayed until {until:.0}", q.1.first);
        }
    }
}

/// A betrayal's time up: made up for (liking up enough since), they stay together; else they
/// break it off (the game's `BuffInstanceBetrayed.OnTimeOutUpdated`).
fn made_up(clock: Res<GameClock>, ui: Option<Res<crate::layout::UiAssets>>, mut life: MessageWriter<LifeEvent>, mut notes: ResMut<crate::interact::Notifications>, mut sims: Query<SimQuery>) {
    let now = clock.minutes;
    let due: Vec<(Entity, Entity, bool)> = sims
        .iter()
        .flat_map(|q| {
            let victim = q.0;
            q.5.0.iter().filter_map(move |(n, r)| r.betrayed.filter(|b| now >= b.0).map(|b| (victim, *n, made_up_for(r, b.1)))).collect::<Vec<_>>()
        })
        .collect();
    for (victim, naughty, saved) in due {
        let told = ours(&sims, victim, naughty);
        let words = match (sims.get(naughty), sims.get(victim)) {
            (Ok(n), Ok(v)) if saved => betrayal_words(ui.as_deref(), "BetrayalSaved", n.1, v.1, format!("{} has convinced {} the relationship will work out.", n.1.first, v.1.first)),
            (Ok(n), Ok(v)) => betrayal_words(ui.as_deref(), "BetrayalFail", n.1, v.1, format!("{} has broken it off with {}.", v.1.first, n.1.first)),
            _ => String::new(),
        };
        if told && !words.is_empty() {
            notes.push(words);
        }
        if saved {
            if let Ok(mut q) = sims.get_mut(victim) {
                q.5.entry(naughty).betrayed = None;
                if !q.5.0.values().any(|r| r.betrayed.is_some()) {
                    q.6.remove(MoodletKind::Betrayed);
                }
            }
        } else {
            break_off(&mut sims, victim, naughty, &mut life);
        }
    }
}

/// `CHEAT=kiss` (a test): the selected Sim, married to another of the household, kisses a
/// neighbour who's dropped by, in front of their spouse, who should have it out with them and
/// feel betrayed. `again`: the spouse already feels betrayed, and breaks it off; `timeout`: the
/// spouse feels betrayed, the time to make it up running out, and breaks it off.
#[allow(clippy::type_complexity)]
fn cheat_test(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    mut state: Local<(Option<f32>, f32, Option<(Entity, Entity, Entity)>, bool)>,
    sel: Query<Entity, With<crate::sim::Selected>>,
    members: Query<(Entity, &Sim), With<HouseholdMember>>,
    visitors: Query<Entity, With<crate::interact::Visitor>>,
    mut sims: Query<(&mut Transform, &mut Relationships, &mut crate::interact::ActionQueue, &Moodlets)>,
) {
    let mode = std::env::var("CHEAT").unwrap_or_default();
    let now = time.elapsed_secs();
    let t0 = *state.0.get_or_insert(now);
    if now - t0 < 5.0 || state.3 {
        return;
    }
    let Some((me, spouse, lover)) = state.2 else {
        let Ok(me) = sel.single() else { return };
        let spouse = members.iter().find(|(e, s)| *e != me && s.age.is_grown() && s.age != crate::sim::Age::Child).map(|(e, _)| e);
        let (Some(spouse), Some(lover)) = (spouse, visitors.iter().next()) else { return };
        let pairs = [(me, spouse, 70.0, 80.0, Some(RelStatus::Married)), (me, lover, 60.0, 40.0, None)];
        for (a, b, f, r, status) in pairs {
            for (x, y) in [(a, b), (b, a)] {
                if let Ok(mut q) = sims.get_mut(x) {
                    let rel = q.1.entry(y);
                    rel.friendship = f;
                    rel.romance = r;
                    rel.romantic = true;
                    if let Some(s) = status {
                        rel.status = s;
                    }
                }
            }
        }
        if let Ok(mut q) = sims.get_mut(spouse) {
            match mode.as_str() {
                "again" => q.1.entry(me).betrayed = Some((clock.minutes + 3000.0, 70.0)),
                "timeout" => q.1.entry(me).betrayed = Some((clock.minutes + 20.0, 70.0)),
                _ => {}
            }
        }
        // (The three together: the neighbour in front, the spouse a few steps off.)
        let Ok(at) = sims.get(me).map(|q| *q.0) else { return };
        for (e, off) in [(lover, at.forward() * 1.3), (spouse, at.right() * 3.5)] {
            if let Ok(mut q) = sims.get_mut(e) {
                q.0.translation = at.translation + off;
                q.2.0.clear();
            }
            commands.entity(e).remove::<crate::doorbell::AtTheDoor>();
        }
        if mode != "timeout"
            && let (Ok(mut q), Some(si)) = (sims.get_mut(me), crate::social::social_index("Kiss"))
        {
            q.2.0.clear();
            q.2.push_player(crate::interact::Action::new("Kiss", crate::interact::ActionKind::Social { target: lover, social: si }, false));
        }
        info!("cheat test ({mode}): {me:?} kisses {lover:?} before {spouse:?}");
        state.2 = Some((me, spouse, lover));
        state.1 = now;
        return;
    };
    if now - state.1 < 3.0 {
        return;
    }
    state.1 = now;
    let Ok(q) = sims.get(spouse) else { return };
    let rel = q.1.get(me);
    let doing: Vec<&str> = q.2.0.iter().map(|a| a.label.as_str()).collect();
    info!("cheat test: spouse {:?} liking {:.0} betrayed {:?} moodlet {} doing {doing:?}", rel.status, rel.friendship, rel.betrayed, q.3.has(MoodletKind::Betrayed));
    let _ = lover;
    let pass = match mode.as_str() {
        "again" | "timeout" => rel.status == RelStatus::Ex && !rel.romantic,
        _ => rel.betrayed.is_some() && q.3.has(MoodletKind::Betrayed),
    };
    if pass {
        info!("cheat test {mode} PASS");
        state.3 = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_romance_seen_with_another_is_taken_as_the_game_takes_it() {
        let other = Entity::from_raw_u32(7).unwrap();
        let spouse = Relationship { status: RelStatus::Married, friendship: 80.0, ..Default::default() };
        // (A flirt: a jealous look; a kiss: have it out; again while betrayed: it's over.)
        assert_eq!(judge(Some(&spouse), 1, None, other, false), Seen::Jealous);
        assert_eq!(judge(Some(&spouse), 2, None, other, false), Seen::Accuse);
        let betrayed = Relationship { betrayed: Some((100.0, 20.0)), ..spouse };
        assert_eq!(judge(Some(&betrayed), 3, None, other, false), Seen::BreakUp);
        // (An ex, a stranger, a friend: nothing to them, unless it's their friend's partner.)
        let ex = Relationship { status: RelStatus::Ex, romantic: true, ..Default::default() };
        assert_eq!(judge(Some(&ex), 3, None, other, false), Seen::Nothing);
        assert_eq!(judge(None, 3, None, other, true), Seen::Nothing);
        let friend = Relationship { friendship: 50.0, ..Default::default() };
        let partner = Entity::from_raw_u32(9).unwrap();
        assert_eq!(judge(Some(&friend), 2, Some(partner), other, true), Seen::Witness);
        assert_eq!(judge(Some(&friend), 2, Some(partner), other, false), Seen::Nothing);
        // (With their own partner: nothing wrong.)
        assert_eq!(judge(Some(&friend), 2, Some(partner), partner, true), Seen::Nothing);
        // (Time to make it up: five days married, three steady, two otherwise.)
        assert_eq!(betrayed_minutes(RelStatus::Married), 7200.0);
        assert_eq!(betrayed_minutes(RelStatus::Partner), 4320.0);
        assert_eq!(betrayed_minutes(RelStatus::None), 2880.0);
        assert!(made_up_for(&Relationship { friendship: 30.0, ..Default::default() }, 0.0));
        assert!(!made_up_for(&Relationship { friendship: 20.0, ..Default::default() }, 0.0));
    }
}
