//! The sky: weather and the time of day (docs/32-changing-land.md,
//! "Weather and the time of day").
//!
//! The match's [`Sky`] (from [`MatchSetup::sky`](crate::MatchSetup::sky))
//! says where the clock starts, how long a day is and which spells of
//! weather come when. Each tick the world moves the weather towards the
//! latest spell that has begun, a step at a time, so it changes smoothly:
//! a new kind of weather first fades the old one out. The game reads
//! [`World::weather`](crate::World::weather) and
//! [`World::daylight`](crate::World::daylight) to draw it.
//!
//! What it does in play is the [`Air`]: how far troops see and shoot, and
//! how fast they march. Night and weather each take a share of the sight,
//! and snow slows the march. Rain also wets the ground, which soaks low
//! ground near water soft ([`crate::Terrain::soak`]).

use alloc::vec::Vec;

use gc_fixed::{Fx, Pcg32};

use crate::{StateHasher, TICKS_PER_SECOND};

/// What is falling or hanging in the air, in the order the renderer's
/// weather lists them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WeatherKind {
    #[default]
    Clear = 0,
    Rain = 1,
    Snow = 2,
    Fog = 3,
}

impl WeatherKind {
    pub const ALL: [WeatherKind; 4] = [
        WeatherKind::Clear,
        WeatherKind::Rain,
        WeatherKind::Snow,
        WeatherKind::Fog,
    ];
}

/// The weather at a moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Weather {
    pub kind: WeatherKind,
    /// How hard it rains or snows, or how thick the fog is: 0 to 255.
    /// Nothing for clear weather.
    pub strength: u8,
    /// Cloud cover, 0 (a clear sky) to 255 (overcast). Any kind may have
    /// cloud: an overcast day is clear weather under heavy cloud.
    pub cloud: u8,
    /// Which way and how hard the wind blows, in sixteenths of a tile a
    /// second along x and y. For drawing: it moves nothing in play.
    pub wind: (i8, i8),
}

impl Weather {
    /// A clear, still sky.
    pub const FAIR: Weather = Weather {
        kind: WeatherKind::Clear,
        strength: 0,
        cloud: 0,
        wind: (0, 0),
    };
}

/// A spell of weather: from tick `at` on, the weather moves towards
/// `weather`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Spell {
    pub at: u32,
    pub weather: Weather,
}

/// Ticks in a game day: eight hours of play, so an hour of the day is
/// twenty minutes and a dusk battle has time to turn into a night one.
pub const DAY_TICKS: u32 = 8 * 60 * 60 * TICKS_PER_SECOND;

/// The tick of a [`DAY_TICKS`] day at `hour`:`minute`.
pub const fn at_hour(hour: u32, minute: u32) -> u32 {
    (hour * 60 + minute) * (DAY_TICKS / (24 * 60))
}

/// A match's sky, as the setup gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sky<'a> {
    /// The tick of the day at the match's first tick.
    pub start: u32,
    /// Ticks in a day; the clock goes round once in this many.
    pub day_ticks: u32,
    /// Whether the clock moves. A still clock keeps `start` all match.
    pub runs: bool,
    /// The spells of weather, by their starting tick. The first one in
    /// force at tick 0 is the weather from the start, without easing in.
    pub spells: &'a [Spell],
    /// Ticks in a year of four [`Season`]s, the match starting with the
    /// spring; 0 for a match with no seasons.
    pub year_ticks: u32,
}

impl Sky<'static> {
    /// A fair afternoon whose clock stands still: what every match had
    /// before the sky, and tests and old replays still have.
    pub const STILL: Sky<'static> = Sky {
        start: at_hour(17, 0),
        day_ticks: DAY_TICKS,
        runs: false,
        spells: &[],
        year_ticks: 0,
    };
}

impl Default for Sky<'static> {
    fn default() -> Self {
        Self::STILL
    }
}

/// The seasons of a year (docs/32-changing-land.md, "Seasons"), in the
/// order they come: a match with seasons starts in the spring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [
        Season::Spring,
        Season::Summer,
        Season::Autumn,
        Season::Winter,
    ];

    /// Its name and what it does, in a word, for the top bar.
    pub fn chip(self) -> &'static str {
        match self {
            Season::Spring => "Spring",
            Season::Summer => "Summer: dry",
            Season::Autumn => "Autumn: harvest",
            Season::Winter => "Winter: frost",
        }
    }

    /// The weather it brings, when the weather changes.
    pub fn climate(self) -> Climate {
        match self {
            Season::Spring => Climate::Showery,
            Season::Summer => Climate::Dry,
            Season::Autumn => Climate::Misty,
            Season::Winter => Climate::Cold,
        }
    }

    /// Percent of the usual speed workers gather Grub at: the autumn's
    /// harvest, the winter's lean months.
    pub fn grub_percent(self) -> u32 {
        match self {
            Season::Autumn => 125,
            Season::Winter => 75,
            Season::Spring | Season::Summer => 100,
        }
    }

    /// Percent of the usual chance fire spreads: dry summer woods catch
    /// more easily, frozen winter ones less.
    pub fn fire_percent(self) -> u32 {
        match self {
            Season::Summer => 150,
            Season::Winter => 50,
            Season::Spring | Season::Autumn => 100,
        }
    }

    /// How many times as fast the ground dries out: twice in the summer.
    pub fn drying(self) -> u16 {
        match self {
            Season::Summer => 2,
            _ => 1,
        }
    }
}

/// Where the year stands at a tick: the season, how far into it, how
/// long it lasts, and which year of the match (0 the first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeasonAt {
    pub season: Season,
    pub into: u32,
    pub length: u32,
    pub year: u32,
}

/// The season at `tick` of a year `year_ticks` long (`None` with no
/// seasons).
pub fn season_at(year_ticks: u32, tick: u32) -> Option<SeasonAt> {
    let length = year_ticks / 4;
    if length == 0 {
        return None;
    }
    let quarter = tick / length;
    Some(SeasonAt {
        season: Season::ALL[(quarter % 4) as usize],
        into: tick % length,
        length,
        year: quarter / 4,
    })
}

/// How the weather changes, for [`changing`]: how often it is clear or
/// foggy, and whether what falls is rain or snow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Climate {
    /// Rain, fog and clear spells.
    Wet,
    /// Snow, fog and clear spells.
    Cold,
    /// The spring's: showers more often than not.
    Showery,
    /// The summer's: mostly clear, with the odd storm.
    Dry,
    /// The autumn's: fog and rain as often as clear sky.
    Misty,
}

impl Climate {
    /// Of 100 rolls, the first so many are clear and the next so many
    /// foggy; the rest rain or snow.
    fn mix(self) -> (u32, u32) {
        match self {
            Climate::Wet | Climate::Cold => (38, 20),
            Climate::Showery => (30, 15),
            Climate::Dry => (70, 5),
            Climate::Misty => (30, 35),
        }
    }
}

/// Spells of changing weather for a match of up to `ticks`, drawn from
/// `seed`: a new spell every three to six minutes, a little over a third
/// of them clear. Every peer draws the same ones from the same seed.
pub fn changing(seed: u64, climate: Climate, ticks: u32) -> Vec<Spell> {
    draw_spells(seed, ticks, 0, |_| climate)
}

/// [`changing`] weather that follows a year of seasons `year_ticks` long:
/// each spell has the climate of the season it begins in, and a new
/// season always starts a new spell, so no season's weather runs into the
/// next. With no seasons (`year_ticks` 0), the same as [`changing`] in a
/// [`Climate::Wet`].
pub fn seasonal(seed: u64, year_ticks: u32, ticks: u32) -> Vec<Spell> {
    draw_spells(seed, ticks, year_ticks / 4, |at| {
        season_at(year_ticks, at).map_or(Climate::Wet, |s| s.season.climate())
    })
}

/// Spells for `ticks`, a new one at least every `season` ticks from the
/// start when that isn't 0.
fn draw_spells(seed: u64, ticks: u32, season: u32, climate: impl Fn(u32) -> Climate) -> Vec<Spell> {
    let mut rng = Pcg32::new(seed, crate::streams::SKY);
    let minute = 60 * TICKS_PER_SECOND;
    let mut spells = Vec::new();
    let mut at = 0u32;
    while at < ticks {
        let roll = rng.next_u32() % 100;
        let climate = climate(at);
        let (clear, fog) = climate.mix();
        let kind = match roll {
            r if r < clear => WeatherKind::Clear,
            r if r < clear + fog => WeatherKind::Fog,
            _ if climate == Climate::Cold => WeatherKind::Snow,
            _ => WeatherKind::Rain,
        };
        let strength = match kind {
            WeatherKind::Clear => 0,
            _ => 90 + (rng.next_u32() % 166) as u8,
        };
        let cloud = match kind {
            WeatherKind::Clear => (rng.next_u32() % 200) as u8,
            _ => 160 + (rng.next_u32() % 96) as u8,
        };
        let wind = (
            (rng.next_u32() % 49) as i8 - 24,
            (rng.next_u32() % 49) as i8 - 24,
        );
        spells.push(Spell {
            at,
            weather: Weather {
                kind,
                strength,
                cloud,
                wind,
            },
        });
        let next = at.saturating_add(3 * minute + rng.next_u32() % (3 * minute));
        at = match season {
            0 => next,
            _ => next.min((at / season + 1).saturating_mul(season)),
        };
    }
    spells
}

/// The time of day at a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Daylight {
    /// Ticks since midnight.
    pub tick_of_day: u32,
    /// Ticks in a day.
    pub day_ticks: u32,
}

/// How far the sun is up, from the middle of the night (0) to noon
/// (`ONE`), at which dawn begins and ends (`light` goes 0 to full between
/// them). The same span either side of noon makes dusk.
const DAWN_FROM: i64 = 400_000;
const DAWN_TO: i64 = 600_000;
const ONE: i64 = 1_000_000;

impl Daylight {
    /// How light it is: 0 all night, rising through dawn to 1000, which
    /// lasts all day, and falling through dusk. Smooth from tick to tick.
    pub fn light(self) -> u16 {
        let day = i64::from(self.day_ticks.max(1));
        let t = i64::from(self.tick_of_day % self.day_ticks.max(1));
        // 0 at midnight, ONE at noon, in a straight line each way.
        let up = ONE - (2 * t * ONE / day - ONE).abs();
        let x = ((up - DAWN_FROM) * ONE / (DAWN_TO - DAWN_FROM)).clamp(0, ONE);
        // Smoothstep: 3x² − 2x³, so dawn starts and ends gently.
        let s = x * x / ONE * (3 * ONE - 2 * x) / ONE;
        (s / 1000) as u16
    }

    /// Whether it is dark enough to cut sight: under [`HALF_LIGHT`].
    pub fn dark(self) -> bool {
        self.light() < HALF_LIGHT
    }

    /// Whether it is the morning half of the day (midnight to noon): the
    /// light is a dawn's, not a dusk's.
    pub fn rising(self) -> bool {
        self.tick_of_day % self.day_ticks.max(1) < self.day_ticks.max(1) / 2
    }
}

/// What the sky does in play now: shares of the usual sight and range, and
/// of the usual marching pace, in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Air {
    pub sight_percent: u8,
    pub march_percent: u8,
    /// Sight as the weather leaves it, without the night's cut: how far a
    /// target in torchlight is seen and shot (see [`crate::vision`]).
    pub lit_percent: u8,
    /// Dark enough to hide in and to need torches: the night's share of
    /// sight is at most [`DARK_PERCENT`].
    pub dark: bool,
}

/// Sight in the dark of night, in percent of the day's.
pub const NIGHT_SIGHT_PERCENT: u32 = 45;
/// From when the night's share of sight is this low (percent) it is dark:
/// troops in the open hide and torches burn (late dusk to early dawn).
pub const DARK_PERCENT: u32 = 70;
/// The light from which sight is full, so a golden-hour battle sees as far
/// as a noon one; under it sight falls towards [`NIGHT_SIGHT_PERCENT`].
pub const HALF_LIGHT: u16 = 500;
/// What each kind of weather takes from sight at full strength, in percent.
pub const FOG_SIGHT_LOSS: u32 = 60;
pub const SNOW_SIGHT_LOSS: u32 = 30;
pub const RAIN_SIGHT_LOSS: u32 = 20;
/// What snow takes from the march at full strength, in percent.
pub const SNOW_MARCH_LOSS: u32 = 15;
/// The air's shares move in steps of this many percent, so they change
/// now and then rather than every tick.
pub const AIR_STEP: u32 = 5;

impl Air {
    /// Clear daylight: everything as usual.
    pub const CLEAR: Air = Air {
        sight_percent: 100,
        march_percent: 100,
        lit_percent: 100,
        dark: false,
    };

    fn of(weather: Weather, light: u16) -> Air {
        let lit = (u32::from(light) * 1000 / u32::from(HALF_LIGHT)).min(1000);
        let night = NIGHT_SIGHT_PERCENT + (100 - NIGHT_SIGHT_PERCENT) * lit / 1000;
        let loss = match weather.kind {
            WeatherKind::Clear => 0,
            WeatherKind::Rain => RAIN_SIGHT_LOSS,
            WeatherKind::Snow => SNOW_SIGHT_LOSS,
            WeatherKind::Fog => FOG_SIGHT_LOSS,
        };
        let strength = u32::from(weather.strength);
        let weather_share = 100 - loss * strength / 255;
        let sight = night * weather_share / 100;
        let march = match weather.kind {
            WeatherKind::Snow => 100 - SNOW_MARCH_LOSS * strength / 255,
            _ => 100,
        };
        // Round down to a step, never below one.
        let stepped = |p: u32| (p / AIR_STEP * AIR_STEP).max(AIR_STEP) as u8;
        Air {
            sight_percent: stepped(sight),
            march_percent: stepped(march),
            lit_percent: stepped(weather_share),
            dark: night <= DARK_PERCENT,
        }
    }

    /// `distance` (a sight or a range) as far as it reaches at a target in
    /// torchlight: the weather's cut, not the night's.
    pub fn lit_sight(self, distance: Fx) -> Fx {
        scale(distance, self.lit_percent)
    }

    /// `distance` (a sight or a range) as far as it reaches in this air.
    pub fn sight(self, distance: Fx) -> Fx {
        scale(distance, self.sight_percent)
    }

    /// A march `step` at this air's pace: never slowed to a standstill.
    pub fn march(self, step: Fx) -> Fx {
        if step > Fx::ZERO {
            scale(step, self.march_percent).max(Fx::from_raw(1))
        } else {
            step
        }
    }
}

impl Default for Air {
    fn default() -> Self {
        Self::CLEAR
    }
}

fn scale(v: Fx, percent: u8) -> Fx {
    if percent == 100 {
        v
    } else {
        Fx::from_raw((i64::from(v.raw()) * i64::from(percent) / 100) as i32)
    }
}

/// Ticks between steps of the weather's easing: a full fade (255 steps)
/// takes about 25 seconds.
pub const EASE_EVERY: u32 = 2;

/// How wet the ground is, from bone dry (0) to soaked through: the sky
/// keeps it (rain wets it, anything else dries it) and
/// [`crate::Terrain::soak`] turns tiles soft by it.
pub const WET_FULL: u16 = 1000;
/// Rain wets the ground by its strength over this much a second (heavy
/// rain about 3 of [`WET_FULL`]: some five minutes to soak it through).
pub const RAIN_WETS: u16 = 64;
/// Anything else (drizzle too light to wet it included) dries the ground
/// by this much a second.
pub const DRIES: u16 = 2;

/// The sky as the match plays it.
#[derive(Clone, Debug)]
pub(crate) struct SkyState {
    start: u32,
    day_ticks: u32,
    runs: bool,
    spells: Vec<Spell>,
    year_ticks: u32,
    /// The next spell to begin.
    next: usize,
    now: Weather,
    air: Air,
    /// How wet the ground is, 0 to [`WET_FULL`].
    wet: u16,
}

impl SkyState {
    pub(crate) fn new(sky: &Sky) -> SkyState {
        let mut spells = sky.spells.to_vec();
        spells.sort_by_key(|s| s.at);
        // The weather from the start: the last spell begun at tick 0.
        let first = spells.iter().take_while(|s| s.at == 0).count();
        let now = first
            .checked_sub(1)
            .map_or(Weather::FAIR, |i| spells[i].weather);
        let mut state = SkyState {
            start: sky.start,
            day_ticks: sky.day_ticks.max(1),
            runs: sky.runs,
            spells,
            year_ticks: sky.year_ticks,
            next: first,
            now,
            air: Air::CLEAR,
            wet: 0,
        };
        state.air = Air::of(now, state.daylight(0).light());
        state
    }

    /// The sky every match had before it: fair, and the clock still.
    /// Hashes as nothing, so such matches hash as they did before.
    fn is_still(&self) -> bool {
        !self.runs && self.spells.is_empty()
    }

    /// Where the year stands at `tick` (`None` with no seasons).
    pub(crate) fn season(&self, tick: u32) -> Option<SeasonAt> {
        season_at(self.year_ticks, tick)
    }

    pub(crate) fn weather(&self) -> Weather {
        self.now
    }

    pub(crate) fn daylight(&self, tick: u32) -> Daylight {
        let passed = if self.runs { tick % self.day_ticks } else { 0 };
        Daylight {
            tick_of_day: (self.start % self.day_ticks + passed) % self.day_ticks,
            day_ticks: self.day_ticks,
        }
    }

    pub(crate) fn air(&self) -> Air {
        self.air
    }

    pub(crate) fn wet(&self) -> u16 {
        self.wet
    }

    /// The weather it is moving towards: the latest spell's, or fair.
    fn goal(&self) -> Weather {
        self.next
            .checked_sub(1)
            .map_or(Weather::FAIR, |i| self.spells[i].weather)
    }

    /// Move on to `tick`; returns whether the air changed.
    pub(crate) fn tick(&mut self, tick: u32) -> bool {
        while self.spells.get(self.next).is_some_and(|s| s.at <= tick) {
            self.next += 1;
        }
        if tick.is_multiple_of(EASE_EVERY) {
            self.ease();
        }
        if tick.is_multiple_of(TICKS_PER_SECOND) {
            // Drizzle too light to wet the ground dries it, as fair weather.
            let wets = match self.now.kind {
                WeatherKind::Rain => u16::from(self.now.strength) / RAIN_WETS,
                _ => 0,
            };
            let drying = self.season(tick).map_or(1, |s| s.season.drying());
            self.wet = if wets > 0 {
                (self.wet + wets).min(WET_FULL)
            } else {
                self.wet.saturating_sub(DRIES * drying)
            };
        }
        let air = Air::of(self.now, self.daylight(tick).light());
        let changed = air != self.air;
        self.air = air;
        changed
    }

    /// One step of the weather towards its goal.
    fn ease(&mut self) {
        let goal = self.goal();
        let now = &mut self.now;
        if now.kind != goal.kind {
            if now.strength == 0 {
                now.kind = goal.kind;
            } else {
                now.strength -= 1;
            }
        } else {
            now.strength = toward(now.strength, goal.strength);
        }
        now.cloud = toward(now.cloud, goal.cloud);
        now.wind = (
            toward(now.wind.0, goal.wind.0),
            toward(now.wind.1, goal.wind.1),
        );
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        if self.is_still() {
            return;
        }
        // The goal follows from `next`.
        h.u32(self.next as u32);
        let w = self.now;
        h.u8(w.kind as u8);
        h.u8(w.strength);
        h.u8(w.cloud);
        h.u8(w.wind.0 as u8);
        h.u8(w.wind.1 as u8);
        h.u16(self.wet);
    }
}

/// `v` one step nearer `goal`.
fn toward<T>(v: T, goal: T) -> T
where
    T: Copy + Ord + From<bool> + core::ops::Add<Output = T> + core::ops::Sub<Output = T>,
{
    v + T::from(v < goal) - T::from(v > goal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32) -> Daylight {
        Daylight {
            tick_of_day: at_hour(hour, 0),
            day_ticks: DAY_TICKS,
        }
    }

    #[test]
    fn it_is_dark_at_night_light_by_day_and_dawn_is_smooth() {
        assert_eq!(at(0).light(), 0);
        assert_eq!(at(3).light(), 0);
        assert_eq!(at(12).light(), 1000);
        assert_eq!(at(9).light(), 1000);
        assert_eq!(at(21).light(), 0);
        let dawn = at(6).light();
        assert!(dawn > 200 && dawn < 800, "{dawn}");
        assert!(at(6).rising() && !at(18).rising());
        // No jump between neighbouring ticks through dawn.
        let mut last = 0u16;
        for t in at_hour(4, 0)..at_hour(8, 0) {
            let l = Daylight {
                tick_of_day: t,
                day_ticks: DAY_TICKS,
            }
            .light();
            assert!(l >= last && l - last <= 2, "{t}: {last} -> {l}");
            last = l;
        }
    }

    #[test]
    fn fog_and_night_cut_sight_and_snow_slows_the_march() {
        let fog = Weather {
            kind: WeatherKind::Fog,
            strength: 255,
            ..Weather::FAIR
        };
        assert_eq!(Air::of(Weather::FAIR, 1000), Air::CLEAR);
        assert_eq!(Air::of(fog, 1000).sight_percent, 40);
        assert_eq!(Air::of(Weather::FAIR, 0).sight_percent, 45);
        assert_eq!(Air::of(Weather::FAIR, 500).sight_percent, 100);
        let still = SkyState::new(&Sky::STILL);
        assert_eq!(still.air(), Air::CLEAR, "the old sky changes nothing");
        assert_eq!(Air::of(fog, 0).sight_percent, 15);
        let snow = Weather {
            kind: WeatherKind::Snow,
            strength: 255,
            ..Weather::FAIR
        };
        assert_eq!(Air::of(snow, 1000).march_percent, 85);
        assert_eq!(Air::of(fog, 1000).march_percent, 100);
    }

    #[test]
    fn a_new_kind_of_weather_fades_the_old_one_out_first() {
        let rain = Weather {
            kind: WeatherKind::Rain,
            strength: 100,
            cloud: 200,
            wind: (8, 0),
        };
        let fog = Weather {
            kind: WeatherKind::Fog,
            strength: 50,
            ..Weather::FAIR
        };
        let spells = [
            Spell {
                at: 0,
                weather: rain,
            },
            Spell {
                at: 10,
                weather: fog,
            },
        ];
        let mut sky = SkyState::new(&Sky {
            spells: &spells,
            ..Sky::STILL
        });
        assert_eq!(sky.weather(), rain, "the first spell is there at once");
        let mut tick = 0;
        while sky.weather().kind == WeatherKind::Rain {
            tick += 1;
            sky.tick(tick);
            assert!(tick < 1000);
        }
        assert_eq!(sky.weather().strength, 0, "rain gone before fog rises");
        while sky.weather() != fog {
            tick += 1;
            sky.tick(tick);
            assert!(tick < 2000);
        }
    }

    #[test]
    fn rain_wets_the_ground_and_drizzle_lets_it_dry() {
        let rain = |strength| {
            [Spell {
                at: 0,
                weather: Weather {
                    kind: WeatherKind::Rain,
                    strength,
                    ..Weather::FAIR
                },
            }]
        };
        let heavy = rain(200);
        let mut sky = SkyState::new(&Sky {
            spells: &heavy,
            ..Sky::STILL
        });
        for tick in 1..=TICKS_PER_SECOND * 100 {
            sky.tick(tick);
        }
        assert_eq!(sky.wet(), 300, "3 a second");
        let drizzle = rain(40);
        let mut sky = SkyState::new(&Sky {
            spells: &drizzle,
            ..Sky::STILL
        });
        sky.wet = 300;
        for tick in 1..=TICKS_PER_SECOND * 10 {
            sky.tick(tick);
        }
        assert_eq!(sky.wet(), 280, "drizzle dries it as fair weather does");
    }

    #[test]
    fn the_clock_runs_only_when_asked() {
        let still = SkyState::new(&Sky::STILL);
        assert_eq!(still.daylight(5000), still.daylight(0));
        let running = SkyState::new(&Sky {
            runs: true,
            ..Sky::STILL
        });
        assert_eq!(running.daylight(5000).tick_of_day, at_hour(17, 0) + 5000);
        assert_eq!(running.daylight(DAY_TICKS), running.daylight(0));
    }

    #[test]
    fn changing_weather_is_the_same_from_the_same_seed() {
        let a = changing(7, Climate::Wet, 60 * 60 * TICKS_PER_SECOND);
        assert_eq!(a, changing(7, Climate::Wet, 60 * 60 * TICKS_PER_SECOND));
        assert_ne!(a, changing(8, Climate::Wet, 60 * 60 * TICKS_PER_SECOND));
        assert!(a.len() >= 10 && a.len() <= 21, "{}", a.len());
        assert!(a.iter().all(|s| s.weather.kind != WeatherKind::Snow));
        assert_eq!(a[0].at, 0);
    }
}
