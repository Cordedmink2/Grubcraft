//! Fords that flood (docs/32-changing-land.md, "Fords that flood"): long
//! rain swells a river over its ford, which then can't be crossed; troops
//! caught in it are swept about and make for the bank; when the rain
//! stops the water goes down and the ford opens again.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::flood::FLOOD_RISE;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::sky::{Spell, Weather, WeatherKind};
use gc_sim::{Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, Sky, World};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const MAP: i32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
/// The river's rows, and the ford's columns across it.
const RIVER: core::ops::Range<i32> = 30..34;
const FORD: core::ops::Range<i32> = 24..40;
const WATER: u16 = 256;

/// The river under the water line, low banks a row either side of it
/// just above, and higher ground beyond.
fn heights() -> Vec<u16> {
    (0..MAP * MAP)
        .map(|i| match i / MAP {
            y if RIVER.contains(&y) => 100,
            y if y == RIVER.start - 1 || y == RIVER.end => 300,
            _ => 600,
        })
        .collect()
}

/// Open ground, the river deep and blocked but for the ford.
fn nav() -> Vec<u8> {
    let side = MAP * CELLS_PER_TILE;
    (0..side * side)
        .map(|c| {
            let (x, y) = (c % side / CELLS_PER_TILE, c / side / CELLS_PER_TILE);
            if RIVER.contains(&y) && !FORD.contains(&x) {
                BLOCKED
            } else if RIVER.contains(&y) {
                3
            } else {
                1
            }
        })
        .collect()
}

const RAIN: Weather = Weather {
    kind: WeatherKind::Rain,
    strength: 255,
    cloud: 255,
    wind: (0, 0),
};
const FAIR: Weather = Weather {
    kind: WeatherKind::Clear,
    strength: 0,
    cloud: 0,
    wind: (0, 0),
};
/// Rain from the start.
const RAINING: &[Spell] = &[Spell {
    at: 0,
    weather: RAIN,
}];
/// Rain for six minutes, then fair.
const SHOWER: &[Spell] = &[
    Spell {
        at: 0,
        weather: RAIN,
    },
    Spell {
        at: 360 * SECOND,
        weather: FAIR,
    },
];

fn world(spells: &'static [Spell]) -> World {
    let heights = heights();
    let nav = nav();
    World::new(&MatchSetup {
        seed: 9,
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
            flow: &[],
        },
        champions: false,
        sky: Sky {
            spells,
            ..Sky::STILL
        },
        generals: false,
    })
}

fn spawn(w: &mut World, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: SPEARS,
            position: FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF),
            facing,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn order(w: &mut World, regiment: RegimentHandle, x: i32, y: i32) {
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment,
            target: FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
}

fn at(w: &World, h: RegimentHandle) -> FxVec2 {
    w.regiments.iter().find(|r| r.handle == h).unwrap().position
}

fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

/// Ticks `w` until the fords shut or open (`closed`), at most `seconds`;
/// returns whether they did.
fn until_flooded(w: &mut World, closed: bool, seconds: u32) -> bool {
    for _ in 0..seconds * SECOND {
        w.tick(&[]);
        if w.events().contains(&Event::Flooded { closed }) {
            return true;
        }
    }
    false
}

#[test]
fn long_rain_floods_the_ford_and_shuts_it() {
    let mut w = world(RAINING);
    assert_eq!(w.flood().fords().count(), FORD.len() * RIVER.len());
    // Before the flood, a regiment walks across the ford.
    let a = spawn(&mut w, 32, 24, SOUTH);
    order(&mut w, a, 32, 40);
    for _ in 0..60 * SECOND {
        w.tick(&[]);
    }
    assert!(at(&w, a).y > Fx::from_int(36), "crossed at {:?}", at(&w, a));
    assert!(!w.flood().closed());
    // Some four minutes of heavy rain, and the river is up.
    assert!(until_flooded(&mut w, true, 300));
    assert!(w.flood().closed());
    assert_eq!(w.terrain().rise(), FLOOD_RISE);
    // Now nobody crosses: sent back, the regiment stops on the bank.
    order(&mut w, a, 32, 22);
    for _ in 0..60 * SECOND {
        w.tick(&[]);
    }
    assert!(at(&w, a).y > Fx::from_int(RIVER.end), "{:?}", at(&w, a));
}

#[test]
fn troops_caught_in_the_flood_are_swept_and_make_for_the_bank() {
    let mut w = world(RAINING);
    let a = spawn(&mut w, 32, 31, NORTH);
    let full = hp(&w, a);
    assert!(until_flooded(&mut w, true, 300));
    assert!(w.swept(a));
    // A second in the torrent costs them, and they struggle out.
    for _ in 0..30 * SECOND {
        w.tick(&[]);
    }
    assert!(hp(&w, a) < full);
    assert!(!w.swept(a), "still in the ford at {:?}", at(&w, a));
}

#[test]
fn the_ford_opens_again_once_the_rain_stops() {
    let mut w = world(SHOWER);
    assert!(until_flooded(&mut w, true, 300));
    assert!(until_flooded(&mut w, false, 600));
    assert!(!w.flood().closed());
    assert!(w.terrain().rise() < FLOOD_RISE / 2);
    // And a regiment crosses again.
    let a = spawn(&mut w, 32, 24, SOUTH);
    order(&mut w, a, 32, 40);
    for _ in 0..60 * SECOND {
        w.tick(&[]);
    }
    assert!(at(&w, a).y > Fx::from_int(36), "crossed at {:?}", at(&w, a));
}

#[test]
fn a_flood_raises_the_water_up_the_banks() {
    let mut w = world(RAINING);
    let tile = |x: i32, y: i32| FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF);
    let (bank, high) = (tile(10, RIVER.start - 1), tile(10, RIVER.start - 2));
    assert!(!w.terrain().wet(bank));
    assert!(until_flooded(&mut w, true, 300));
    assert!(w.terrain().wet(bank), "the low bank is under water");
    assert!(!w.terrain().wet(high));
}
