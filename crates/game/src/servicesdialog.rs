//! The game's "Telephone Services" dialog (UI.dll's `ServicesController` over its `Service`
//! layout), put up when a Sim on their home lot calls for "Services..." (`Phone.CallForServices`):
//! the base game's services in a table, each with its price (the game's `ServicesModel`:
//! "Free!" for those that cost nothing, "§125/day" for the recurring maid), sorted by price, the
//! household's funds under it. A row picked and "Call": the service requested, in the service's
//! own words (`Gameplay/Services/Requested`); a recurring one that's on, cancelled instead ("Maid -
//! Cancel Services"). One already on its way can't be called again, nor one the household can't
//! afford. The game stands still while it's up.

use bevy::prelude::*;

use crate::layout::{Spawned, UiAssets, UiButton};
use crate::{AppState, PlayMode};

pub struct ServicesDialogPlugin;

impl Plugin for ServicesDialogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServicesDialog>()
            .add_systems(Update, (services_test, services_dialog, crate::hud::adopt_choice).chain().run_if(in_state(PlayMode::Live)))
            .add_systems(OnExit(AppState::InGame), |mut d: ResMut<ServicesDialog>| *d = ServicesDialog::default());
    }
}

// The `Service` layout's windows (`ServicesController.ControlID`).
const FUNDS: u32 = 0x06d4_dc00;
const TABLE: u32 = 0x06d4_dc01;
const CALL: u32 = 0x06d4_dc02;
const CANCEL: u32 = 0x06d4_dc03;
/// The table's columns (name, price) and rows.
const COLUMNS: [f32; 2] = [180.0, 92.0];
const ROW: f32 = 26.0;
const HEADER: f32 = 26.0;
const ROWS: usize = 8;
/// `ServiceRowController`'s colours: a row's words, and an active service's.
const ROW_TEXT: u32 = 0xff22_2b4d;
const ACTIVE_TEXT: u32 = 0xffdc_6449;
const PICKED: u32 = 0x4060_a0ff;
const ALTERNATE: u32 = 0x18ff_ffff;

/// The base game's services from the phone (`ServicesModel.GetServices`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Babysitter,
    Maid,
    PizzaDelivery,
    Repairman,
    SocialWorkerAdoption,
    Police,
    Firefighter,
    NewspaperDelivery,
}

impl Service {
    pub const ALL: [Service; 8] = [
        Service::Babysitter,
        Service::Maid,
        Service::PizzaDelivery,
        Service::Repairman,
        Service::SocialWorkerAdoption,
        Service::Police,
        Service::Firefighter,
        Service::NewspaperDelivery,
    ];

    /// Its name in the game's keys (`ServiceType`).
    fn key(self) -> &'static str {
        match self {
            Service::Babysitter => "Babysitter",
            Service::Maid => "Maid",
            Service::PizzaDelivery => "PizzaDelivery",
            Service::Repairman => "Repairman",
            Service::SocialWorkerAdoption => "SocialWorkerAdoption",
            Service::Police => "Police",
            Service::Firefighter => "Firefighter",
            Service::NewspaperDelivery => "NewspaperDelivery",
        }
    }

    fn fallback_name(self) -> &'static str {
        match self {
            Service::Babysitter => "Babysitter",
            Service::Maid => "Maid",
            Service::PizzaDelivery => "Pizza Delivery",
            Service::Repairman => "Repairman",
            Service::SocialWorkerAdoption => "Adoption Service",
            Service::Police => "Police",
            Service::Firefighter => "Fire Department",
            Service::NewspaperDelivery => "Newspaper Delivery",
        }
    }

    /// What it costs, as the dialog shows it (each service's tuned `kCost`; the emergency
    /// services' only for a false call, so free here).
    pub fn price(self) -> i64 {
        match self {
            Service::Babysitter => crate::services::SITTER_COST,
            Service::Maid => crate::services::MAID_COST,
            Service::PizzaDelivery => crate::meals::PIZZA_PRICE,
            Service::Repairman => crate::interact::REPAIRMAN_PRICE,
            _ => 0,
        }
    }

    /// A recurring one (the maid, the paper) is called to start and to stop.
    pub fn recurring(self) -> bool {
        matches!(self, Service::Maid | Service::NewspaperDelivery)
    }
}

/// A service as the table has it: on now or not, and whether it can be called at all.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub service: Service,
    pub active: bool,
}

/// The services the phone offers, sorted as the table is (by price; `Sort(1)`), and which are on
/// now: the babysitter only to a household with children.
pub fn rows(children: bool, active: impl Fn(Service) -> bool) -> Vec<Row> {
    let mut out: Vec<Row> = Service::ALL.into_iter().filter(|s| *s != Service::Babysitter || children).map(|s| Row { service: s, active: active(s) }).collect();
    out.sort_by_key(|r| r.service.price());
    out
}

/// Whether "Call" can be pressed for a row, or why not (the game's tooltips).
pub fn callable(r: &Row, funds: i64) -> Result<(), &'static str> {
    if r.active {
        return if r.service.recurring() { Ok(()) } else { Err("AlreadyActive") };
    }
    if r.service.price() > funds {
        return Err("InsufficientFunds");
    }
    Ok(())
}

/// Who's calling, and the dialog: what's picked.
#[derive(Resource, Default)]
pub struct ServicesDialog {
    pub sim: Option<Entity>,
    s: Option<Spawned>,
    picked: Option<usize>,
    rows: Vec<Entity>,
    shown: Option<(Vec<Row>, Option<usize>, i64)>,
    /// (Tests: "Call" pressed.)
    test_call: bool,
}

impl ServicesDialog {
    /// Up for a Sim (unless it's up already).
    pub fn open(&mut self, sim: Entity) {
        if self.sim.is_none() {
            *self = ServicesDialog { sim: Some(sim), ..default() };
        }
    }
}

/// A row's button.
#[derive(Component, Clone, Copy)]
struct ServiceRow(usize);

/// SERVICES=<service key>[:call] (tests): the dialog up for the selected Sim ten seconds in, that
/// row picked, and with `:call`, called two seconds on.
fn services_test(mut d: ResMut<ServicesDialog>, time: Res<Time>, sel: Query<Entity, With<crate::sim::Selected>>, mut done: Local<u8>, mut pending_call: Local<Option<Service>>) {
    let Ok(v) = std::env::var("SERVICES") else { return };
    let t = time.elapsed_secs();
    let Ok(e) = sel.single() else { return };
    let (key, call) = v.split_once(':').map_or((v.as_str(), false), |(k, c)| (k, c == "call"));
    match *done {
        0 if t >= 10.0 => {
            *done = 1;
            info!("services test: the services dialog up");
            d.open(e);
            *pending_call = Service::ALL.into_iter().find(|s| s.key().eq_ignore_ascii_case(key));
        }
        1 if t >= 11.0 => {
            *done = 2;
            if let (Some(s), Some((rows, _, _))) = (*pending_call, d.shown.as_ref()) {
                d.picked = rows.iter().position(|r| r.service == s);
                info!("services test: picked {s:?}");
            }
        }
        2 if t >= 13.0 && call => {
            *done = 3;
            d.test_call = true;
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn services_dialog(
    mut commands: Commands,
    mut d: ResMut<ServicesDialog>,
    ui: Option<ResMut<UiAssets>>,
    (mut images, mut fonts): (ResMut<Assets<Image>>, ResMut<Assets<Font>>),
    (household, maid, sitter, pizza, repair, paper): (
        Option<Res<crate::interact::Household>>,
        Res<crate::services::MaidService>,
        Res<crate::services::Sitter>,
        Option<Res<crate::meals::PizzaOrder>>,
        Option<Res<crate::interact::RepairmanVisit>>,
        Res<crate::mail::PaperRound>,
    ),
    (breakin, dept): (Res<crate::burglar::Break>, Res<crate::fire::FireDepartment>),
    sims: Query<&crate::sim::Sim>,
    members: Query<&crate::sim::Sim, With<crate::sim::HouseholdMember>>,
    (clicks, row_clicks, mut buttons): (Query<(Entity, &Interaction), Changed<Interaction>>, Query<(&Interaction, &ServiceRow), Changed<Interaction>>, Query<&mut UiButton>),
    mut play: MessageWriter<crate::sound::PlaySound>,
) {
    let Some(mut ui) = ui else { return };
    let close = |commands: &mut Commands, d: &mut ServicesDialog| {
        if let Some(r) = d.s.take().and_then(|s| s.root) {
            commands.entity(r).try_despawn();
        }
        *d = ServicesDialog::default();
    };
    let Some(me) = d.sim else { return };
    let Ok(caller) = sims.get(me) else {
        close(&mut commands, &mut d);
        return;
    };
    let loc = |ui: &UiAssets, k: &str, or: &str| ui.localize(k).unwrap_or_else(|| or.to_string());
    if d.s.is_none() {
        let Some(s) = crate::gamedialogs::spawn_dialog(&mut commands, &mut ui, &mut images, &mut fonts, "Service") else {
            warn!("no Service layout");
            d.sim = None;
            return;
        };
        // The table's headers: the game's column buttons.
        if let (Some(col), Some(t)) = (ui.layout("ColumnControl").cloned(), s.id(TABLE)) {
            commands.entity(t).despawn_children();
            let mut x = 0.0;
            for (k, (key, or)) in [("Ui/Caption/Services/Column:Name", "Name"), ("Ui/Caption/Services/Column:Price", "Price")].iter().enumerate() {
                let mut w = col.clone();
                w.place = s3bake::ui::UiPlace::Fixed;
                w.area = [x, 0.0, x + COLUMNS[k], HEADER];
                w.caption = loc(&ui, key, or);
                ui.spawn_under(&mut commands, &mut images, &mut fonts, &w, t);
                x += COLUMNS[k];
            }
        }
        if let Some(e) = s.id(CANCEL) {
            commands.entity(e).insert(crate::icons::Tooltip(loc(&ui, "Ui/Caption/Services/Order:Cancel", "Cancel")));
        }
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_open"));
        d.s = Some(s);
    }
    let Some(s) = d.s.clone() else { return };
    let funds = household.as_ref().map_or(0, |h| h.funds);
    let children = members.iter().any(|m| m.age.is_little() || m.age == crate::sim::Age::Child);
    let active = |sv: Service| match sv {
        Service::Babysitter => sitter.here(),
        Service::Maid => maid.hired,
        Service::PizzaDelivery => pizza.is_some(),
        Service::Repairman => repair.is_some(),
        Service::SocialWorkerAdoption => false,
        Service::Police => breakin.called.is_some(),
        Service::Firefighter => dept.called.is_some(),
        Service::NewspaperDelivery => !paper.cancelled,
    };
    let list = rows(children, active);
    if crate::livehud::pressed(&clicks, s.id(CANCEL)) {
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    if let Some((_, r)) = row_clicks.iter().find(|(i, _)| **i == Interaction::Pressed) {
        d.picked = Some(r.0);
        play.write(crate::sound::PlaySound::ui("ui_tertiary_button"));
    }
    let picked = d.picked.and_then(|i| list.get(i)).cloned();
    let can = picked.as_ref().map(|r| callable(r, funds));
    let test_call = std::mem::take(&mut d.test_call);
    if (crate::livehud::pressed(&clicks, s.id(CALL)) || test_call)
        && let Some(r) = &picked
        && can == Some(Ok(()))
    {
        let on = !r.active;
        let say = |k: &str| ui.localize(&format!("Gameplay/Services/{k}:{}", r.service.key()));
        let words = if on { say("Requested") } else { say("Cancelled") };
        info!("services: {} {:?}", if on { "called" } else { "cancelled" }, r.service);
        let (service, name) = (r.service, caller.first.clone());
        commands.queue(move |w: &mut World| request(w, me, service, on, &name, words));
        play.write(crate::sound::PlaySound::ui("ui_hardwindow_close"));
        close(&mut commands, &mut d);
        return;
    }
    // The funds, the Call button and why it can't be pressed.
    let sig = (list.clone(), d.picked, funds);
    if d.shown.as_ref() == Some(&sig) {
        return;
    }
    d.shown = Some(sig);
    if let Some(t) = s.id(FUNDS).and_then(|w| s.text_of(w)) {
        let words = loc(&ui, "Ui/Caption/Shopping/Cart:AvailableFunds", "Available funds: {0.String}").replace("{0.String}", &format!("§{funds}"));
        commands.entity(t).insert(Text::new(words));
    }
    if let Some(b) = s.id(CALL) {
        if let Ok(mut btn) = buttons.get_mut(b) {
            let off = can != Some(Ok(()));
            if btn.disabled != off {
                btn.disabled = off;
            }
        }
        let tip = match can {
            Some(Err("AlreadyActive")) => loc(&ui, "Gameplay/UI/ServicesUIWindow:AlreadyActive", "This service is already active."),
            Some(Err(_)) => loc(&ui, "Gameplay/UI/ShoppingUIWindow:InsufficientFundsDialogTitle", "Insufficient Funds"),
            _ if picked.as_ref().is_some_and(|r| r.active && r.service.recurring()) => loc(&ui, "Gameplay/UI/ServicesUIWindow:RecurrentAlreadyActive", "This service is active. Call to cancel."),
            _ => String::new(),
        };
        commands.entity(b).insert(crate::icons::Tooltip(tip));
    }
    // The rows, afresh: each service's name (a recurring one's "- Recurring", or "- Cancel
    // Services" while it's on) and its price.
    for e in d.rows.drain(..) {
        commands.entity(e).try_despawn();
    }
    let Some(t) = s.id(TABLE) else { return };
    let (font, line) = ui.text_font(&mut fonts, 0);
    let holder = commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(HEADER), width: Val::Px(COLUMNS.iter().sum()), height: Val::Px(ROWS as f32 * ROW), flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() },
            Pickable::IGNORE,
            ChildOf(t),
        ))
        .id();
    for (n, r) in list.iter().enumerate() {
        let mut name = loc(&ui, &format!("Ui/Caption/Services/Service:{}", r.service.key()), r.service.fallback_name());
        if r.service.recurring() {
            let more = if r.active { loc(&ui, "Ui/Caption/Services:CancelServices", "Cancel Services") } else { loc(&ui, "Ui/Caption/Services:Recurring", "Recurring") };
            name = format!("{name} - {more}");
        }
        let price = match r.service.price() {
            0 => loc(&ui, "Ui/Caption/Services/Price:Free", "Free!"),
            p if r.service.recurring() => loc(&ui, "Ui/Caption/Services/Price:Recurring", "{0.Money}/day").replace("{0.Money}", &format!("§{p}")),
            p => format!("§{p}"),
        };
        let bg = if d.picked == Some(n) { crate::layout::color(PICKED) } else if n % 2 == 1 { crate::layout::color(ALTERNATE) } else { Color::NONE };
        let colour = crate::layout::color(if r.active { ACTIVE_TEXT } else { ROW_TEXT });
        let row = commands.spawn((Button, ServiceRow(n), Node { height: Val::Px(ROW), flex_shrink: 0.0, align_items: AlignItems::Center, ..default() }, BackgroundColor(bg), ChildOf(holder))).id();
        for (w, words) in COLUMNS.iter().zip([name, price]) {
            let cell = commands.spawn((Node { width: Val::Px(*w), padding: UiRect::left(Val::Px(6.0)), ..default() }, Pickable::IGNORE, ChildOf(row))).id();
            commands.spawn((Text::new(words), font.clone(), line, TextColor(colour), Pickable::IGNORE, ChildOf(cell)));
        }
    }
    d.rows.push(holder);
}

/// A service called (or a recurring one stopped), as the game's `MakeServiceRequest`: the
/// service's words to say so, then on its way.
fn request(w: &mut World, sim: Entity, service: Service, on: bool, name: &str, words: Option<String>) {
    let now = w.resource::<crate::clock::GameClock>().minutes;
    let say = |w: &mut World, s: String| w.resource_mut::<crate::interact::Notifications>().push(s);
    if let Some(words) = words {
        say(w, words);
    }
    match service {
        Service::Babysitter => w.resource_mut::<crate::services::Sitter>().request(),
        Service::Maid => w.resource_mut::<crate::services::MaidService>().hired = on,
        Service::PizzaDelivery => {
            let price = crate::meals::PIZZA_PRICE;
            let Some(mut h) = w.get_resource_mut::<crate::interact::Household>() else { return };
            if h.funds < price {
                return;
            }
            h.funds -= price;
            w.insert_resource(crate::meals::PizzaOrder { arrive_at: now + 60.0 });
            w.write_message(crate::dreams::DreamEvent { key: Some("PizzaDelivery".into()), ..crate::dreams::DreamEvent::new(sim, "kOrderedService") });
        }
        Service::Repairman => w.insert_resource(crate::interact::RepairmanVisit { arrive_at: now + 90.0 }),
        Service::SocialWorkerAdoption => {
            // (Who's to come home: the game asks once the social worker's there; here, now.)
            w.insert_resource(crate::hud::AdoptChoice(sim));
        }
        Service::Police | Service::Firefighter => {
            // (Called with nothing to come for: a false call, fined, as the game's.)
            let emergency = if service == Service::Police { w.resource::<crate::burglar::Break>().police_wanted() } else { w.query_filtered::<(), With<crate::fire::Fire>>().iter(w).next().is_some() };
            if emergency {
                let kind = if service == Service::Police { crate::interact::ActionKind::CallPolice } else { crate::interact::ActionKind::CallFirefighters };
                crate::calls::emergency_call(w, sim, kind, name, now);
            } else {
                let fine = FALSE_CALL_FINE;
                if let Some(mut h) = w.get_resource_mut::<crate::interact::Household>() {
                    h.funds = (h.funds - fine).max(0);
                }
                let key = if service == Service::Police { "Gameplay/Services/PoliceSituation/LectureSim:PoliceLecture" } else { "Gameplay/Services/FirefighterSituation/RouteToScold:ChargeHousehold" };
                let words = w
                    .get_resource::<UiAssets>()
                    .and_then(|u| u.localize(key))
                    .unwrap_or_else(|| "You shouldn't play with the phone like that. We're going to fine you {0.Money}.".into())
                    .replace("{0.Money}", &format!("§{fine}"))
                    .replace("{0.Number}", &fine.to_string());
                say(w, words);
            }
        }
        Service::NewspaperDelivery => w.resource_mut::<crate::mail::PaperRound>().cancelled = !on,
    }
}

/// What a false call to the police or the fire department costs (their tuned `kCost`).
pub const FALSE_CALL_FINE: i64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_lists_the_base_games_services_by_price() {
        let r = rows(false, |_| false);
        assert!(r.iter().all(|r| r.service != Service::Babysitter));
        let r = rows(true, |_| false);
        assert_eq!(r.len(), 8);
        assert_eq!(r.iter().map(|r| r.service.price()).collect::<Vec<_>>(), vec![0, 0, 0, 0, 30, 50, 75, 125]);
        assert_eq!(r.last().unwrap().service, Service::Maid);
    }

    #[test]
    fn call_waits_for_money_and_stops_a_recurring_service() {
        let maid = Row { service: Service::Maid, active: false };
        assert_eq!(callable(&maid, 100), Err("InsufficientFunds"));
        assert_eq!(callable(&maid, 200), Ok(()));
        assert_eq!(callable(&Row { active: true, ..maid.clone() }, 0), Ok(()));
        assert_eq!(callable(&Row { service: Service::PizzaDelivery, active: true }, 999), Err("AlreadyActive"));
        assert_eq!(callable(&Row { service: Service::Police, active: false }, 0), Ok(()));
    }
}
