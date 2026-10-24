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

/// How much of the wet ground's share adds to the mist, and the hour a
/// sky with no clock has.
const MIST_AFTER_RAIN: f32 = 0.5;
const MIST_STILL_HOUR: f32 = 17.0;

/// The mist on the water at an hour of the day (0 to 24), 0 to 1: thick at
/// dawn, gone by mid-morning and through the noon, rising again toward
/// dusk and lying thinly through the night.
fn mist_at(hour: f32) -> f32 {
    const CURVE: [(f32, f32); 9] = [
        (0.0, 0.3),
        (4.0, 0.7),
        (6.0, 1.0),
        (8.5, 0.5),
        (10.5, 0.0),
        (14.5, 0.0),
        (17.5, 0.35),
        (20.0, 0.3),
        (24.0, 0.3),
    ];
    let hour = hour.rem_euclid(24.0);
    CURVE.windows(2).find(|w| hour <= w[1].0).map_or(0.5, |w| {
        let k = (hour - w[0].0) / (w[1].0 - w[0].0);
        w[0].1 + (w[1].1 - w[0].1) * k
    })
}

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
/// Seconds the ground takes to soak through in full rain, and to dry again
/// (time constants): rain wets it quickly and it dries over minutes.
const SOAK_SETTLE: f32 = 12.0;
const DRY_SETTLE: f32 = 90.0;
/// How little wet is dry.
const DRY_BELOW: f32 = 0.002;
/// Seconds snow takes to settle as a time constant, to melt on its own, and
/// to melt under rain.
const SNOW_SETTLE: f32 = 50.0;
/// Under this much snow the ground is bare to the eye, so it is none.
const SNOW_BARE: f32 = 0.06;
const SNOW_MELT: f32 = 240.0;
const RAIN_MELT: f32 = 25.0;
/// How much of the full rain it takes to storm, and the seconds between
/// flashes (shortest, longest).
const STORM_AT: f32 = 0.75;
const STRIKE_GAP: (f32, f32) = (5.0, 16.0);
/// The wind of a weather chosen by hand (F12, `--weather`), tiles a second.
const PREVIEW_WIND: Vec2 = Vec2::new(0.64, 0.32);

/// The sky the simulation says there is over the match (docs/32), for the
/// picture to follow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyView {
    pub weather: Weather,
    /// How much of the weather there is, 0 to 1 (the simulation's strength
    /// over 255). It already eases in and out.
    pub amount: f32,
    /// Cloud cover, 0 a clear sky to 1 overcast.
    pub cloud: f32,
    /// Which way and how hard the wind blows, in tiles a second along x and
    /// z (the simulation's sixteenths).
    pub wind: Vec2,
    /// The hour of the day, 0 to 24.
    pub hour: f32,
    /// How light the day is, 0 night to 1 day.
    pub light: f32,
}

impl SkyView {
    /// The sky the simulation's `world` has now.
    pub fn of(world: &gc_sim::World) -> SkyView {
        SkyView::at(world.weather(), world.daylight())
    }

    /// The sky of this `weather` at this time of day.
    pub fn at(weather: gc_sim::Weather, daylight: gc_sim::Daylight) -> SkyView {
        use gc_sim::WeatherKind;
        let kind = match weather.kind {
            WeatherKind::Clear => Weather::Clear,
            WeatherKind::Rain => Weather::Rain,
            WeatherKind::Snow => Weather::Snow,
            WeatherKind::Fog => Weather::Fog,
        };
        let day = daylight.day_ticks.max(1);
        SkyView {
            weather: kind,
            amount: f32::from(weather.strength) / 255.0,
            cloud: f32::from(weather.cloud) / 255.0,
            wind: Vec2::new(f32::from(weather.wind.0), f32::from(weather.wind.1)) / 16.0,
            hour: (daylight.tick_of_day % day) as f32 / day as f32 * 24.0,
            light: f32::from(daylight.light()) / 1000.0,
        }
    }
}

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
    /// How wet the ground is (0 to 1), which outlasts the rain.
    soaked: f32,
    /// How much snow lies on the ground (0 to 1), which outlasts the fall.
    settled: f32,
    storm: Storm,
    /// The sky the match has now, which the weather and the light follow
    /// until F12 (weather) or F9 (look) takes them over, or `--weather` or
    /// `--look` has.
    sky: Option<SkyView>,
    weather_preview: bool,
    look_preview: bool,
    tuning: Tuning,
}

/// The lightning of a heavy rain: seconds to the next flash, how long the
/// last has been going, and a random state (the picture's own, not the
/// simulation's, so it needs no seed from the match).
struct Storm {
    next: f32,
    age: Option<f32>,
    /// A flash began since the picture last asked.
    struck: bool,
    rng: u32,
}

impl Storm {
    fn new() -> Storm {
        let mut storm = Storm {
            next: 0.0,
            age: None,
            struck: false,
            rng: 0x9E37_79B9,
        };
        storm.next = storm.gap();
        storm
    }

    /// Seconds to the next flash.
    fn gap(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        let unit = (self.rng >> 8) as f32 / (1u32 << 24) as f32;
        STRIKE_GAP.0 + (STRIKE_GAP.1 - STRIKE_GAP.0) * unit
    }

    /// Move on `dt` seconds, `stormy` or not, and say how bright the sky
    /// is with lightning now (0 none, 1 the brightest). A flash that has
    /// begun plays out whatever the weather does.
    fn step(&mut self, dt: f32, stormy: bool) -> f32 {
        if stormy {
            self.next -= dt;
            if self.next <= 0.0 {
                self.age = Some(0.0);
                self.struck = true;
                self.next = self.gap();
            }
        }
        let Some(age) = self.age.as_mut() else {
            return 0.0;
        };
        *age += dt;
        let level = bolt(*age);
        if *age > BOLT_LENGTH {
            self.age = None;
        }
        level
    }
}

/// Seconds a flash lasts, a sharp stroke and a dimmer one after it.
const BOLT_LENGTH: f32 = 1.0;

/// The sky's brightness `age` seconds into a flash.
fn bolt(age: f32) -> f32 {
    let stroke = |t: f32, peak: f32| {
        if t < 0.0 {
            0.0
        } else if t < 0.04 {
            peak * t / 0.04
        } else {
            peak * (-(t - 0.04) / 0.12).exp()
        }
    };
    stroke(age, 1.0).max(stroke(age - 0.22, 0.6))
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
            soaked: 0.0,
            settled: 0.0,
            storm: Storm::new(),
            sky: None,
            weather_preview: false,
            look_preview: false,
            tuning: Tuning::default(),
        }
    }

    /// Whether the light follows the match's clock (not `--look` or F9).
    fn following_day(&self) -> bool {
        self.look_flag.is_none() && !self.look_preview
    }

    /// What lights the picture, for the panel and the log: the look's name,
    /// or the clock's hour while it follows the match's day.
    fn light_name(&self) -> String {
        match self.sky {
            Some(sky) if self.following_day() => {
                let minutes = (sky.hour * 60.0).floor() as u32 % (24 * 60);
                format!("{:02}:{:02}", minutes / 60, minutes % 60)
            }
            _ => self.preset.name().to_string(),
        }
    }

    /// The sky the match has this frame: the weather and the light follow
    /// it, unless a flag or F12 or F9 chose their own.
    pub fn follow(&mut self, sky: SkyView) {
        self.sky = Some(sky);
    }

    /// Every match opens in this weather (`--weather`).
    pub fn with_weather(mut self, weather: Option<Weather>) -> Atmosphere {
        self.weather_flag = weather;
        self.start_weather();
        self
    }

    fn start_weather(&mut self) {
        self.weather_preview = self.weather_flag.is_some();
        self.weather = self.weather_flag.unwrap_or_default();
        self.fade = if self.weather == Weather::Clear {
            0.0
        } else {
            1.0
        };
    }

    /// F12: the next weather; it comes in over a few seconds.
    pub fn next_weather(&mut self) {
        self.weather_preview = true;
        self.leaving = (self.weather, self.fade);
        self.weather = self.weather.next();
        self.fade = 0.0;
    }

    /// The weather and how much of it there is now.
    pub fn weather(&self) -> (Weather, f32) {
        (self.weather, self.fade)
    }

    /// The wind, in tiles a second along x and z: the simulation's while the
    /// weather follows it, else the slant rain has always had.
    pub fn wind(&self) -> Vec2 {
        match self.sky {
            Some(sky) if !self.weather_preview => sky.wind,
            _ => PREVIEW_WIND,
        }
    }

    /// How thick the mist on the water is, 0 to 1: it follows the clock (see
    /// `mist_at`) and the ground's wetness, and is none in fog (the whole
    /// air is thick already). A fair sky with no clock is the usual 17:00.
    pub fn mist(&self) -> f32 {
        // The clock's hour while the light follows it, else the hour the
        // chosen look stands for.
        let hour = match self.sky {
            Some(sky) if self.following_day() => sky.hour,
            Some(_) => match self.preset {
                LookPreset::Noon | LookPreset::Overcast | LookPreset::Storm => 12.0,
                LookPreset::GoldenHour => 17.0,
                LookPreset::Dusk => 18.0,
                LookPreset::Night => 23.0,
            },
            None => MIST_STILL_HOUR,
        };
        // A fog, coming in or going out, has air thick enough already.
        let fog = [
            self.weather_share(Weather::Fog),
            self.leaving_share(Weather::Fog),
        ]
        .into_iter()
        .fold(0.0, f32::max);
        (mist_at(hour) + self.soaked * MIST_AFTER_RAIN).clamp(0.0, 1.0) * (1.0 - fog)
    }

    fn weather_share(&self, kind: Weather) -> f32 {
        if self.weather == kind { self.fade } else { 0.0 }
    }

    fn leaving_share(&self, kind: Weather) -> f32 {
        if self.leaving.0 == kind {
            self.leaving.1
        } else {
            0.0
        }
    }

    /// Whether lightning began since this was last called, for the bolt to
    /// be drawn (and thunder, when there is some) with the flash.
    pub fn take_strike(&mut self) -> bool {
        std::mem::take(&mut self.storm.struck)
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
        self.look_preview = false;
        self.sky = None;
        self.shares = None;
        self.start_weather();
        self.leaving = (Weather::Clear, 0.0);
        self.soaked = 0.0;
        self.settled = 0.0;
        self.storm = Storm::new();
    }

    /// F9: the next prepared look.
    pub fn next_preset(&mut self) {
        self.look_preview = true;
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
        if let Some(sky) = self.sky.filter(|_| !self.weather_preview) {
            // The simulation eases its weather in and out itself.
            self.weather = sky.weather;
            self.fade = sky.amount.clamp(0.0, 1.0);
            self.leaving = (Weather::Clear, 0.0);
        } else {
            let k = 1.0 - (-dt.max(0.0) / WEATHER_SETTLE).exp();
            if self.weather != Weather::Clear {
                self.fade += (1.0 - self.fade) * k;
            }
            self.leaving.1 *= 1.0 - k;
            if self.leaving.1 < 0.01 {
                self.leaving = (Weather::Clear, 0.0);
            }
        }
        // The ground soaks up the rain that falls, and keeps it a while.
        let rain = [self.leaving, (self.weather, self.fade)]
            .into_iter()
            .filter(|&(weather, _)| weather == Weather::Rain)
            .map(|(_, amount)| amount)
            .fold(0.0, f32::max);
        let settle = if rain > self.soaked {
            SOAK_SETTLE
        } else {
            DRY_SETTLE
        };
        self.soaked += (rain - self.soaked) * (1.0 - (-dt.max(0.0) / settle).exp());
        // Dry, not a trace of wet for ever.
        if rain == 0.0 && self.soaked < DRY_BELOW {
            self.soaked = 0.0;
        }
        // Snow settles while it falls and stays while it eases, melts slowly
        // once it has stopped and quickly under rain.
        let snowfall = [self.leaving, (self.weather, self.fade)]
            .into_iter()
            .filter(|&(weather, _)| weather == Weather::Snow)
            .map(|(_, amount)| amount)
            .fold(0.0, f32::max);
        let (target, settle) = if rain > 0.0 {
            (0.0, RAIN_MELT)
        } else if snowfall > self.settled {
            (snowfall, SNOW_SETTLE)
        } else if snowfall > 0.0 {
            (self.settled, SNOW_SETTLE)
        } else {
            (0.0, SNOW_MELT)
        };
        self.settled += (target - self.settled) * (1.0 - (-dt.max(0.0) / settle).exp());
        if snowfall == 0.0 && self.settled < SNOW_BARE {
            self.settled = 0.0;
        }
        let stormy = self.weather == Weather::Rain && self.fade >= STORM_AT;
        let flash = self.storm.step(dt.max(0.0), stormy);
        // Under Clarity the weather dims the light and thickens the air
        // only a little, so the field stays readable whatever the sky does.
        let share = if clarity { CLARITY_WEATHER } else { 1.0 };
        // The light of the hour the match's clock says (unless a look was
        // chosen), under the cloud the weather leaves over.
        // A look or a weather chosen by hand is the whole picture: it takes
        // the simulation's cloud with it.
        let (day, cloud, light) = match self.sky {
            Some(sky) if self.following_day() => (Look::of_day(sky.hour), sky.cloud, sky.light),
            _ => (self.preset.look(), 0.0, 1.0),
        };
        let cloud = if self.weather_preview { 0.0 } else { cloud };
        // Weather that falls carries its own cloud: the one clearing counts too.
        let weather_left = 1.0
            - [(self.weather, self.fade), self.leaving]
                .into_iter()
                .filter(|&(weather, _)| weather != Weather::Clear)
                .map(|(_, amount)| amount)
                .fold(0.0, f32::max);
        let mut look = day
            .in_air(&LandAir::mixed(&AIRS, &shares))
            .clouded(cloud * weather_left * share, light)
            .in_weather(self.leaving.0, self.leaving.1 * share)
            .in_weather(self.weather, self.fade * share);
        look.wet = self.soaked * share;
        look.snow = self.settled * share;
        if clarity {
            look = look.clear();
        }
        self.tuning.under = look;
        self.tuning.shown = look.tuned(&self.tuning.deltas);
        // The flash is on the picture only, not the values the panel shows.
        self.tuning.shown.flashed(flash * share)
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
            self.light_name(),
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
            self.light_name(),
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

    fn run(atmosphere: &mut Atmosphere, seconds: f32) -> Look {
        let map = Map::generate(1, 64);
        let mut look = Look::default();
        for _ in 0..(seconds * 10.0) as usize {
            look = atmosphere.look(0.1, &map, Vec2::new(32.0, 32.0), 40.0, false);
        }
        look
    }

    fn sim_sky(kind: gc_sim::WeatherKind, strength: u8, cloud: u8, hour: u32) -> SkyView {
        SkyView::at(
            gc_sim::Weather {
                kind,
                strength,
                cloud,
                wind: (0, 0),
            },
            gc_sim::Daylight {
                tick_of_day: gc_sim::sky::at_hour(hour, 0),
                day_ticks: gc_sim::sky::DAY_TICKS,
            },
        )
    }

    /// One frame of `atmosphere` over the 64-tile test map.
    fn frame(atmosphere: &mut Atmosphere, dt: f32) -> Look {
        frame_with(atmosphere, dt, false)
    }

    fn frame_with(atmosphere: &mut Atmosphere, dt: f32, clarity: bool) -> Look {
        static MAP: std::sync::OnceLock<Map> = std::sync::OnceLock::new();
        let map = MAP.get_or_init(|| Map::generate(1, 64));
        atmosphere.look(dt, map, Vec2::new(32.0, 32.0), 40.0, clarity)
    }

    #[test]
    fn the_sky_view_reads_the_simulations_weather_and_clock() {
        use gc_sim::WeatherKind;
        let view = sim_sky(WeatherKind::Rain, 255, 51, 17);
        assert_eq!(view.weather, Weather::Rain);
        assert_eq!(view.amount, 1.0);
        assert!((view.cloud - 0.2).abs() < 1e-3);
        assert!((view.hour - 17.0).abs() < 0.01, "{}", view.hour);
        assert!(view.light > 0.9);
        let night = sim_sky(WeatherKind::Fog, 0, 0, 23);
        assert_eq!(
            (night.weather, night.amount, night.light),
            (Weather::Fog, 0.0, 0.0)
        );
        assert_eq!(sim_sky(WeatherKind::Snow, 9, 0, 3).weather, Weather::Snow);
        assert_eq!(
            sim_sky(WeatherKind::Clear, 0, 0, 12).weather,
            Weather::Clear
        );
    }

    #[test]
    fn the_weather_follows_the_simulation_with_no_easing_of_its_own() {
        use gc_sim::WeatherKind;
        let mut atmosphere = Atmosphere::new(None, None);
        atmosphere.follow(sim_sky(WeatherKind::Rain, 128, 0, 12));
        frame(&mut atmosphere, 0.016);
        let (weather, amount) = atmosphere.weather();
        assert_eq!(weather, Weather::Rain);
        assert!((amount - 128.0 / 255.0).abs() < 1e-3);
        // The simulation fades one weather out before the next grows, so
        // nothing is left behind.
        atmosphere.follow(sim_sky(WeatherKind::Clear, 0, 0, 12));
        frame(&mut atmosphere, 0.016);
        assert_eq!(atmosphere.weather(), (Weather::Clear, 0.0));
        assert_eq!(atmosphere.leaving(), (Weather::Clear, 0.0));
    }

    #[test]
    fn f12_and_the_weather_flag_take_the_weather_over_and_a_new_match_gives_it_back() {
        use gc_sim::WeatherKind;
        let calm = sim_sky(WeatherKind::Clear, 0, 0, 12);
        let mut atmosphere = Atmosphere::new(None, None);
        atmosphere.follow(calm);
        atmosphere.next_weather();
        frame(&mut atmosphere, 0.016);
        assert_eq!(atmosphere.weather().0, Weather::Rain, "the preview stays");
        atmosphere.start(LookPreset::Noon);
        atmosphere.follow(sim_sky(WeatherKind::Snow, 255, 0, 12));
        frame(&mut atmosphere, 0.016);
        assert_eq!(atmosphere.weather(), (Weather::Snow, 1.0));
        let mut flagged = Atmosphere::new(None, None).with_weather(Some(Weather::Fog));
        flagged.follow(calm);
        frame(&mut flagged, 0.016);
        assert_eq!(flagged.weather(), (Weather::Fog, 1.0));
    }

    #[test]
    fn the_light_follows_the_clock_unless_a_look_was_chosen() {
        use gc_sim::WeatherKind;
        let sun = |atmosphere: &mut Atmosphere, hour| {
            atmosphere.follow(sim_sky(WeatherKind::Clear, 0, 0, hour));
            frame(atmosphere, 0.016).sun_color[0]
        };
        let mut atmosphere = Atmosphere::new(None, None);
        let (noon, night) = (sun(&mut atmosphere, 12), sun(&mut atmosphere, 23));
        assert!(noon > 1.5 && night < 0.5, "{noon} {night}");
        // 17:00 is golden hour, the game's usual look.
        let golden = sun(&mut atmosphere, 17);
        assert!((golden - LookPreset::GoldenHour.look().sun_color[0]).abs() < 0.2);
        // `--look` and F9 keep their look whatever the hour.
        let mut flagged = Atmosphere::new(Some(LookPreset::Noon), None);
        assert_eq!(sun(&mut flagged, 23), LookPreset::Noon.look().sun_color[0]);
        atmosphere.next_preset();
        let kept = sun(&mut atmosphere, 12);
        assert_eq!(kept, sun(&mut atmosphere, 23));
        // A new match follows the clock again.
        atmosphere.start(LookPreset::Noon);
        assert!(sun(&mut atmosphere, 23) < 0.5);
    }

    #[test]
    fn a_look_or_a_weather_chosen_by_hand_takes_no_cloud_and_clearing_weather_counts() {
        use gc_sim::WeatherKind;
        let overcast = sim_sky(WeatherKind::Clear, 0, 220, 12);
        let sun = |atmosphere: &mut Atmosphere| {
            atmosphere.follow(overcast);
            frame(atmosphere, 0.016).sun_color[0]
        };
        let plain = |preset| {
            let mut atmosphere = Atmosphere::new(Some(preset), None);
            sun(&mut atmosphere)
        };
        // `--look overcast` is the prepared Overcast, not cloud on top of it.
        assert_eq!(
            plain(LookPreset::Overcast),
            LookPreset::Overcast.look().sun_color[0]
        );
        let mut hand = Atmosphere::new(None, None);
        hand.next_preset();
        assert!(sun(&mut hand) > 1.0, "F9's look is whole");
        // A weather chosen by hand brings no cloud with it.
        let mut clear = Atmosphere::new(None, None).with_weather(Some(Weather::Clear));
        let mut followed = Atmosphere::new(None, None);
        assert!(sun(&mut clear) > sun(&mut followed) * 1.3);
        // F12 from rain: no jump as the cloud would land on the clearing rain.
        let mut raining = Atmosphere::new(None, None);
        raining.follow(sim_sky(WeatherKind::Rain, 200, 230, 12));
        let before = frame(&mut raining, 0.016).sun_color[0];
        raining.next_weather();
        raining.follow(sim_sky(WeatherKind::Rain, 200, 230, 12));
        let after = frame(&mut raining, 0.016).sun_color[0];
        assert!(after > before * 0.8, "{before} to {after}");
    }

    #[test]
    fn the_panel_names_the_hour_while_it_follows_the_clock() {
        use gc_sim::WeatherKind;
        let mut atmosphere = Atmosphere::new(None, None);
        atmosphere.follow(sim_sky(WeatherKind::Clear, 0, 0, 23));
        frame(&mut atmosphere, 0.016);
        assert!(atmosphere.panel_text().contains("23:00 light"));
        atmosphere.next_preset();
        assert!(!atmosphere.panel_text().contains("23:00"));
    }

    #[test]
    fn cloud_dims_the_day_but_heavy_rain_does_not_dim_it_twice() {
        use gc_sim::WeatherKind;
        let sun = |kind, strength, cloud| {
            let mut atmosphere = Atmosphere::new(None, None);
            atmosphere.follow(sim_sky(kind, strength, cloud, 12));
            frame(&mut atmosphere, 0.016).sun_color[0]
        };
        let clear = sun(WeatherKind::Clear, 0, 0);
        let overcast = sun(WeatherKind::Clear, 0, 220);
        assert!(overcast < clear * 0.7, "{overcast} {clear}");
        // Rain at full strength already means heavy cloud.
        let rain = sun(WeatherKind::Rain, 255, 0);
        assert_eq!(sun(WeatherKind::Rain, 255, 255), rain);
        assert!(rain < overcast);
    }

    #[test]
    fn mist_is_thickest_at_dawn_and_gone_at_noon() {
        assert_eq!(mist_at(0.0), mist_at(24.0));
        assert!(mist_at(6.0) > mist_at(8.5) && mist_at(8.5) > mist_at(10.5));
        assert_eq!(mist_at(12.0), 0.0);
        assert!(mist_at(17.5) > 0.0 && mist_at(22.0) > mist_at(14.5));
        for tenth in 0..240 {
            let m = mist_at(tenth as f32 / 10.0);
            assert!((0.0..=1.0).contains(&m), "{tenth}: {m}");
        }
    }

    #[test]
    fn the_atmosphere_lays_mist_by_the_clock_the_rain_and_the_fog() {
        use gc_sim::WeatherKind;
        let mist = |kind, strength, hour| {
            let mut atmosphere = Atmosphere::new(None, None);
            atmosphere.follow(sim_sky(kind, strength, 0, hour));
            for _ in 0..4000 {
                frame(&mut atmosphere, 0.1);
            }
            atmosphere.mist()
        };
        assert!(mist(WeatherKind::Clear, 0, 6) > 0.9);
        assert_eq!(mist(WeatherKind::Clear, 0, 12), 0.0);
        // A noon after rain is misty again; fog has no mist of its own.
        assert!(mist(WeatherKind::Rain, 255, 12) > 0.3);
        assert_eq!(mist(WeatherKind::Fog, 255, 6), 0.0);
        // A fog going out (F12) still holds the mist back while it clears.
        let mut atmosphere = Atmosphere::new(None, None);
        atmosphere.follow(sim_sky(WeatherKind::Clear, 0, 0, 6));
        for _ in 0..3 {
            atmosphere.next_weather();
        }
        for _ in 0..400 {
            frame(&mut atmosphere, 0.1);
        }
        assert_eq!(atmosphere.weather().0, Weather::Fog);
        assert!(atmosphere.mist() < 0.05);
        atmosphere.next_weather();
        frame(&mut atmosphere, 0.016);
        assert_eq!(atmosphere.weather().0, Weather::Clear);
        let clearing = atmosphere.mist();
        assert!(clearing < 0.3, "{clearing}");
        for _ in 0..400 {
            frame(&mut atmosphere, 0.1);
        }
        assert!(atmosphere.mist() > clearing);
        // `--look` and F9 move the mist with the look, not the clock: night
        // in a noon match is a night's mist.
        let mut flagged = Atmosphere::new(Some(LookPreset::Night), None);
        flagged.follow(sim_sky(WeatherKind::Clear, 0, 0, 12));
        assert!((flagged.mist() - mist_at(23.0)).abs() < 1e-6);
        // A sky with no clock is the usual 17:00.
        let sandbox = Atmosphere::new(None, None);
        assert!((sandbox.mist() - mist_at(17.0)).abs() < 1e-6);
    }

    #[test]
    fn the_wind_is_the_simulations_unless_the_weather_was_chosen_by_hand() {
        use gc_sim::WeatherKind;
        let mut atmosphere = Atmosphere::new(None, None);
        assert_eq!(atmosphere.wind(), PREVIEW_WIND, "no sky yet");
        let mut sky = sim_sky(WeatherKind::Rain, 200, 0, 12);
        sky.wind = Vec2::new(-3.0, 1.0);
        atmosphere.follow(sky);
        frame(&mut atmosphere, 0.016);
        assert_eq!(atmosphere.wind(), Vec2::new(-3.0, 1.0));
        atmosphere.next_weather();
        assert_eq!(atmosphere.wind(), PREVIEW_WIND);
        // The simulation's sixteenths of a tile a second.
        let view = SkyView::at(
            gc_sim::Weather {
                kind: WeatherKind::Rain,
                strength: 1,
                cloud: 0,
                wind: (16, -8),
            },
            gc_sim::Daylight {
                tick_of_day: 0,
                day_ticks: 100,
            },
        );
        assert_eq!(view.wind, Vec2::new(1.0, -0.5));
    }

    #[test]
    fn each_flash_asks_for_one_bolt() {
        let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(Weather::Rain));
        let mut strikes = 0;
        for _ in 0..1200 {
            frame(&mut atmosphere, 0.05);
            strikes += usize::from(atmosphere.take_strike());
        }
        // A minute of storm: one flash every 5 to 16 seconds.
        assert!((3..=12).contains(&strikes), "{strikes}");
        assert!(!atmosphere.take_strike(), "asked for once");
        let mut dry = Atmosphere::new(None, None);
        for _ in 0..1200 {
            frame(&mut dry, 0.05);
            assert!(!dry.take_strike());
        }
    }

    #[test]
    fn the_ground_soaks_in_rain_and_dries_slowly_after() {
        let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(Weather::Rain));
        assert!(run(&mut atmosphere, 0.1).wet < 0.05, "dry at first");
        let wet = run(&mut atmosphere, 40.0).wet;
        assert!(wet > 0.9, "sodden after a while of rain: {wet}");
        atmosphere.next_weather();
        atmosphere.next_weather();
        atmosphere.next_weather();
        assert_eq!(atmosphere.weather().0, Weather::Clear);
        let later = run(&mut atmosphere, 30.0).wet;
        assert!(later > 0.6 && later < wet, "still wet 30 s on: {later}");
        assert_eq!(
            run(&mut atmosphere, 900.0).wet,
            0.0,
            "dry in the end, not a trace"
        );
    }

    #[test]
    fn snow_settles_while_it_falls_stays_and_melts_slowly_or_fast_under_rain() {
        use gc_sim::WeatherKind;
        let run = |atmosphere: &mut Atmosphere, secs: f32| {
            let mut look = frame(atmosphere, 0.016);
            for _ in 0..(secs * 10.0) as u32 {
                look = frame(atmosphere, 0.1);
            }
            look
        };
        let mut atmosphere = Atmosphere::new(None, None);
        atmosphere.follow(sim_sky(WeatherKind::Snow, 255, 0, 12));
        assert!(run(&mut atmosphere, 0.1).snow < 0.05, "bare at first");
        let early = run(&mut atmosphere, 20.0).snow;
        let late = run(&mut atmosphere, 200.0).snow;
        assert!(early > 0.1 && early < 0.7 && late > 0.95, "{early} {late}");
        // The fall stops: it stays, melting over minutes.
        atmosphere.follow(sim_sky(WeatherKind::Clear, 0, 0, 12));
        let after = run(&mut atmosphere, 60.0).snow;
        assert!(after > 0.7 && after < late, "{after}");
        assert!(
            run(&mut atmosphere, 1500.0).snow == 0.0,
            "melted in the end"
        );
        // Rain melts what is left quickly, and snow does not wet the ground.
        atmosphere.follow(sim_sky(WeatherKind::Snow, 255, 0, 12));
        let covered = run(&mut atmosphere, 200.0);
        assert!(covered.snow > 0.95 && covered.wet == 0.0);
        atmosphere.follow(sim_sky(WeatherKind::Rain, 255, 0, 12));
        assert!(run(&mut atmosphere, 100.0).snow < 0.05);
        // A new match starts bare, and Clarity keeps a share.
        atmosphere.start(LookPreset::Noon);
        assert_eq!(atmosphere.settled, 0.0);
        let mut plain = Atmosphere::new(None, None);
        plain.follow(sim_sky(WeatherKind::Snow, 255, 0, 12));
        let full = run(&mut plain, 300.0).snow;
        let clear = frame_with(&mut plain, 0.1, true).snow;
        assert!(
            (clear - full * CLARITY_WEATHER).abs() < 0.02,
            "{clear} {full}"
        );
    }

    #[test]
    fn snow_and_fog_do_not_wet_the_ground_and_a_new_match_starts_dry() {
        for weather in [Weather::Snow, Weather::Fog] {
            let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(weather));
            assert_eq!(run(&mut atmosphere, 60.0).wet, 0.0);
        }
        // Rain by F12, then on to snow, and a match begins as it clears.
        let mut atmosphere = Atmosphere::new(None, None);
        atmosphere.next_weather();
        assert_eq!(atmosphere.weather().0, Weather::Rain);
        assert!(run(&mut atmosphere, 60.0).wet > 0.9);
        atmosphere.next_weather();
        atmosphere.start(LookPreset::Noon);
        assert_eq!(atmosphere.soaked, 0.0);
        let map = Map::generate(1, 64);
        let fresh = atmosphere.look(0.1, &map, Vec2::new(32.0, 32.0), 40.0, false);
        assert_eq!(
            fresh.wet, 0.0,
            "the last match's rain does not wet this one"
        );
    }

    #[test]
    fn a_bolt_is_a_sharp_flash_then_a_dimmer_one_and_dies_away() {
        assert_eq!(bolt(0.0), 0.0);
        assert!(bolt(0.04) > 0.99);
        assert!(bolt(0.15) < bolt(0.04));
        assert!(bolt(0.26) > bolt(0.18), "the second stroke");
        assert!(bolt(0.26) < bolt(0.04));
        assert!(bolt(BOLT_LENGTH) < 0.02);
    }

    #[test]
    fn lightning_comes_in_heavy_rain_only_and_lights_the_sky() {
        let (map, at) = (Map::generate(1, 64), Vec2::new(32.0, 32.0));
        // The brightest the sky gets in a minute, and its first frame.
        let sky = |weather| {
            let mut atmosphere = Atmosphere::new(None, None).with_weather(Some(weather));
            let first = atmosphere.look(0.0, &map, at, 40.0, false).sky_ambient[2];
            let most = (0..1200)
                .map(|_| atmosphere.look(0.05, &map, at, 40.0, false).sky_ambient[2])
                .fold(first, f32::max);
            (first, most)
        };
        let (calm, brightest) = sky(Weather::Rain);
        assert!(brightest > calm * 3.0, "{brightest} against {calm}");
        for weather in [Weather::Clear, Weather::Snow, Weather::Fog] {
            let (first, most) = sky(weather);
            assert_eq!(first, most, "{weather:?}");
        }
    }

    #[test]
    fn clarity_keeps_a_little_of_the_flash() {
        let (map, at) = (Map::generate(1, 64), Vec2::new(32.0, 32.0));
        let peak = |clarity| {
            let mut sky = Atmosphere::new(None, None).with_weather(Some(Weather::Rain));
            (0..1200)
                .map(|_| sky.look(0.05, &map, at, 40.0, clarity).sky_ambient[2])
                .fold(0.0, f32::max)
        };
        assert!(peak(true) < peak(false));
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
