//! Living waterways, stage 3 (docs/38-living-waterways.md): a storm in
//! changing weather may send a flash flood down a river. A warning comes
//! first, then the river's fords shut as the wave reaches them, and open
//! again once the water has fallen. Fixed weather and still water never
//! get one.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::flash::Stage;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::sky::{Spell, Weather, WeatherKind};
use gc_sim::{Capacities, Command, Flow, Ground, MatchSetup, PlayerCommand, Sky, World};

const MAP: i32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const WATER: u16 = 256;
/// A river running east off the map, deep but for a ford.
const RIVER: core::ops::Range<i32> = 24..40;
const FORD: core::ops::Range<i32> = 30..34;

fn heights() -> Vec<u16> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            match () {
                _ if RIVER.contains(&y) && FORD.contains(&x) => WATER - 40,
                _ if RIVER.contains(&y) => 20,
                _ => 600,
            }
        })
        .collect()
}

fn nav() -> Vec<u8> {
    let side = MAP * CELLS_PER_TILE;
    (0..side * side)
        .map(|c| {
            let (x, y) = (c % side / CELLS_PER_TILE, c / side / CELLS_PER_TILE);
            match () {
                _ if RIVER.contains(&y) && FORD.contains(&x) => 3,
                _ if RIVER.contains(&y) => BLOCKED,
                _ => 1,
            }
        })
        .collect()
}

fn flow(running: bool) -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            if running && RIVER.contains(&(i / MAP)) {
                Flow::byte(Angle(0), 2)
            } else {
                0
            }
        })
        .collect()
}

const STORM: Weather = Weather {
    kind: WeatherKind::Rain,
    strength: 255,
    cloud: 255,
    wind: (0, 0),
};

/// Changing weather: a storm ten seconds in.
const CHANGING: &[Spell] = &[
    Spell {
        at: 0,
        weather: Weather::FAIR,
    },
    Spell {
        at: 10 * SECOND,
        weather: STORM,
    },
    Spell {
        at: 4 * 60 * SECOND,
        weather: Weather::FAIR,
    },
];

/// A storm all match long: fixed weather.
const FIXED: &[Spell] = &[Spell {
    at: 0,
    weather: STORM,
}];

fn world(seed: u64, spells: &'static [Spell], running: bool) -> World {
    let (heights, nav, flow) = (heights(), nav(), flow(running));
    World::new(&MatchSetup {
        seed,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: MAP as u32,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &heights,
            water_level: WATER,
            cover: &[],
            growth: &[],
            footing: &[],
            flow: &flow,
        },
        champions: false,
        sky: Sky {
            spells,
            ..Sky::STILL
        },
        generals: false,
    })
}

/// The flash flood stages told up to `ticks`.
fn stages(w: &mut World, ticks: u32) -> Vec<(u32, Stage)> {
    let mut told = Vec::new();
    while w.tick_count() < ticks {
        w.tick(&[]);
        for e in w.events() {
            if let Event::FlashFlood { stage } = *e {
                told.push((w.tick_count(), stage));
            }
        }
    }
    told
}

/// A seed whose storm rolls a flash flood (most do not every time).
fn flooding_seed() -> u64 {
    (0..64)
        .find(|&seed| {
            let mut w = world(seed, CHANGING, true);
            stages(&mut w, 12 * SECOND);
            w.flood().flash().wave().is_some()
        })
        .expect("some seed rolls a flash flood")
}

#[test]
fn a_storm_sends_a_flash_flood_down_the_river() {
    let mut w = world(flooding_seed(), CHANGING, true);
    assert_eq!(w.flood().flash().rivers(), 1);
    // A regiment standing in the ford.
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::new(Fx::from_int(32), Fx::from_int(32)),
            facing: Angle(0),
        },
    }]);
    let h = w.regiments.iter().next().unwrap().handle;
    let told = stages(&mut w, 2 * 60 * SECOND);
    let order: Vec<Stage> = told.iter().map(|&(_, s)| s).collect();
    assert_eq!(order, [Stage::Coming, Stage::Here, Stage::Gone], "{told:?}");
    // The warning comes before the wave, the wave a while after the storm.
    assert!(told[0].0 < told[1].0);
    assert!(told[1].0 > 10 * SECOND);
    // The ford was shut and the regiment struggled out of it.
    let at = w.regiments.iter().find(|r| r.handle == h).unwrap().position;
    assert!(!w.swept(h));
    assert!(
        !(Fx::from_int(RIVER.start + 1)..Fx::from_int(RIVER.end - 1)).contains(&at.y),
        "{at:?}"
    );
    assert!(w.flood().flash().wave().is_none());
}

#[test]
fn the_wave_runs_only_while_the_water_is_up() {
    let mut w = world(flooding_seed(), CHANGING, true);
    let told = stages(&mut w, 60 * SECOND);
    assert!(told.iter().any(|&(_, s)| s == Stage::Here), "{told:?}");
    assert!(w.terrain().wave_running());
    stages(&mut w, 2 * 60 * SECOND);
    assert!(!w.terrain().wave_running());
}

#[test]
fn fixed_weather_and_still_water_never_flash_flood() {
    for seed in 0..16 {
        let mut w = world(seed, FIXED, true);
        assert!(stages(&mut w, 60 * SECOND).is_empty());
        let mut w = world(seed, CHANGING, false);
        assert!(stages(&mut w, 60 * SECOND).is_empty());
        assert_eq!(w.flood().flash().rivers(), 0);
    }
}
