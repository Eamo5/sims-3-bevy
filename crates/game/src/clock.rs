//! Game clock, speed controls and the day/night cycle.

use bevy::prelude::*;

use crate::{AppState, PlayMode};

pub struct ClockPlugin;

impl Plugin for ClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Night>().insert_resource(GameClock::default())
            .insert_resource(SimDelta::default())
            .add_systems(
                Update,
                (speed_keys, advance_clock, day_night).chain().run_if(in_state(PlayMode::Live)),
            )
            .add_systems(OnEnter(AppState::InGame), reset_clock);
    }
}

pub const SPEED_RATES: [f32; 4] = [0.0, 1.0, 3.0, 12.0];
const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// Game time in minutes since the start of the first Monday.
#[derive(Resource)]
pub struct GameClock {
    pub minutes: f64,
    pub speed: usize,
    paused_from: usize,
}

impl Default for GameClock {
    fn default() -> Self {
        Self { minutes: 8.0 * 60.0, speed: 1, paused_from: 1 }
    }
}

impl GameClock {
    pub fn hour_f(&self) -> f32 {
        ((self.minutes / 60.0) % 24.0) as f32
    }
    pub fn day(&self) -> u32 {
        (self.minutes / 1440.0) as u32
    }
    pub fn weekday(&self) -> usize {
        (self.day() % 7) as usize
    }
    pub fn weekday_name(&self) -> &'static str {
        DAYS[self.weekday()]
    }
    pub fn is_workday(&self) -> bool {
        self.weekday() < 5
    }
    pub fn time_string(&self) -> String {
        let total = (self.minutes as u64) % 1440;
        let (h, m) = (total / 60, total % 60);
        let (h12, ampm) = match h {
            0 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        format!("{h12}:{m:02} {ampm}")
    }
    pub fn set_speed(&mut self, s: usize) {
        if s == 0 && self.speed != 0 {
            self.paused_from = self.speed;
        }
        self.speed = s.min(3);
    }
    pub fn toggle_pause(&mut self) {
        if self.speed == 0 {
            self.speed = self.paused_from.max(1);
        } else {
            self.set_speed(0);
        }
    }
}

/// Game minutes elapsed this frame (0 while paused).
#[derive(Resource, Default)]
pub struct SimDelta(pub f32);

fn reset_clock(mut clock: ResMut<GameClock>) {
    *clock = GameClock::default();
}

fn speed_keys(keys: Res<ButtonInput<KeyCode>>, mut clock: ResMut<GameClock>) {
    if keys.just_pressed(KeyCode::KeyP) || keys.just_pressed(KeyCode::Digit0) || keys.just_pressed(KeyCode::Space) {
        clock.toggle_pause();
    }
    if keys.just_pressed(KeyCode::Digit1) {
        clock.set_speed(1);
    }
    if keys.just_pressed(KeyCode::Digit2) {
        clock.set_speed(2);
    }
    if keys.just_pressed(KeyCode::Digit3) {
        clock.set_speed(3);
    }
}

fn advance_clock(time: Res<Time>, mut clock: ResMut<GameClock>, mut delta: ResMut<SimDelta>, modal: Query<(), With<crate::dialog::Modal>>) {
    // Normal speed: one game minute per real second, like the original (and the game stands
    // still while a question's waiting for an answer).
    let rate = if modal.is_empty() { SPEED_RATES[clock.speed] } else { 0.0 };
    let dt = time.delta_secs().min(0.1) * rate;
    clock.minutes += dt as f64;
    delta.0 = dt;
}

/// The sun's height (-1..1, 0 at the horizon): up at 6:00, highest at 13:00, down at 20:00, as
/// the game's summer days.
pub fn sun_elevation(h: f32) -> f32 {
    sun_elevation_in(h, 6.0, 20.0)
}

/// The sun's height with sunrise and sunset at these hours (the seasons move them).
pub fn sun_elevation_in(h: f32, rise: f32, set: f32) -> f32 {
    let h = h.rem_euclid(24.0);
    let day = (set - rise).clamp(1.0, 23.0);
    if (rise..set).contains(&h) { ((h - rise) / day * std::f32::consts::PI).sin() } else { -((h - set).rem_euclid(24.0) / (24.0 - day) * std::f32::consts::PI).sin() }
}

/// How dark it is: 0 in daylight, 1 at night (smooth through dusk and dawn).
#[derive(Resource, Default, Clone, Copy)]
pub struct Night(pub f32);

#[allow(clippy::too_many_arguments)]
fn day_night(
    clock: Res<GameClock>,
    mut night: ResMut<Night>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight)>,
    mut ambient: Query<&mut AmbientLight>,
    mut clear: ResMut<ClearColor>,
    weather: Option<Res<crate::weather::Weather>>,
    lightning: Option<Res<crate::weather_fx::Lightning>>,
    time: Res<Time>,
) {
    let h = clock.hour_f();
    // Sun angle: rises at 6:00, sets at 20:00 (in summer: later and earlier in the other
    // seasons).
    let (rise, set) = weather.as_ref().map_or((6.0, 20.0), |w| w.daylight(clock.minutes));
    let t = (h - rise) / (set - rise);
    let elev = sun_elevation_in(h, rise, set);
    // (Under cloud the sun's light is dimmed; lightning lights everything up for a moment.)
    let overcast = weather.as_ref().map_or(0.0, |w| w.overcast());
    let flash = lightning.map_or(0.0, |l| l.flash(time.elapsed_secs()));
    let day = elev.clamp(0.0, 1.0);
    let dark = (1.0 - (elev + 0.08) / 0.3).clamp(0.0, 1.0);
    if (night.0 - dark).abs() > 0.002 {
        night.0 = dark;
    }
    let twilight = (1.0 - (elev.abs() * 4.0).min(1.0)).max(0.0);
    let azimuth = t * std::f32::consts::PI + 0.6;
    for (mut tf, mut light) in &mut sun {
        let pitch = -(elev.max(0.08)) * 1.2;
        tf.rotation = Quat::from_euler(EulerRot::YXZ, azimuth, pitch, 0.0);
        // (Evenings stay light until the sun is nearly down.)
        light.illuminance = (400.0 + 9500.0 * day.powf(0.6)) * (1.0 - 0.72 * overcast) + flash * 30000.0;
        light.color = Color::srgb(1.0, 0.85 + 0.12 * day, 0.70 + 0.25 * day).mix(&Color::srgb(1.0, 0.6, 0.35), twilight * 0.6);
    }
    for mut a in &mut ambient {
        a.brightness = (180.0 + 650.0 * day.powf(0.6)) * (1.0 - 0.25 * overcast) + flash * 3000.0;
        a.color = Color::srgb(0.55, 0.62, 0.95).mix(&Color::srgb(0.85, 0.88, 1.0), day).mix(&Color::srgb(0.8, 0.82, 0.85), overcast * 0.7);
    }
    let sky_day = Color::srgb(0.53, 0.70, 0.90);
    let sky_night = Color::srgb(0.03, 0.05, 0.12);
    let sky_dusk = Color::srgb(0.85, 0.55, 0.40);
    clear.0 = sky_night.mix(&sky_day, day).mix(&sky_dusk, twilight * 0.5);
}
