//! Woods that burn (docs/32-changing-land.md, "Woods that burn"): a
//! regiment sets the woods beside it alight, the fire spreads with the
//! wind, dies in the rain, burns troops and lights the night.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::sky::{Spell, Weather, WeatherKind, at_hour};
use gc_sim::{
    Capacities, Command, Cover, Ground, Growth, MatchSetup, PlayerCommand, RegimentHandle, Sky,
    World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const MAP: u32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;

/// Woods on the tiles `x0..x1` by `y0..y1`, open elsewhere.
fn woods(x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                Cover::Wood as u8
            } else {
                Cover::Open as u8
            }
        })
        .collect()
}

fn world(cover: &[u8], sky: Sky<'static>) -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover,
            growth: &[],
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky,
        generals: false,
    })
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player,
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

fn burn(w: &mut World, player: u8, regiment: RegimentHandle) -> bool {
    w.tick(&[PlayerCommand {
        player,
        command: Command::Burn { regiment },
    }])
    .commands_applied
        == 1
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

/// Tiles of `x0..x1` by `y0..y1` with nothing growing on them.
fn burnt(w: &World, x0: i32, x1: i32, y0: i32, y1: i32) -> usize {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if w.terrain().growth(x, y) == Growth::Open {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn a_regiment_sets_the_woods_alight_and_they_burn_to_open_ground() {
    let cover = woods(20, 30, 20, 30);
    let mut w = world(&cover, Sky::STILL);
    // Standing just south of the wood's edge.
    let a = spawn(&mut w, 0, 25, 31, NORTH);
    run(&mut w, SECOND);
    assert!(!burn(&mut w, 1, a), "only its own side");
    assert!(burn(&mut w, 0, a));
    let kindled: Vec<_> = (w.events().iter())
        .filter(|e| matches!(e, Event::Kindled { .. }))
        .collect();
    assert_eq!(kindled.len(), 1, "{:?}", w.events());
    assert_eq!(w.fires().len(), 1);
    // It spreads, and the first tiles burn out to open ground, scorched.
    let mut scorched = 0;
    for _ in 0..60 * SECOND {
        w.tick(&[]);
        scorched += (w.events().iter())
            .filter(|e| matches!(e, Event::Cleared { burnt: true, .. }))
            .count();
    }
    assert!(scorched > 3, "{scorched} burnt out");
    assert!(burnt(&w, 20, 30, 20, 30) >= scorched);
}

#[test]
fn nothing_burns_out_of_reach_or_in_the_wet() {
    let cover = woods(20, 30, 20, 30);
    let mut w = world(&cover, Sky::STILL);
    let a = spawn(&mut w, 0, 25, 45, NORTH);
    run(&mut w, SECOND);
    assert!(!burn(&mut w, 0, a), "no woods in reach");
    // After a while of heavy rain the ground is too wet to catch.
    const RAIN: &[Spell] = &[Spell {
        at: 0,
        weather: Weather {
            kind: WeatherKind::Rain,
            strength: 255,
            cloud: 255,
            wind: (0, 0),
        },
    }];
    let mut wet = world(
        &cover,
        Sky {
            spells: RAIN,
            ..Sky::STILL
        },
    );
    let b = spawn(&mut wet, 0, 25, 31, NORTH);
    run(&mut wet, 45 * SECOND);
    assert!(!burn(&mut wet, 0, b));
}

#[test]
fn the_wind_carries_the_fire_downwind() {
    // A wide wood; fire set in its middle, the wind blowing east.
    const EAST: &[Spell] = &[Spell {
        at: 0,
        weather: Weather {
            kind: WeatherKind::Clear,
            strength: 0,
            cloud: 0,
            wind: (24, 0),
        },
    }];
    let cover = woods(4, 60, 20, 30);
    let mut w = world(
        &cover,
        Sky {
            spells: EAST,
            ..Sky::STILL
        },
    );
    let a = spawn(&mut w, 0, 32, 31, NORTH);
    run(&mut w, SECOND);
    assert!(burn(&mut w, 0, a));
    run(&mut w, 40 * SECOND);
    let (mut east, mut west) = (0, 0);
    for b in w.fires() {
        if b.x > 32 {
            east += 1;
        } else if b.x < 32 {
            west += 1;
        }
    }
    let (east, west) = (
        east + burnt(&w, 33, 60, 20, 30),
        west + burnt(&w, 4, 32, 20, 30),
    );
    assert!(east > 2 * west.max(1), "east {east}, west {west}");
}

#[test]
fn fire_burns_the_troops_in_it_and_shows_them() {
    let cover = woods(20, 30, 20, 30);
    let mut w = world(&cover, Sky::STILL);
    // In the wood: hidden from the enemy out in the open.
    let a = spawn(&mut w, 0, 25, 24, NORTH);
    let _enemy = spawn(&mut w, 1, 25, 50, NORTH);
    run(&mut w, 5 * SECOND);
    assert!(w.hidden(a));
    let full = hp(&w, a);
    assert!(burn(&mut w, 0, a));
    run(&mut w, 3 * SECOND);
    assert!(w.in_fire(a));
    assert!(!w.hidden(a));
    assert!(hp(&w, a) < full);
}

#[test]
fn fire_lights_the_night_round_it() {
    const NIGHT: Sky<'static> = Sky {
        start: at_hour(23, 30),
        ..Sky::STILL
    };
    let cover = woods(20, 30, 20, 30);
    let mut w = world(&cover, NIGHT);
    assert!(w.air().dark);
    // A torch-less enemy in the open, a few tiles off the wood, and too
    // far from the spears to be seen in the dark.
    let _spears = spawn(&mut w, 0, 25, 50, NORTH);
    let enemy = spawn(&mut w, 1, 37, 22, SOUTH);
    let firer = spawn(&mut w, 0, 25, 31, NORTH);
    run(&mut w, 5 * SECOND);
    assert!(!w.sees(0, enemy));
    assert!(burn(&mut w, 0, firer));
    // Once it has spread to the wood's east edge, its light shows them.
    run(&mut w, 30 * SECOND);
    assert!(w.fires().iter().any(|b| b.x >= 27), "the fire spread east");
    assert!(w.sees(0, enemy));
}

#[test]
fn a_tile_burns_once_and_never_on_open_ground() {
    let cover = woods(10, 50, 10, 50);
    let mut w = world(&cover, Sky::STILL);
    let a = spawn(&mut w, 0, 30, 51, NORTH);
    run(&mut w, SECOND);
    assert!(burn(&mut w, 0, a));
    let mut kindled = std::collections::HashMap::new();
    for _ in 0..200 * SECOND {
        w.tick(&[]);
        for e in w.events() {
            if let Event::Kindled { x, y } = *e {
                *kindled.entry((x, y)).or_insert(0) += 1;
            }
        }
        for b in w.fires() {
            assert_ne!(w.terrain().growth(b.x, b.y), Growth::Open, "{b:?}");
        }
    }
    assert!(kindled.len() > 100, "{} tiles caught", kindled.len());
    assert!(kindled.values().all(|&n| n == 1));
}

#[test]
fn summer_woods_burn_faster_and_winter_ones_slower() {
    // One-minute seasons: summer from a minute in, winter from three.
    let cover = woods(4, 60, 4, 60);
    let spread = |year_ticks: u32, wait: u32| {
        let mut w = world(
            &cover,
            Sky {
                year_ticks,
                ..Sky::STILL
            },
        );
        let a = spawn(&mut w, 0, 32, 61, NORTH);
        run(&mut w, wait * SECOND);
        assert!(burn(&mut w, 0, a));
        run(&mut w, 40 * SECOND);
        w.fires().len() + burnt(&w, 4, 60, 4, 60)
    };
    const YEAR: u32 = 4 * 60 * SECOND;
    let (summer, plain) = (spread(YEAR, 65), spread(0, 65));
    assert!(summer > plain, "summer {summer}, no seasons {plain}");
    let (winter, plain) = (spread(YEAR, 185), spread(0, 185));
    assert!(winter < plain, "winter {winter}, no seasons {plain}");
}
