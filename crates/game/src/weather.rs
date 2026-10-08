//! Seasons and weather, as the Seasons expansion has them (from its tuning: the `Seasons` table
//! and SeasonsManager's managers). Summer, fall, winter and spring come round a week each
//! (`kDefaultDaysPerSeason`), the days shorter in fall and winter. Temperatures run between the
//! season's lows and highs for the time of day (mornings, noons, evenings, nights), somewhere
//! between them that drifts from day to day. The weather is picked by its chances in the season
//! (sun, rain, snow, fog, hail, each among the temperatures it comes at), lasts its hours, and
//! rain and snow fall light, moderate or heavy. Clouds gather for it; rain wets the ground and
//! melts snow, which builds up when it snows (frost first) and melts as it warms, as the
//! terrain-cover tuning says; heavy rain brings lightning and thunder.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::PlayMode;
use crate::clock::GameClock;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Weather>()
            .add_message::<Thunder>()
            .add_plugins(crate::weather_fx::WeatherFxPlugin)
            .add_systems(Update, (simulate, sim_temperatures).chain().run_if(in_state(PlayMode::Live)));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize, Hash)]
pub enum Season {
    #[default]
    Summer,
    Fall,
    Winter,
    Spring,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Summer, Season::Fall, Season::Winter, Season::Spring];

    pub fn name(self) -> &'static str {
        match self {
            Season::Summer => "Summer",
            Season::Fall => "Fall",
            Season::Winter => "Winter",
            Season::Spring => "Spring",
        }
    }

    /// Hours added to sunrise and to sunset (`kFallSunriseAndSunsetOffsets`...): later mornings
    /// and earlier evenings away from summer.
    pub fn daylight(self) -> (f32, f32) {
        match self {
            Season::Summer => (0.0, 0.0),
            Season::Fall | Season::Spring => (1.0, -1.25),
            Season::Winter => (2.0, -2.5),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum WeatherKind {
    #[default]
    Sunny,
    Rain,
    Snow,
    Fog,
    Hail,
}

impl WeatherKind {
    pub fn name(self) -> &'static str {
        match self {
            WeatherKind::Sunny => "Sunny",
            WeatherKind::Rain => "Rain",
            WeatherKind::Snow => "Snow",
            WeatherKind::Fog => "Fog",
            WeatherKind::Hail => "Hail",
        }
    }

    fn from_name(n: &str) -> Option<Self> {
        [WeatherKind::Sunny, WeatherKind::Rain, WeatherKind::Snow, WeatherKind::Fog, WeatherKind::Hail].into_iter().find(|k| k.name().eq_ignore_ascii_case(n))
    }

    /// Whether something falls from the sky.
    pub fn falls(self) -> bool {
        matches!(self, WeatherKind::Rain | WeatherKind::Snow | WeatherKind::Hail)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Intensity {
    Light,
    #[default]
    Moderate,
    Heavy,
}

impl Intensity {
    fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        ["Light", "Moderate", "Heavy"][self.index()]
    }
}

/// The town's season and weather (kept in saves).
#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Weather {
    pub kind: WeatherKind,
    pub intensity: Intensity,
    /// When it started and until when it lasts (game minutes).
    pub since: f64,
    pub until: f64,
    /// Now (°F).
    pub temperature: f32,
    /// Where between the season's lows and highs today's temperatures run (0..1), and the day
    /// it was chosen for.
    pub variation: f32,
    pub variation_day: u32,
    /// The ground: snow on it, frost, wetness (0..100, as the game's terrain cover levels).
    pub snow: f32,
    pub frost: f32,
    pub wet: f32,
    /// Cloud cover (0..1) and where it's heading.
    pub clouds: f32,
    pub cloud_target: f32,
    pub next_clouds: f64,
    pub next_lightning: f64,
    /// Days in each season, and the season day 0 falls in (index into `Season::ALL`), and how
    /// many days into it.
    pub season_days: u32,
    pub first_season: usize,
    pub first_day: u32,
    /// The game minute last simulated.
    pub last: f64,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            kind: WeatherKind::Sunny,
            intensity: Intensity::Moderate,
            since: 0.0,
            until: 0.0,
            temperature: f32::NAN,
            variation: 0.5,
            variation_day: u32::MAX,
            snow: 0.0,
            frost: 0.0,
            wet: 0.0,
            clouds: 0.2,
            cloud_target: 0.2,
            next_clouds: 0.0,
            next_lightning: 0.0,
            season_days: DAYS_PER_SEASON,
            first_season: 0,
            first_day: 0,
            last: 0.0,
        }
    }
}

/// `kDefaultDaysPerSeason`.
const DAYS_PER_SEASON: u32 = 7;
/// `kMaximumTempuratureChangePerHour` (°F an hour).
const MAX_TEMP_CHANGE: f32 = 10.0;
/// `kMaxVariationChange`: how far a day's temperatures can move from the day before's.
const MAX_VARIATION_CHANGE: f32 = 0.5;
/// Lightning (`RainEvent`): sim minutes between strikes, from moderate rain up.
const LIGHTNING_MINUTES: (f32, f32) = (15.0, 45.0);

impl Weather {
    pub fn season(&self, day: u32) -> Season {
        Season::ALL[(((day + self.first_day) / self.season_days.max(1)) as usize + self.first_season) % 4]
    }

    /// How far through the season (0..1).
    pub fn season_progress(&self, minutes: f64) -> f32 {
        let days = self.season_days.max(1) as f64;
        ((minutes / 1440.0 + self.first_day as f64) % days / days) as f32
    }

    /// The day of the season (1..).
    pub fn season_day(&self, day: u32) -> u32 {
        (day + self.first_day) % self.season_days.max(1) + 1
    }

    /// How hard it's coming down (0..1): its intensity, easing in over its first half hour and
    /// out over its last.
    pub fn falling(&self, minutes: f64) -> f32 {
        if !self.kind.falls() {
            return 0.0;
        }
        let ease = (((minutes - self.since) / 30.0).min((self.until - minutes) / 30.0)).clamp(0.0, 1.0) as f32;
        [0.35, 0.65, 1.0][self.intensity.index()] * ease
    }

    /// How overcast (0..1): nothing for a few clouds, all of it under rain or snow.
    pub fn overcast(&self) -> f32 {
        ((self.clouds - 0.4) / 0.6).clamp(0.0, 1.0)
    }

    /// How thick the fog (0..1): fog's, and some in heavy rain and snow.
    pub fn fog(&self, minutes: f64) -> f32 {
        match self.kind {
            WeatherKind::Fog => (((minutes - self.since) / 45.0).min((self.until - minutes) / 45.0)).clamp(0.0, 1.0) as f32,
            WeatherKind::Snow => self.falling(minutes) * 0.55,
            WeatherKind::Rain | WeatherKind::Hail => self.falling(minutes) * 0.35,
            WeatherKind::Sunny => 0.0,
        }
    }

    /// Sunrise and sunset (hours): later and earlier away from summer
    /// (`kWinterSunriseAndSunsetOffsets`...), easing from one season's to the next's over its
    /// first two days (`kDaysForSeasonTransition`).
    pub fn daylight(&self, minutes: f64) -> (f32, f32) {
        let day = (minutes / 1440.0) as u32;
        let (now, before) = (self.season(day).daylight(), self.season(day.saturating_sub(self.season_days)).daylight());
        let into = ((minutes / 1440.0 + self.first_day as f64) % self.season_days.max(1) as f64) as f32;
        let f = (into / 2.0).clamp(0.0, 1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * f;
        (6.0 + lerp(before.0, now.0), 20.0 + lerp(before.1, now.1))
    }

    /// The trees (`TreeManager`): fall colour, how many leaves have gone (0..1) and snow on
    /// them. Leaves turn in the first fifth of fall (`kFallColorChangeEndTime`), drop from then
    /// till nine tenths of the way through (`kFallLeavesStartTime`, `kFallLeavesEndTime`), and
    /// are back by halfway through spring (`kSpringLeavesRegrowTime`).
    pub fn trees(&self, minutes: f64) -> Vec3 {
        let p = self.season_progress(minutes);
        let ramp = |a: f32, b: f32| ((p - a) / (b - a)).clamp(0.0, 1.0);
        let (colour, gone) = match self.season((minutes / 1440.0) as u32) {
            Season::Summer => (0.0, 0.0),
            Season::Fall => (ramp(0.0, 0.2), ramp(0.2, 0.9)),
            Season::Winter => (1.0, 1.0),
            Season::Spring => (1.0 - ramp(0.0, 0.5), 1.0 - ramp(0.0, 0.5)),
        };
        Vec3::new(colour, gone, (self.snow / 60.0).clamp(0.0, 1.0).max(if self.kind == WeatherKind::Snow { self.falling(minutes) * 0.6 } else { 0.0 }))
    }

    pub fn raining(&self) -> bool {
        matches!(self.kind, WeatherKind::Rain | WeatherKind::Hail)
    }

    /// The weather in a few words: "Light Rain", "Fog", "Sunny".
    pub fn describe(&self) -> String {
        match self.kind {
            WeatherKind::Rain | WeatherKind::Snow => format!("{} {}", self.intensity.name(), self.kind.name()),
            k => k.name().to_string(),
        }
    }
}

/// Whether a point is under a roof: in a room of the house (or of the lot being visited).
pub fn sheltered(building: Option<&crate::building::ActiveBuilding>, p: Vec3) -> bool {
    let inside = |b: &crate::building::ActiveBuilding| b.room_at(b.level_at(p.y + 0.3), p).is_some_and(|k| k != s3bake::ROOM_OUTSIDE && k != s3bake::ROOM_PORCH);
    building.is_some_and(|b| inside(b) || b.away.as_deref().is_some_and(inside))
}

/// A Sim's temperature (`SimTemperature`, -100 freezing .. 100 baking; 0 comfortable), and how
/// long they've been out in the rain.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct BodyTemperature {
    pub value: f32,
    pub in_rain: f32,
}

/// `SimTemperature` tuning: the world temperature that's neither warm nor cold to a Sim
/// (`kSimNeutralTempInWorldTemp`), how many of a Sim's degrees one of the world's is
/// (`kSimTempToWorldTempMultiplier`), and how quickly a Sim's temperature follows outdoors and
/// returns to normal indoors (an hour).
const NEUTRAL_TEMP: f32 = 60.0;
const SIM_DEGREES: f32 = 3.0;
const RATE_OUTDOORS: f32 = 10.0;
const RATE_INDOORS: f32 = 40.0;
/// `kTemperatureThresholdToGetFrostbitten`.
const FROSTBITE: f32 = -95.0;
/// `kDelayBeforeSoakedBuffFromRain`: minutes out in light, moderate, heavy rain.
const SOAKED_AFTER: [f32; 3] = [60.0, 55.0, 50.0];

/// Sims feel the weather: out of doors their temperature heads for the world's, indoors back
/// to comfortable, giving the game's Temperature motive buffs (Teeth Chattering from -71, Getting
/// Chilly from -31, Getting Warm from 30, Sweating Profusely from 71, Frostbitten at -95); and
/// out in the rain long enough they're Soaked.
#[allow(clippy::type_complexity)]
fn sim_temperatures(
    mut commands: Commands,
    delta: Res<crate::clock::SimDelta>,
    clock: Res<GameClock>,
    w: Res<Weather>,
    building: Option<Res<crate::building::ActiveBuilding>>,
    mut sims: Query<
        (Entity, &Transform, &mut crate::life::Moodlets, Option<&mut BodyTemperature>, Has<crate::swim::Swimming>),
        (With<crate::sim::Sim>, Without<crate::interact::OffLot>, Without<crate::rabbitholes::AtRabbitHole>, Without<crate::careers::AtWork>),
    >,
) {
    let dh = delta.0 / 60.0;
    if dh <= 0.0 || w.temperature.is_nan() {
        return;
    }
    use crate::life::MoodletKind as M;
    let world = ((w.temperature - NEUTRAL_TEMP) * SIM_DEGREES).clamp(-100.0, 100.0);
    let raining = w.raining() && w.falling(clock.minutes) > 0.1;
    for (e, tf, mut moodlets, t, swimming) in &mut sims {
        let Some(mut t) = t else {
            commands.entity(e).insert(BodyTemperature::default());
            continue;
        };
        let outdoors = !sheltered(building.as_deref(), tf.translation);
        let (target, rate) = if outdoors { (world, RATE_OUTDOORS) } else { (0.0, RATE_INDOORS) };
        let step = rate * dh;
        t.value += (target - t.value).clamp(-step, step);
        let v = t.value;
        moodlets.set_while(M::TeethChattering, v <= -71.0);
        moodlets.set_while(M::GettingChilly, (-71.0..=-31.0).contains(&v));
        moodlets.set_while(M::GettingWarm, (30.0..71.0).contains(&v));
        moodlets.set_while(M::SweatingProfusely, v >= 71.0);
        if v <= FROSTBITE && !moodlets.has(M::Frostbitten) {
            moodlets.add(M::Frostbitten, clock.minutes);
        }
        // (Swimmers are wet anyway.)
        if outdoors && raining && !swimming {
            t.in_rain += delta.0;
            if t.in_rain >= SOAKED_AFTER[w.intensity.index()] {
                moodlets.add(M::Soaked, clock.minutes);
            }
        } else {
            t.in_rain = 0.0;
        }
    }
}

/// The World Adventures destinations, where Seasons doesn't reach.
pub fn vacation(world: &str) -> bool {
    ["Shang Simla", "Al Simhara", "Champs Les Sims"].contains(&world)
}

/// Lightning has struck: thunder follows (near, medium or far).
#[derive(Message, Clone, Copy)]
pub struct Thunder {
    pub distance: u8,
}

/// The season's temperature now (°F): between its lows and highs for the time of day (mornings
/// at 8:00, noons at 14:00, evenings at 19:00, nights at 2:00, and in between), `variation` of
/// the way up.
fn target_temperature(t: &s3bake::gamedata::SeasonsTuning, season: Season, hour: f32, variation: f32) -> f32 {
    let Some((_, table)) = t.temperature.iter().find(|(s, _)| s == season.name()) else { return 70.0 };
    const AT: [f32; 4] = [8.0, 14.0, 19.0, 26.0];
    let at = |i: usize| table[i % 4][0] + (table[i % 4][1] - table[i % 4][0]) * variation;
    let h = if hour < AT[0] { hour + 24.0 } else { hour };
    let i = AT.iter().rposition(|a| h >= *a).unwrap_or(3);
    let (a, b) = (AT[i], if i == 3 { AT[0] + 24.0 } else { AT[i + 1] });
    let f = ((h - a) / (b - a)).clamp(0.0, 1.0);
    at(i) + (at(i + 1) - at(i)) * f
}

/// The weather next: one of the season's kinds that comes at this temperature, by their
/// weights, for between its least and most hours; how heavy by its weights.
fn pick(t: &s3bake::gamedata::SeasonsTuning, season: Season, temperature: f32, rng: &mut impl Rng) -> (WeatherKind, Intensity, f32) {
    let options: Vec<&s3bake::gamedata::WeatherProfile> =
        t.weather.iter().filter(|w| w.season == season.name() && w.weight > 0.0 && (w.min_temp..=w.max_temp).contains(&temperature)).collect();
    let total: f32 = options.iter().map(|w| w.weight).sum();
    let mut r = rng.random_range(0.0..total.max(1e-3));
    let Some(w) = options.iter().find(|w| {
        r -= w.weight;
        r < 0.0
    }) else {
        return (WeatherKind::Sunny, Intensity::Moderate, 3.0);
    };
    let kind = WeatherKind::from_name(&w.kind).unwrap_or_default();
    let weights = w.intensity;
    let sum: f32 = weights.iter().sum();
    let intensity = if sum <= 0.0 {
        Intensity::Moderate
    } else {
        let mut r = rng.random_range(0.0..sum);
        let i = weights.iter().position(|x| {
            r -= x;
            r < 0.0
        });
        [Intensity::Light, Intensity::Moderate, Intensity::Heavy][i.unwrap_or(1)]
    };
    let hours = if w.max_length > w.min_length { rng.random_range(w.min_length..w.max_length) } else { w.max_length };
    (kind, intensity, hours.max(1.0))
}

/// The freeze/melt rate at this temperature (an hour): from the table's points, in between.
fn freeze_melt(t: &s3bake::gamedata::SeasonsTuning, temperature: f32) -> f32 {
    let p = &t.freeze_melt;
    let Some(first) = p.first() else { return 0.0 };
    if temperature <= first[0] {
        return first[1];
    }
    for w in p.windows(2) {
        if temperature <= w[1][0] {
            let f = (temperature - w[0][0]) / (w[1][0] - w[0][0]).max(1e-3);
            return w[0][1] + (w[1][1] - w[0][1]) * f;
        }
    }
    p.last().map_or(0.0, |l| l[1])
}

/// The test settings: `WEATHER=<kind>[:<intensity>]` (rain, snow, fog, hail, sunny),
/// `SEASON=<season>[:<day>]`, `SNOW=<level>`, `WET=<level>`, `TEMP=<°F>`.
fn test_settings(w: &mut Weather, clock: &GameClock) {
    if let Ok(v) = std::env::var("SEASON")
        && let (s, d) = v.split_once(':').map_or((v.as_str(), 1), |(s, d)| (s, d.parse::<u32>().unwrap_or(1)))
        && let Some(i) = Season::ALL.iter().position(|x| x.name().eq_ignore_ascii_case(s))
    {
        // (Today the day asked for of that season.)
        let days = w.season_days.max(1);
        let today = clock.day() % days;
        w.first_day = (d.saturating_sub(1) % days + days - today) % days;
        let here = (((clock.day() + w.first_day) / days) as usize) % 4;
        w.first_season = (i + 4 - here) % 4;
    }
    if let Ok(v) = std::env::var("WEATHER") {
        let (k, i) = v.split_once(':').unwrap_or((&v, "moderate"));
        if let Some(kind) = WeatherKind::from_name(k) {
            w.kind = kind;
            w.intensity = [Intensity::Light, Intensity::Moderate, Intensity::Heavy].into_iter().find(|x| x.name().eq_ignore_ascii_case(i)).unwrap_or_default();
            w.since = clock.minutes - 60.0;
            w.until = clock.minutes + 24.0 * 60.0;
            w.clouds = if kind.falls() { 1.0 } else { w.clouds };
        }
    }
    let num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
    if let Some(s) = num("SNOW") {
        w.snow = s;
        w.frost = 100.0;
    }
    if let Some(s) = num("WET") {
        w.wet = s;
    }
    if let Some(t) = num("TEMP") {
        w.temperature = t;
    }
}

#[allow(clippy::too_many_arguments)]
fn simulate(
    clock: Res<GameClock>,
    ui: Option<Res<crate::icons::GameUi>>,
    mut w: ResMut<Weather>,
    mut notes: ResMut<crate::interact::Notifications>,
    world: Option<Res<crate::data::SelectedWorld>>,
    mut thunder: MessageWriter<Thunder>,
    mut tested: Local<bool>,
) {
    let Some(ui) = ui else { return };
    let t = &ui.data.seasons;
    // (The vacation destinations have no seasons: always summer, always fine.)
    if t.temperature.is_empty() || world.as_ref().is_some_and(|w| vacation(&w.0.name)) {
        return;
    }
    let now = clock.minutes;
    let mut rng = rand::rng();
    let season = w.season(clock.day());
    // (Starting out, or a jump in time: the weather taken up as it is now.)
    if w.last <= 0.0 || now < w.last || now - w.last > 2.0 * 1440.0 {
        w.last = now;
        if w.temperature.is_nan() {
            w.temperature = target_temperature(t, season, clock.hour_f(), w.variation);
        }
        if !*tested {
            *tested = true;
            test_settings(&mut w, &clock);
        }
        return;
    }
    let dh = ((now - w.last) / 60.0) as f32;
    if dh <= 0.0 {
        return;
    }
    let before = w.season((w.last / 1440.0) as u32);
    w.last = now;
    if before != season {
        let town = world.as_ref().map_or("town".to_string(), |w| w.0.name.clone());
        notes.push(match season {
            Season::Summer => format!("Summer has come to {town}: long, warm days."),
            Season::Fall => format!("Fall has come to {town}: the leaves are turning."),
            Season::Winter => format!("Winter has come to {town}: wrap up warm!"),
            Season::Spring => format!("Spring has come to {town}: the days are getting longer."),
        });
    }
    // Each day's temperatures somewhere new between the season's lows and highs.
    if w.variation_day != clock.day() {
        w.variation_day = clock.day();
        w.variation = (w.variation + rng.random_range(-MAX_VARIATION_CHANGE..MAX_VARIATION_CHANGE)).clamp(0.0, 1.0);
    }
    let mut target = target_temperature(t, season, clock.hour_f(), w.variation);
    // (Rain needs it above freezing, snow below 40°F...)
    if let Some(p) = t.weather.iter().find(|p| p.season == season.name() && WeatherKind::from_name(&p.kind) == Some(w.kind)) {
        target = target.clamp(p.min_temp, p.max_temp);
    }
    let step = MAX_TEMP_CHANGE * dh;
    w.temperature += (target - w.temperature).clamp(-step, step);
    // The next weather, once this has run its course.
    if now >= w.until {
        let (kind, intensity, hours) = pick(t, season, w.temperature, &mut rng);
        if kind != w.kind && kind.falls() {
            debug!("weather: {} {} for {hours:.1} h at {:.0}°F", intensity.name(), kind.name(), w.temperature);
        }
        w.kind = kind;
        w.intensity = intensity;
        w.since = now;
        w.until = now + hours as f64 * 60.0;
    }
    // Clouds: gathered for rain and snow, a blanket for fog, otherwise drifting about.
    if now >= w.next_clouds {
        w.next_clouds = now + rng.random_range(3.0..5.0) * 60.0;
        w.cloud_target = rng.random_range(0.0..0.45);
    }
    let want = match w.kind {
        k if k.falls() => 1.0,
        WeatherKind::Fog => 0.75,
        _ => w.cloud_target,
    };
    let rate = 0.6 * dh;
    w.clouds += (want - w.clouds).clamp(-rate, rate);
    // The ground: the terrain cover tuning (hours under light, moderate, heavy).
    let cover = |k: &str, i: usize, d: f32| t.cover.get(k).and_then(|v| v.get(i).or(v.first())).copied().unwrap_or(d).max(0.01);
    let i = w.intensity.index();
    let per = |hours: f32| 100.0 / hours * dh;
    match w.kind {
        WeatherKind::Snow => {
            w.frost += per(cover("kFrostAccumulationTimeWhenSnowing", i, 1.0));
            if w.frost >= 100.0 {
                w.snow += per(cover("kSnowAccumulationTime", i, 12.0));
            }
            w.wet -= per(cover("kSnowDryingWetGroundTime", i, 1.0));
        }
        WeatherKind::Rain | WeatherKind::Hail => {
            w.wet += per(cover("kRainAccumulationTime", i, 6.0));
            let melt = per(cover("kRainMeltingSnowAndFrostTime", i, 6.0));
            w.snow -= melt;
            w.frost -= melt;
        }
        _ => w.wet -= per(cover("kRainDryTime", 0, 4.0)),
    }
    // Warmth melts snow and frost (the cold freezes ponds; snow stays).
    let melt = freeze_melt(t, w.temperature);
    if melt > 0.0 && w.kind != WeatherKind::Snow {
        w.snow -= melt * dh;
        w.frost -= melt * dh;
    }
    let w = &mut *w;
    for v in [&mut w.snow, &mut w.frost, &mut w.wet] {
        *v = v.clamp(0.0, 100.0);
    }
    // Lightning, in moderate rain and heavier.
    if w.kind == WeatherKind::Rain && w.intensity >= Intensity::Moderate && w.falling(now) > 0.5 {
        if w.next_lightning < w.since {
            w.next_lightning = now + rng.random_range(LIGHTNING_MINUTES.0..LIGHTNING_MINUTES.1) as f64 * 0.3;
        }
        if now >= w.next_lightning {
            w.next_lightning = now + rng.random_range(LIGHTNING_MINUTES.0..LIGHTNING_MINUTES.1) as f64;
            thunder.write(Thunder { distance: rng.random_range(0..3) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning() -> s3bake::gamedata::SeasonsTuning {
        let row = |a: f32, b: f32| [a, b];
        s3bake::gamedata::SeasonsTuning {
            temperature: vec![
                ("Summer".into(), [row(60.0, 80.0), row(70.0, 100.0), row(60.0, 80.0), row(50.0, 70.0)]),
                ("Winter".into(), [row(10.0, 35.0), row(10.0, 45.0), row(10.0, 35.0), row(0.0, 35.0)]),
            ],
            freeze_melt: vec![[0.0, -100.0], [32.0, 0.0], [70.0, 30.0], [100.0, 100.0]],
            weather: vec![
                s3bake::gamedata::WeatherProfile { kind: "Snow".into(), season: "Winter".into(), weight: 6.0, min_length: 6.0, max_length: 18.0, min_temp: -1000.0, max_temp: 40.0, intensity: [1.0, 3.0, 2.0] },
                s3bake::gamedata::WeatherProfile { kind: "Rain".into(), season: "Winter".into(), weight: 0.0, min_length: 0.0, max_length: 3.0, min_temp: 30.0, max_temp: 1000.0, intensity: [1.0, 1.0, 1.0] },
            ],
            cover: Default::default(),
        }
    }

    #[test]
    fn temperatures_follow_the_day() {
        let t = tuning();
        // Noon in summer, halfway between its low and high; the night cooler.
        assert!((target_temperature(&t, Season::Summer, 14.0, 0.5) - 85.0).abs() < 0.01);
        assert!((target_temperature(&t, Season::Summer, 2.0, 0.5) - 60.0).abs() < 0.01);
        // (In between: past noon towards evening.)
        let t16 = target_temperature(&t, Season::Summer, 16.5, 0.5);
        assert!(t16 < 85.0 && t16 > 70.0, "{t16}");
        assert!(target_temperature(&t, Season::Winter, 14.0, 1.0) <= 45.0);
    }

    #[test]
    fn winter_snows_and_warmth_melts() {
        let t = tuning();
        let mut rng = rand::rng();
        // (Rain has no weight in winter: always snow, among the hours it lasts.)
        for _ in 0..20 {
            let (k, _, h) = pick(&t, Season::Winter, 20.0, &mut rng);
            assert_eq!(k, WeatherKind::Snow);
            assert!((6.0..=18.0).contains(&h));
        }
        assert!(freeze_melt(&t, 16.0) < 0.0);
        assert!((freeze_melt(&t, 51.0) - 15.0).abs() < 0.01);
    }

    #[test]
    fn seasons_turn_weekly() {
        let w = Weather::default();
        assert_eq!(w.season(0), Season::Summer);
        assert_eq!(w.season(7), Season::Fall);
        assert_eq!(w.season(15), Season::Winter);
        assert_eq!(w.season(27), Season::Spring);
        assert_eq!(w.season(28), Season::Summer);
        assert_eq!(w.season_day(9), 3);
    }
}
