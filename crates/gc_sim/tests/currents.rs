//! Living waterways, stage 2 (docs/38-living-waterways.md): rivers run one
//! way. Wading troops are carried down a little as they cross, and march
//! slower up a river than down it. Still water (no flow layer) is as
//! before.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{
    Capacities, Command, Flow, Ground, MatchSetup, PlayerCommand, RegimentHandle, Sky, World,
};

const SOUTH: Angle = Angle(0x4000);
const EAST: Angle = Angle(0);
const MAP: i32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const WATER: u16 = 256;
/// A wide ford, all of it wadeable.
const RIVER: core::ops::Range<i32> = 24..40;

fn heights() -> Vec<u16> {
    (0..MAP * MAP)
        .map(|i| {
            if RIVER.contains(&(i / MAP)) {
                WATER - 40
            } else {
                600
            }
        })
        .collect()
}

fn nav() -> Vec<u8> {
    let side = MAP * CELLS_PER_TILE;
    (0..side * side)
        .map(|c| {
            if RIVER.contains(&(c / side / CELLS_PER_TILE)) {
                3
            } else {
                1
            }
        })
        .collect()
}

/// The river running east at `speed` (0: still).
fn flow(speed: u8) -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            if RIVER.contains(&(i / MAP)) {
                Flow::byte(EAST, speed)
            } else {
                0
            }
        })
        .collect()
}

fn world(flow: &[u8]) -> World {
    let (heights, nav) = (heights(), nav());
    World::new(&MatchSetup {
        seed: 5,
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
            flow,
        },
        champions: false,
        sky: Sky::STILL,
        generals: false,
    })
}

fn spawn(w: &mut World, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF),
            facing,
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

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Where a regiment crossing the river from north to south is after
/// `secs`, in the river at `speed`.
fn crossing(speed: u8, secs: u32) -> FxVec2 {
    let mut w = world(&flow(speed));
    let a = spawn(&mut w, 20, 20, SOUTH);
    order(&mut w, a, 20, 44);
    run(&mut w, secs * SECOND);
    at(&w, a)
}

#[test]
fn the_current_carries_a_crossing_regiment_downstream() {
    let still = crossing(0, 6);
    let fast = crossing(3, 6);
    assert!(
        still.y > Fx::from_int(RIVER.start + 2),
        "still in the river: {still:?}"
    );
    assert!(still.y < Fx::from_int(RIVER.end), "{still:?}");
    // Carried east of its straight line, by less than the drift a second
    // would give over the whole crossing.
    let drift = fast.x - still.x;
    assert!(drift > Fx::from_ratio(1, 4), "{still:?} {fast:?}");
    assert!(drift < Fx::from_int(5), "{still:?} {fast:?}");
}

/// Ticks a regiment wading from `from` to `to` along the river takes.
fn wade(speed: u8, from: i32, to: i32) -> u32 {
    let mut w = world(&flow(speed));
    let facing = if to > from { EAST } else { Angle(0x8000) };
    let a = spawn(&mut w, from, 32, facing);
    order(&mut w, a, to, 32);
    for t in 0..120 * SECOND {
        w.tick(&[]);
        if (at(&w, a).x - Fx::from_int(to) - Fx::HALF).abs() < Fx::from_ratio(1, 10) {
            return t;
        }
    }
    panic!("never got there: {:?}", at(&w, a));
}

#[test]
fn wading_up_a_river_is_slower_than_down_it() {
    let still = wade(0, 10, 50);
    let down = wade(3, 10, 50);
    let up = wade(3, 50, 10);
    assert!(down < still, "{down} {still}");
    assert!(up > still, "{up} {still}");
}

#[test]
fn a_regiment_holding_in_the_river_is_not_carried() {
    let mut w = world(&flow(3));
    let a = spawn(&mut w, 30, 32, SOUTH);
    let before = at(&w, a);
    run(&mut w, 20 * SECOND);
    assert_eq!(at(&w, a), before);
}

#[test]
fn the_flow_byte_round_trips_and_turns() {
    for k in 0..8u16 {
        let b = Flow::byte(Angle(k << 13), 2);
        assert_eq!(Flow::of(b), Some((Angle(k << 13), 2)));
        assert_eq!(
            Flow::of(Flow::turned(b)),
            Some((Angle(((k + 4) & 7) << 13), 2))
        );
    }
    assert_eq!(Flow::byte(EAST, 0), 0);
    assert_eq!(Flow::of(0), None);
    // Still water everywhere is no layer at all.
    assert!(Flow::layer(&flow(0), MAP as u32).is_empty());
}
