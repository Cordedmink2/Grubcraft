//! What the sky does in a fight: fog, rain and night shorten sight and
//! archers' range, snow slows the march (docs/32-changing-land.md,
//! "Weather and the time of day").

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::sky::at_hour;
use gc_sim::{
    Air, Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, Sky, Spell,
    UnitStats, Weather, WeatherKind, World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const ARCHERS: u16 = 2;

fn fearless() -> [UnitStats; 4] {
    DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u })
}

/// The whole match under `weather` (at full strength), at `hour`.
fn under(kind: WeatherKind, hour: u32) -> (u32, Vec<Spell>) {
    let spells = match kind {
        WeatherKind::Clear => Vec::new(),
        _ => vec![Spell {
            at: 0,
            weather: Weather {
                kind,
                strength: 255,
                ..Weather::FAIR
            },
        }],
    };
    (at_hour(hour, 0), spells)
}

fn world(units: &[UnitStats], sky: Sky) -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units,
        map_tiles: 64,
        nav: &[],
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        // Generals off: these tests count wounds and paces.
        generals: false,
        sky,
    })
}

fn sky(start: u32, spells: &[Spell]) -> Sky<'_> {
    Sky {
        start,
        spells,
        ..Sky::STILL
    }
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::new(tile(x), tile(y)),
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

fn tile(n: i32) -> Fx {
    Fx::from_int(n) + Fx::HALF
}

fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// What archers 20 tiles from a standing target take off it in ten
/// seconds (they reach 25 in clear daylight).
fn wounds(kind: WeatherKind, hour: u32) -> u32 {
    let (start, spells) = under(kind, hour);
    let mut w = world(&fearless(), sky(start, &spells));
    let _archers = spawn(&mut w, 0, ARCHERS, 32, 10, SOUTH);
    let target = spawn(&mut w, 1, SPEARS, 32, 30, NORTH);
    let before = hp(&w, target);
    run(&mut w, 20 * 10);
    before - hp(&w, target)
}

#[test]
fn fog_and_the_dark_keep_archers_from_reaching_and_rain_does_not() {
    assert!(wounds(WeatherKind::Clear, 12) > 0, "in range by day");
    assert!(
        wounds(WeatherKind::Rain, 12) > 0,
        "rain takes a fifth: 20 left"
    );
    assert_eq!(wounds(WeatherKind::Fog, 12), 0, "fog takes 60%: 10 left");
    assert_eq!(
        wounds(WeatherKind::Clear, 0),
        0,
        "night leaves 45%: 11 left"
    );
}

#[test]
fn snow_slows_the_march() {
    let walked = |kind| {
        let (start, spells) = under(kind, 12);
        let mut w = world(&fearless(), sky(start, &spells));
        let r = spawn(&mut w, 0, SPEARS, 32, 60, NORTH);
        w.tick(&[PlayerCommand {
            player: 0,
            command: Command::MoveTo {
                regiment: r,
                target: FxVec2::new(tile(32), tile(40)),
                facing: None,
                files: 0,
                pace: Fx::ZERO,
            },
        }]);
        run(&mut w, 40);
        tile(60) - w.regiments.get(r).unwrap().position.y
    };
    let (clear, snow) = (walked(WeatherKind::Clear), walked(WeatherKind::Snow));
    assert!(
        snow * 100 <= clear * 88 && snow * 100 >= clear * 82,
        "snow {snow:?} vs clear {clear:?}"
    );
    assert_eq!(walked(WeatherKind::Fog), clear, "fog doesn't slow anyone");
}

#[test]
fn the_air_follows_the_clock_through_dusk() {
    let mut w = world(
        &fearless(),
        Sky {
            start: at_hour(18, 0),
            runs: true,
            ..Sky::STILL
        },
    );
    assert_eq!(w.air(), Air::CLEAR);
    let mut last = 100;
    // An hour and a half of game day: well into the night.
    for _ in 0..at_hour(1, 30) {
        w.tick(&[]);
        let now = w.air().sight_percent;
        assert!(now <= last, "the light only fades at dusk");
        last = now;
    }
    assert_eq!(last, 45, "dark at half past seven");
    assert!(!w.daylight().rising());
}

#[test]
fn the_sky_is_part_of_the_state_and_the_old_sky_hashes_as_before() {
    let units = fearless();
    let still = world(&units, Sky::STILL);
    let running = world(
        &units,
        Sky {
            runs: true,
            ..Sky::STILL
        },
    );
    assert_ne!(still.state_hash(), running.state_hash());
    let (start, spells) = under(WeatherKind::Fog, 12);
    let a = world(&units, sky(start, &spells));
    let b = world(&units, sky(start, &spells));
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.weather().kind, WeatherKind::Fog);
}
