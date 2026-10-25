//! Living waterways, stage 1 (docs/38-living-waterways.md): with seasons
//! the rivers follow the year. The winter's snow melts in the spring and
//! swells them over the fords; a dry summer drops the water, so the deep
//! edge of the channel can be waded.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::flood::{LOW_DROP, MELT_MIN};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::sky::{Season, Spell, Weather, WeatherKind};
use gc_sim::{Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, Sky, World};

const SOUTH: Angle = Angle(0x4000);
const MAP: i32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const MINUTE: u32 = 60 * SECOND;
/// Two-minute seasons: summer from 2 minutes, winter from 6, the next
/// spring from 8.
const YEAR: u32 = 4 * 2 * MINUTE;
const WATER: u16 = 256;
/// The river's rows, with a shallow edge at each side of a deep middle.
const RIVER: core::ops::Range<i32> = 28..36;
const DEEP: core::ops::Range<i32> = 30..34;
/// Columns where the deep middle is shallow enough to wade in low water.
const BAR: core::ops::Range<i32> = 40..46;
/// Columns where the deep middle's north edge is as shallow: a strip along
/// one bank, which crosses nothing, so it stays deep.
const STRIP: core::ops::Range<i32> = 10..20;

fn heights() -> Vec<u16> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            match () {
                // Deep water too deep for any summer, but for the bar: a
                // quarter of a tile under the wading depth.
                _ if DEEP.contains(&y) && BAR.contains(&x) => WATER - 89 - 30,
                _ if y == DEEP.start && STRIP.contains(&x) => WATER - 89 - 30,
                _ if DEEP.contains(&y) => 20,
                _ if RIVER.contains(&y) => WATER - 40,
                _ => 600,
            }
        })
        .collect()
}

/// Open ground, the river's deep middle blocked, its edges a ford.
fn nav() -> Vec<u8> {
    let side = MAP * CELLS_PER_TILE;
    (0..side * side)
        .map(|c| {
            let y = c / side / CELLS_PER_TILE;
            if DEEP.contains(&y) {
                BLOCKED
            } else if RIVER.contains(&y) {
                3
            } else {
                1
            }
        })
        .collect()
}

/// Fair all year, but for heavy snow through the winter.
const SNOWY: &[Spell] = &[
    Spell {
        at: 0,
        weather: Weather::FAIR,
    },
    Spell {
        at: 6 * MINUTE,
        weather: Weather {
            kind: WeatherKind::Snow,
            strength: 255,
            cloud: 255,
            wind: (0, 0),
        },
    },
    Spell {
        at: 8 * MINUTE,
        weather: Weather::FAIR,
    },
];

fn world(spells: &'static [Spell], year_ticks: u32) -> World {
    let (heights, nav) = (heights(), nav());
    World::new(&MatchSetup {
        seed: 11,
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
        },
        champions: false,
        sky: Sky {
            spells,
            year_ticks,
            ..Sky::STILL
        },
        generals: false,
    })
}

fn spawn(w: &mut World, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF),
            facing: SOUTH,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    let h = w
        .regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h));
    h.unwrap()
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

/// Ticks `w` to `tick`, returning the events it told on the way.
fn run_to(w: &mut World, tick: u32) -> Vec<Event> {
    let mut told = Vec::new();
    while w.tick_count() < tick {
        w.tick(&[]);
        told.extend(
            w.events()
                .iter()
                .filter(|e| matches!(e, Event::Shallows { .. } | Event::Flooded { .. })),
        );
    }
    told
}

#[test]
fn only_the_bar_can_be_waded_in_low_water() {
    let w = world(SNOWY, YEAR);
    let bar: Vec<_> = w.flood().shallows().collect();
    assert_eq!(bar.len(), BAR.len() * DEEP.len(), "{bar:?}");
    assert!(
        bar.iter()
            .all(|&(x, y)| BAR.contains(&x) && DEEP.contains(&y))
    );
}

#[test]
fn a_dry_summer_opens_the_summer_fords_and_the_autumn_shuts_them() {
    let mut w = world(SNOWY, YEAR);
    // A regiment can't cross in the spring.
    let a = spawn(&mut w, 43, 22);
    order(&mut w, a, 43, 42);
    run_to(&mut w, 90 * SECOND);
    assert!(at(&w, a).y < Fx::from_int(DEEP.start), "{:?}", at(&w, a));
    // In the summer the water drops and the bar opens.
    let told = run_to(&mut w, 2 * MINUTE + 70 * SECOND);
    assert_eq!(w.season().unwrap().season, Season::Summer);
    assert_eq!(told, [Event::Shallows { open: true }]);
    assert_eq!(w.terrain().low(), LOW_DROP);
    assert!(w.flood().exposed());
    // Now it wades across, over the bar.
    order(&mut w, a, 43, 42);
    run_to(&mut w, 4 * MINUTE - 10 * SECOND);
    assert!(at(&w, a).y > Fx::from_int(RIVER.end), "{:?}", at(&w, a));
    // The autumn brings the water back, and the bar is deep again.
    let told = run_to(&mut w, 5 * MINUTE);
    assert_eq!(told, [Event::Shallows { open: false }]);
    assert_eq!(w.terrain().low(), 0);
}

#[test]
fn the_winter_snow_melts_in_the_spring_and_shuts_the_fords() {
    let mut w = world(SNOWY, YEAR);
    run_to(&mut w, 8 * MINUTE);
    assert!(w.flood().snowpack() >= MELT_MIN, "{}", w.flood().snowpack());
    let told = run_to(&mut w, 9 * MINUTE);
    assert!(told.contains(&Event::Flooded { closed: true }), "{told:?}");
    // The melt runs out, the water goes down and the fords open again.
    let told = run_to(&mut w, 11 * MINUTE);
    assert!(told.contains(&Event::Flooded { closed: false }), "{told:?}");
    assert_eq!(w.flood().snowpack(), 0);
}

#[test]
fn no_snow_no_melt_and_no_seasons_no_change() {
    // A fair winter leaves nothing to melt.
    const FAIR: &[Spell] = &[Spell {
        at: 0,
        weather: Weather::FAIR,
    }];
    let mut w = world(FAIR, YEAR);
    let told = run_to(&mut w, 10 * MINUTE);
    assert!(!told.contains(&Event::Flooded { closed: true }), "{told:?}");
    // With no seasons the water never drops, however dry.
    let mut w = world(FAIR, 0);
    run_to(&mut w, 5 * MINUTE);
    assert_eq!(w.terrain().low(), 0);
    assert!(!w.flood().exposed());
}

#[test]
fn those_left_on_the_bar_wade_out_when_the_water_comes_back() {
    let mut w = world(SNOWY, YEAR);
    run_to(&mut w, 2 * MINUTE + 70 * SECOND);
    assert!(w.flood().exposed());
    // A regiment halts in the middle of the bar and stays there.
    let a = spawn(&mut w, 43, 31);
    run_to(&mut w, 5 * MINUTE + 30 * SECOND);
    assert!(!w.flood().exposed());
    let y = at(&w, a).y;
    assert!(
        y < Fx::from_int(DEEP.start) || y >= Fx::from_int(DEEP.end),
        "{:?}",
        at(&w, a)
    );
}
