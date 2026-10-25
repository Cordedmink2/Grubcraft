//! Living waterways, stage 4 (docs/38-living-waterways.md): the computer
//! players read the water. A ford a flash flood is coming down, or a flood
//! has shut, is no way across to them; once the water has gone it is again.

use gc_ai::terrain::{Terrain, Water};
use gc_fixed::Angle;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::sky::{Spell, Weather, WeatherKind};
use gc_sim::{Capacities, Flow, Ground, MatchSetup, Sky, World};
use glam::Vec2;

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

fn flow() -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            if RIVER.contains(&(i / MAP)) {
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

fn world(seed: u64) -> World {
    let (heights, nav, flow) = (heights(), nav(), flow());
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
            spells: CHANGING,
            ..Sky::STILL
        },
        generals: false,
    })
}

fn step(w: &mut World, minds: &mut Terrain) -> Water {
    w.tick(&[]);
    minds.read_water(w, Water::of(w));
    minds.seen_water()
}

/// A seed whose storm rolls a flash flood (most do not every time).
fn flooding_seed() -> u64 {
    (0..64)
        .find(|&seed| {
            let mut w = world(seed);
            while w.tick_count() < 12 * SECOND {
                w.tick(&[]);
            }
            w.flood().flash().wave().is_some()
        })
        .expect("some seed rolls a flash flood")
}

#[test]
fn the_minds_keep_off_a_ford_the_water_is_coming_over() {
    let ford = Vec2::new(32.5, 32.5);
    let mut w = world(flooding_seed());
    let mut minds = Terrain::new(&nav(), MAP as u32);
    assert_eq!(minds.cost(ford), 3, "a ford to wade before the storm");
    let mut warned = false;
    while w.tick_count() < 60 * SECOND {
        let water = step(&mut w, &mut minds);
        if water.wave.is_some() {
            warned = true;
            break;
        }
    }
    assert!(warned, "the wave's warning comes");
    assert_eq!(minds.cost(ford), BLOCKED, "kept off once the warning comes");
    assert!(minds.ford_shut_near(ford, 2.0));
    // Once the wave and the storm's flood have both gone, wade again.
    while w.tick_count() < 30 * 60 * SECOND {
        let water = step(&mut w, &mut minds);
        if water == Water::default() {
            break;
        }
    }
    assert_eq!(Water::of(&w), Water::default(), "the water goes down");
    assert_eq!(minds.cost(ford), 3, "the ford can be waded again");
    assert!(!minds.ford_shut_near(ford, 2.0));
}
