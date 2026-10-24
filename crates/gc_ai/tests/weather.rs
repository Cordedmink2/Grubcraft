//! The computer players read the sky (docs/32-changing-land.md, "Weather
//! and the time of day"): archers' reach as fog, rain and night leave it,
//! the march as snow leaves it, and mud underfoot.

use gc_ai::ground::Line;
use gc_ai::{Perception, Terrain};
use gc_map::{FieldMap, GROWN};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{
    Capacities, Ground, MatchSetup, Sky, Spell, TICKS_PER_SECOND, Weather, WeatherKind, World,
};
use glam::Vec2;

const ARCHERS: usize = 2;
/// Six minutes of heavy rain: long enough to soak the banks through.
const RAIN_TICKS: u32 = TICKS_PER_SECOND * 60 * 6;

fn spell(kind: WeatherKind) -> [Spell; 1] {
    [Spell {
        at: 0,
        weather: Weather {
            kind,
            strength: 255,
            ..Weather::FAIR
        },
    }]
}

fn world<'a>(ground: Ground<'a>, nav: &'a [u8], tiles: u32, spells: &'a [Spell]) -> World {
    World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities { regiments: 8 },
        units: &DRILL_UNITS,
        map_tiles: tiles,
        nav,
        teams: &[],
        ground,
        champions: false,
        sky: Sky {
            spells,
            ..Sky::STILL
        },
        generals: false,
    })
}

#[test]
fn fog_shortens_the_archers_the_minds_see_and_snow_their_march() {
    let read = |kind| {
        let spells = spell(kind);
        let w = world(Ground::FLAT, &[], 64, &spells);
        let mut view = Perception::new();
        view.capture(&w);
        (view.units[ARCHERS].range, view.units[ARCHERS].speed)
    };
    let (clear_range, clear_speed) = read(WeatherKind::Clear);
    let (fog_range, _) = read(WeatherKind::Fog);
    let (_, snow_speed) = read(WeatherKind::Snow);
    assert!(
        (fog_range / clear_range - 0.4).abs() < 0.01,
        "{fog_range} of {clear_range}"
    );
    assert!(
        (snow_speed / clear_speed - 0.85).abs() < 0.01,
        "{snow_speed} of {clear_speed}"
    );
}

#[test]
fn the_minds_ground_follows_the_mud_the_rain_soaks() {
    let map = FieldMap {
        choice: GROWN,
        tiles: 192,
        seed: 1,
    }
    .make();
    let nav = map.sim_nav_costs();
    let spells = spell(WeatherKind::Rain);
    let mut w = world(map.ground(), &nav, map.tiles, &spells);
    let n = map.tiles;
    let soft_tiles = |t: &Terrain| (0..n * n).filter(|i| t.soft(centre(i % n, i / n))).count();
    let mut minds = Terrain::new(&nav, n);
    assert!(minds.stale(w.terrain()));
    minds.follow(w.terrain());
    let soft_before = soft_tiles(&minds);
    for _ in 0..RAIN_TICKS {
        w.tick(&[]);
    }
    assert!(w.terrain().soaked() > 0);
    assert!(minds.stale(w.terrain()));
    minds.follow(w.terrain());
    assert!(!minds.stale(w.terrain()));
    assert_eq!(
        soft_tiles(&minds),
        soft_before + w.terrain().soaked() as usize
    );
    // A battle line on the soaked bank has worse footing than it did dry.
    let (x, y) = w.terrain().soaked_tiles().next().unwrap();
    let line = Line {
        centre: centre(x as u32, y as u32),
        forward: Vec2::Y,
        width: 4.0,
        depth: 2.0,
    };
    let mut dry = Terrain::new(&nav, n);
    dry.follow(world(map.ground(), &nav, n, &spells).terrain());
    assert!(line.footing(&minds, 0.0) < line.footing(&dry, 0.0));
}

fn centre(x: u32, y: u32) -> Vec2 {
    Vec2::new(x as f32 + 0.5, y as f32 + 0.5)
}
