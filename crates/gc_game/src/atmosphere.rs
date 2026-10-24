//! The light the game is in (docs/30-cinematic-lighting.md): the look
//! chosen for the match (the lobby's time of day, or F9), tinted by the
//! lands round the camera, then plain if the player asked for Clarity, then
//! moved by hand in the tuning panel (F10). Presentation only: it reads the
//! map's lands and writes nothing to the world.
//!
//! The lands blend with the camera rather than one look per map, because a
//! grown map is a patchwork: scroll from a meadow into the Waste and the
//! haze turns amber over a few seconds; ride over a river valley and the
//! mist thickens.

use gc_map::Map;
use gc_map::def::Land;
use gc_render::{KNOBS, Knob, LandAir, Look, LookPreset, Weather};
use glam::Vec2;

/// How each land changes the air, in `Land::ALL` order. Meadow, the ground
/// everywhere until painted, is the look as it is.
const AIRS: [LandAir; Land::ALL.len()] = [
    // Meadow.
    LandAir::NEUTRAL,
    // Forest: darker, greener shade under a thicker air.
    LandAir {
        fog: [-0.05, -0.01, -0.04],
        density: 1.15,
        saturation: 0.97,
        sun: [1.0, 1.0, 0.97],
        shadow_tint: [0.95, 1.0, 0.95],
        ..LandAir::NEUTRAL
    },
    // Fen: green mist that hugs the low ground.
    LandAir {
        fog: [-0.14, 0.0, -0.14],
        haze: [-0.14, 0.0, -0.14],
        sky_horizon: [-0.08, 0.0, -0.08],
        density: 1.8,
        falloff: 1.5,
        saturation: 0.92,
        sun: [0.96, 1.0, 0.92],
        light_tint: [0.98, 1.0, 0.96],
        ..LandAir::NEUTRAL
    },
    // Highland: cold, clear air that shows the far hills.
    LandAir {
        fog: [-0.10, -0.02, 0.08],
        haze: [-0.20, -0.06, 0.14],
        sky_horizon: [-0.12, -0.04, 0.12],
        sky_top: [-0.04, -0.02, 0.04],
        density: 0.55,
        saturation: 0.92,
        contrast: 1.05,
        sun: [0.95, 0.98, 1.05],
        shadow_tint: [0.96, 0.99, 1.06],
        light_tint: [0.97, 1.0, 1.04],
        ..LandAir::NEUTRAL
    },
    // Waste: a dusty amber haze.
    LandAir {
        fog: [0.12, 0.04, -0.12],
        haze: [0.04, -0.02, -0.12],
        sky_horizon: [0.03, -0.03, -0.14],
        density: 1.35,
        saturation: 0.9,
        contrast: 1.04,
        sun: [1.05, 0.98, 0.88],
        shadow_tint: [1.03, 1.0, 0.94],
        light_tint: [1.04, 1.0, 0.94],
        ..LandAir::NEUTRAL
    },
    // Valley: a river's slight mist.
    LandAir {
        fog: [-0.04, 0.02, -0.04],
        density: 1.2,
        ..LandAir::NEUTRAL
    },
];

/// Samples a side of the square laid over the camera's surroundings.
const SAMPLES: i32 = 9;
/// How far round the camera the lands count, as a share of its distance
/// from the ground, and the least and most in tiles.
const REACH: f32 = 0.6;
const REACH_MIN: f32 = 20.0;
const REACH_MAX: f32 = 70.0;
/// Seconds the air takes to cross most of the way to a new land's.
const SETTLE: f32 = 2.5;
/// How many steps a knob moves with Shift held, as in the sandbox's tuner.
const FAST: f32 = 10.0;
/// Seconds for weather to settle in (or clear), as a time constant.
const WEATHER_SETTLE: f32 = 3.0;
/// How much of its weather a look keeps under Clarity.
const CLARITY_WEATHER: f32 = 0.35;

/// The light of a match.
pub struct Atmosphere {
    preset: LookPreset,
    /// A look every match opens with (`--look`), in place of its own.
    look_flag: Option<LookPreset>,
    /// How much of the camera's surroundings each land fills, eased; `None`
    /// until the first frame of a match, which starts there.
    shares: Option<[f32; Land::ALL.len()]>,
    /// A land whose air is used everywhere (`--air`, or L in the tuning
    /// panel), to see or tune one land's look wherever the camera is.
    forced: Option<Land>,
    /// The weather over the match, and how much of it has come in (eased,
    /// 0 to 1).
    weather: Weather,
    weather_flag: Option<Weather>,
    fade: f32,
    /// The weather that was here before the last change, and how much of
    /// it is left, so one weather clears as the next comes in.
    leaving: (Weather, f32),
    tuning: Tuning,
}

/// The tuning panel's state.
#[derive(Default)]
struct Tuning {
    open: bool,
    selected: usize,
    /// How far each knob has been moved from the look underneath it.
    deltas: [f32; KNOBS.len()],
    /// The look the knobs act on (the land's air and Clarity in), and the
    /// look as last drawn, knobs and all, for the panel to read.
    under: Look,
    shown: Look,
}

impl Atmosphere {
    /// `look` lights every match in place of its own (`--look`), and
    /// `air` is one land's air everywhere (`--air`).
    pub fn new(look: Option<LookPreset>, air: Option<Land>) -> Atmosphere {
        Atmosphere {
            preset: look.unwrap_or_default(),
            look_flag: look,
            shares: None,
            forced: air,
            weather: Weather::Clear,
            weather_flag: None,
            fade: 0.0,
            leaving: (Weather::Clear, 0.0),
            tuning: Tuning::default(),
        }
    }

    /// Every match opens in this weather (`--weather`).
    pub fn with_weather(mut self, weather: Option<Weather>) -> Atmosphere {
        self.weather_flag = weather;
        self.start_weather();
        self
    }

    fn start_weather(&mut self) {
        self.weather = self.weather_flag.unwrap_or_default();
        self.fade = if self.weather == Weather::Clear {
            0.0
        } else {
            1.0
        };
    }

    /// F12: the next weather; it comes in over a few seconds.
    pub fn next_weather(&mut self) {
        self.leaving = (self.weather, self.fade);
        self.weather = self.weather.next();
        self.fade = 0.0;
    }

    /// The weather and how much of it there is now.
    pub fn weather(&self) -> (Weather, f32) {
        (self.weather, self.fade)
    }

    /// The weather that is clearing as the new one comes in, with how much
    /// of it is left (clear, and nothing, when none is).
    pub fn leaving(&self) -> (Weather, f32) {
        self.leaving
    }

    /// Light a new match with `preset` (unless `--look` said otherwise),
    /// and let the air start right in the lands where the camera is.
    pub fn start(&mut self, preset: LookPreset) {
        self.preset = self.look_flag.unwrap_or(preset);
        self.shares = None;
        self.start_weather();
    }

    /// F9: the next prepared look.
    pub fn next_preset(&mut self) {
        self.preset = self.preset.next();
    }

    /// The look to draw this frame: the camera at `focus`, `distance` from
    /// the ground, over `map`; `clarity` as the settings say.
    pub fn look(&mut self, dt: f32, map: &Map, focus: Vec2, distance: f32, clarity: bool) -> Look {
        let target = match self.forced {
            Some(land) => {
                let mut one = [0.0; Land::ALL.len()];
                one[land as usize] = 1.0;
                one
            }
            None => land_shares(map, focus, distance),
        };
        let shares = match &mut self.shares {
            Some(now) => {
                let k = 1.0 - (-dt.max(0.0) / SETTLE).exp();
                for (n, t) in now.iter_mut().zip(target) {
                    *n += (t - *n) * k;
                }
                *now
            }
            none => *none.insert(target),
        };
        let k = 1.0 - (-dt.max(0.0) / WEATHER_SETTLE).exp();
        if self.weather != Weather::Clear {
            self.fade += (1.0 - self.fade) * k;
        }
        self.leaving.1 *= 1.0 - k;
        if self.leaving.1 < 0.01 {
            self.leaving = (Weather::Clear, 0.0);
        }
        // Under Clarity the weather dims the light and thickens the air
        // only a little, so the field stays readable whatever the sky does.
        let share = if clarity { CLARITY_WEATHER } else { 1.0 };
        let mut look = self
            .preset
            .look()
            .in_air(&LandAir::mixed(&AIRS, &shares))
            .in_weather(self.leaving.0, self.leaving.1 * share)
            .in_weather(self.weather, self.fade * share);
        if clarity {
            look = look.clear();
        }
        self.tuning.under = look;
        self.tuning.shown = look.tuned(&self.tuning.deltas);
        self.tuning.shown
    }

    pub fn panel_open(&self) -> bool {
        self.tuning.open
    }

    pub fn toggle_panel(&mut self) {
        self.tuning.open = !self.tuning.open;
    }

    /// A key of the tuning panel (`fast` is Shift).
    pub fn tune_key(&mut self, key: TuneKey, fast: bool) {
        let t = &mut self.tuning;
        match key {
            TuneKey::Up => t.selected = (t.selected + KNOBS.len() - 1) % KNOBS.len(),
            TuneKey::Down => t.selected = (t.selected + 1) % KNOBS.len(),
            TuneKey::Less | TuneKey::More => {
                let sign = if key == TuneKey::More { 1.0 } else { -1.0 };
                let knob = &KNOBS[t.selected];
                let step = knob.step * if fast { FAST } else { 1.0 };
                // The knob's own limit decides how far it goes, so the
                // panel shows what is on screen and a press past a limit
                // does not pile up unseen.
                let mut moved = t.under;
                (knob.set)(
                    &mut moved,
                    (knob.get)(&t.under) + t.deltas[t.selected] + sign * step,
                );
                t.deltas[t.selected] = (knob.get)(&moved) - (knob.get)(&t.under);
            }
            TuneKey::Land => {
                // None, then each land in turn, then None again.
                let next = self.forced.map_or(0, |l| l as usize + 1);
                self.forced = Land::ALL.get(next).copied();
            }
            TuneKey::Reset if fast => t.deltas = [0.0; KNOBS.len()],
            TuneKey::Reset => t.deltas[t.selected] = 0.0,
        }
    }

    /// The panel's text: every knob with its value now and how far it has
    /// been moved.
    pub fn panel_text(&self) -> String {
        let t = &self.tuning;
        let mut text = format!(
            "LIGHT TUNING                                   F10 closes\n\
             {} light{}, {}{}\n",
            self.preset.name(),
            self.weather_note(),
            self.forced
                .map_or("the air of the land round the camera".to_string(), |l| {
                    format!("{l:?} air everywhere")
                }),
            if t.deltas.iter().any(|&d| d != 0.0) {
                ", moved by hand"
            } else {
                ""
            },
        );
        for (i, (knob, value, d)) in self.knobs().enumerate() {
            let mark = if i == t.selected { '>' } else { ' ' };
            let moved = if d == 0.0 {
                String::new()
            } else {
                format!("  ({d:+.4})")
            };
            text.push_str(&format!("{mark} {:<18} {value:>8.4}{moved}\n", knob.name));
        }
        text.push_str(
            "Up Down choose   Left Right change (Shift: 10x)   L force a land's air\n\
             Backspace reset (Shift: all)   Enter write values to the log",
        );
        text
    }

    /// The tuned values, one line each, for the log: what to tell whoever
    /// is making the look, with how far each was moved by hand.
    pub fn values(&self) -> String {
        let air = self
            .forced
            .map_or("the lands round the camera".to_string(), |l| {
                format!("{l:?}")
            });
        let mut lines = vec![format!(
            "light tuning on {}{} in {air}'s air (air, weather and Clarity included):",
            self.preset.name(),
            self.weather_note()
        )];
        for (knob, value, d) in self.knobs() {
            lines.push(format!("  {:<18} {value:.4}  (moved {d:+.4})", knob.name));
        }
        lines.join("\n")
    }

    /// ", in rain" (or snow, fog), with how much has come in, or nothing
    /// in clear weather, for the panel and the log: values tuned in weather
    /// are not the plain look's.
    fn weather_note(&self) -> String {
        match self.weather {
            Weather::Clear => String::new(),
            w => format!(
                ", in {} ({:.0}%)",
                w.name().to_lowercase(),
                self.fade * 100.0
            ),
        }
    }

    /// Every knob, its value on screen and how far it has been moved.
    fn knobs(&self) -> impl Iterator<Item = (&'static Knob, f32, f32)> + '_ {
        let t = &self.tuning;
        KNOBS
            .iter()
            .zip(t.deltas)
            .map(|(knob, d)| (knob, (knob.get)(&t.shown), d))
    }
}

/// The keys the tuning panel answers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuneKey {
    Up,
    Down,
    Less,
    More,
    Reset,
    Land,
}

/// How much of the ground round `focus` each land fills, as shares adding
/// up to 1 (a map without lands is all meadow): a square of samples laid
/// over the camera's surroundings, the middle counting most.
fn land_shares(map: &Map, focus: Vec2, distance: f32) -> [f32; Land::ALL.len()] {
    let mut shares = [0.0; Land::ALL.len()];
    let reach = (distance * REACH).clamp(REACH_MIN, REACH_MAX);
    let last = (map.tiles as i32 - 1).max(0);
    let mut total = 0.0;
    for j in 0..SAMPLES {
        for i in 0..SAMPLES {
            let (u, v) = (
                (i as f32 + 0.5) / SAMPLES as f32 * 2.0 - 1.0,
                (j as f32 + 0.5) / SAMPLES as f32 * 2.0 - 1.0,
            );
            let weight = 1.0 - (u * u + v * v) / 2.0;
            let p = focus + Vec2::new(u, v) * reach;
            let tile = (p.y.floor() as i32).clamp(0, last) * map.tiles as i32
                + (p.x.floor() as i32).clamp(0, last);
            let land = map.land_at(tile as usize).unwrap_or_default();
            shares[land as usize] += weight;
            total += weight;
        }
    }
    shares.map(|s| s / total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64-tile map whose west half is `west` and east half `east`.
    fn split(west: Land, east: Land) -> Map {
        let mut map = Map::generate(1, 64);
        map.land = (0..64 * 64)
            .map(|i| if i % 64 < 32 { west } else { east } as u8)
            .collect();
        map
    }

    #[test]
    fn the_air_follows_the_land_under_the_camera() {
        let map = split(Land::Waste, Land::Fen);
        let near = |x: f32| land_shares(&map, Vec2::new(x, 32.0), 20.0);
        let (west, east) = (near(8.0), near(56.0));
        assert!(west[Land::Waste as usize] > 0.9, "{west:?}");
        assert!(east[Land::Fen as usize] > 0.9, "{east:?}");
        let mid = near(32.0);
        assert!(mid[Land::Waste as usize] > 0.2 && mid[Land::Fen as usize] > 0.2);
        assert!((mid.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_map_without_lands_is_a_meadow() {
        let mut map = Map::generate(1, 64);
        map.land.clear();
        let shares = land_shares(&map, Vec2::new(10.0, 10.0), 100.0);
        assert_eq!(shares[Land::Meadow as usize], 1.0);
    }

    #[test]
    fn the_waste_is_amber_the_fen_green_and_the_highland_cold() {
        let base = LookPreset::GoldenHour.look();
        let haze_in = |land: Land| base.in_air(&AIRS[land as usize]).haze;
        let (waste, fen, high) = (
            haze_in(Land::Waste),
            haze_in(Land::Fen),
            haze_in(Land::Highland),
        );
        // Amber: red well over blue. Green mist: green over red and blue.
        // Cold: blue over red.
        assert!(waste[0] - waste[2] > 0.35, "{waste:?}");
        assert!(fen[1] > fen[0] && fen[1] > fen[2], "{fen:?}");
        assert!(high[2] > high[0], "{high:?}");
        // The Fen's air is thickest and the Highland's clearest.
        let density = |land: Land| base.in_air(&AIRS[land as usize]).fog_density;
        assert!(density(Land::Fen) > density(Land::Waste));
        assert!(density(Land::Highland) < density(Land::Meadow));
    }

    #[test]
    fn the_look_settles_toward_a_new_land_and_starts_there() {
        let map = split(Land::Meadow, Land::Waste);
        let mut atmosphere = Atmosphere::new(None, None);
        let west = atmosphere.look(0.0, &map, Vec2::new(8.0, 32.0), 20.0, false);
        let at_once = atmosphere.look(0.0, &map, Vec2::new(56.0, 32.0), 20.0, false);
        assert_eq!(west, at_once, "no time has passed");
        let soon = atmosphere.look(1.0, &map, Vec2::new(56.0, 32.0), 20.0, false);
        let later = atmosphere.look(30.0, &map, Vec2::new(56.0, 32.0), 20.0, false);
        // Warmer and warmer haze.
        assert!(soon.haze[2] < west.haze[2] && later.haze[2] < soon.haze[2]);
        // A fresh match starts in the Waste's air at once.
        atmosphere.start(LookPreset::GoldenHour);
        let fresh = atmosphere.look(0.0, &map, Vec2::new(56.0, 32.0), 20.0, false);
        assert!((fresh.haze[2] - later.haze[2]).abs() < 0.02);
    }

    #[test]
    fn weather_comes_in_over_a_few_seconds_and_a_new_match_starts_clear() {
        let map = split(Land::Meadow, Land::Meadow);
        let mut atmosphere = Atmosphere::new(None, None);
        let clear = atmosphere.look(0.0, &map, Vec2::new(10.0, 10.0), 40.0, false);
        atmosphere.next_weather();
        assert_eq!(atmosphere.weather(), (Weather::Rain, 0.0));
        let mut last = clear.fog_density;
        for _ in 0..4 {
            let look = atmosphere.look(1.0, &map, Vec2::new(10.0, 10.0), 40.0, false);
            assert!(look.fog_density > last, "the air thickens each second");
            last = look.fog_density;
        }
        let (weather, amount) = atmosphere.weather();
        assert_eq!(weather, Weather::Rain);
        assert!(amount > 0.7 && amount < 1.0, "{amount}");
        atmosphere.start(LookPreset::GoldenHour);
        assert_eq!(atmosphere.weather(), (Weather::Clear, 0.0));
    }

    #[test]
    fn one_weather_clears_as_the_next_comes_in_without_a_jump() {
        let map = split(Land::Meadow, Land::Meadow);
        let at = Vec2::new(10.0, 10.0);
        let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(Weather::Rain));
        let rain = atmosphere.look(0.0, &map, at, 40.0, false);
        atmosphere.next_weather();
        assert_eq!(atmosphere.leaving(), (Weather::Rain, 1.0));
        // The frame the weather changes looks the same as the one before.
        let first = atmosphere.look(0.0, &map, at, 40.0, false);
        assert!((first.sun_color[0] - rain.sun_color[0]).abs() < 1e-4);
        assert!((first.fog_density - rain.fog_density).abs() < 1e-6);
        // The sun never brightens past the dimmer of the two on the way.
        let snow_sun = rain.in_weather(Weather::Snow, 0.0).sun_color[0];
        for _ in 0..30 {
            let look = atmosphere.look(0.5, &map, at, 40.0, false);
            assert!(look.sun_color[0] <= snow_sun.max(rain.sun_color[0] * 2.0));
        }
        assert_eq!(atmosphere.leaving(), (Weather::Clear, 0.0), "rain is gone");
        // Going to clear eases out too, not in one frame.
        let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(Weather::Fog));
        let fog = atmosphere.look(0.0, &map, at, 40.0, false);
        atmosphere.next_weather();
        assert_eq!(atmosphere.weather().0, Weather::Clear);
        let one_frame = atmosphere.look(0.016, &map, at, 40.0, false);
        let clear = Atmosphere::new(None, None).look(0.0, &map, at, 40.0, false);
        assert!(
            one_frame.fog_density > clear.fog_density,
            "{} {}",
            one_frame.fog_density,
            clear.fog_density
        );
        assert!(one_frame.fog_density <= fog.fog_density);
    }

    #[test]
    fn the_panel_and_the_log_name_the_weather() {
        let mut atmosphere = Atmosphere::new(None, None);
        assert!(!atmosphere.panel_text().contains("in rain"));
        atmosphere = atmosphere.with_weather(Some(Weather::Rain));
        assert!(atmosphere.panel_text().contains("in rain (100%)"));
        assert!(atmosphere.values().contains("in rain (100%)"));
    }

    #[test]
    fn a_weather_flag_opens_every_match_in_full_weather() {
        let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(Weather::Snow));
        assert_eq!(atmosphere.weather(), (Weather::Snow, 1.0));
        atmosphere.next_weather();
        atmosphere.start(LookPreset::Noon);
        assert_eq!(atmosphere.weather(), (Weather::Snow, 1.0));
    }

    #[test]
    fn weather_cycles_back_to_clear_and_clarity_keeps_it() {
        let map = split(Land::Meadow, Land::Meadow);
        let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(Weather::Fog));
        let foggy = atmosphere.look(1.0, &map, Vec2::new(10.0, 10.0), 40.0, true);
        let clear_sky =
            Atmosphere::new(None, None).look(1.0, &map, Vec2::new(10.0, 10.0), 40.0, true);
        assert!(foggy.fog_density > clear_sky.fog_density * 1.5);
        assert!(
            foggy.fog_density < clear_sky.fog_density * 3.5,
            "Clarity thins the weather"
        );
        assert_eq!(foggy.vignette, 0.0, "Clarity still plain");
        for _ in 0..Weather::ALL.len() {
            atmosphere.next_weather();
        }
        assert_eq!(atmosphere.weather().0, Weather::Fog);
    }

    #[test]
    fn clarity_is_plain_light_in_every_land() {
        let map = split(Land::Highland, Land::Waste);
        for preset in LookPreset::ALL {
            let mut atmosphere = Atmosphere::new(Some(preset), None);
            let look = atmosphere.look(0.0, &map, Vec2::new(8.0, 32.0), 40.0, true);
            assert_eq!((look.vignette, look.grain), (0.0, 0.0), "{}", preset.name());
        }
    }

    #[test]
    fn a_knob_stops_at_its_limit_and_the_panel_shows_what_is_on_screen() {
        let map = split(Land::Meadow, Land::Meadow);
        let mut atmosphere = Atmosphere::new(None, None);
        let focus = Vec2::new(30.0, 30.0);
        atmosphere.look(0.0, &map, focus, 40.0, false);
        // Sun height is the first knob; Shift+Left many times.
        for _ in 0..30 {
            atmosphere.tune_key(TuneKey::Less, true);
            atmosphere.look(0.0, &map, focus, 40.0, false);
        }
        assert!(
            atmosphere.panel_text().contains(" 2.0000"),
            "{}",
            atmosphere.panel_text()
        );
        // One press back up moves it at once, not after a pile of presses.
        atmosphere.tune_key(TuneKey::More, false);
        let up = atmosphere.look(0.0, &map, focus, 40.0, false);
        assert!(up.sun_dir().y > 0.05);
    }

    #[test]
    fn the_panel_moves_one_knob_and_resets() {
        let map = split(Land::Meadow, Land::Meadow);
        let mut atmosphere = Atmosphere::new(None, None);
        let focus = Vec2::new(30.0, 30.0);
        let plain = atmosphere.look(0.0, &map, focus, 40.0, false);
        // Down to Sun heading (second), then four steps up, one fast.
        atmosphere.tune_key(TuneKey::Down, false);
        for fast in [false, false, false, true] {
            atmosphere.tune_key(TuneKey::More, fast);
        }
        let turned = atmosphere.look(0.0, &map, focus, 40.0, false);
        assert_ne!(turned.sun, plain.sun);
        assert_eq!(turned.exposure, plain.exposure);
        assert!(atmosphere.panel_text().contains("moved by hand"));
        atmosphere.tune_key(TuneKey::Reset, false);
        assert_eq!(atmosphere.look(0.0, &map, focus, 40.0, false), plain);
        // Shift+Backspace puts everything back.
        atmosphere.tune_key(TuneKey::More, false);
        atmosphere.tune_key(TuneKey::Up, false);
        atmosphere.tune_key(TuneKey::More, false);
        atmosphere.tune_key(TuneKey::Reset, true);
        assert_eq!(atmosphere.look(0.0, &map, focus, 40.0, false), plain);
        assert_eq!(atmosphere.panel_text().lines().count(), KNOBS.len() + 4);
    }
}
