//! Seasons in Conquest (docs/32-changing-land.md, "Seasons"): the year
//! turns from spring to winter, the weather follows it, and the winter
//! freezes the shallows over.

use gc_fixed::{Fx, FxVec2};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::sky::{Season, WeatherKind, season_at, seasonal};
use gc_sim::{Capacities, Ground, MatchSetup, Sky, World};

const MAP: i32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const MINUTE: u32 = 60 * SECOND;
/// Four one-minute seasons.
const YEAR: u32 = 4 * MINUTE;
/// The river's rows.
const RIVER: core::ops::Range<i32> = 30..34;

#[test]
fn the_year_turns_from_spring_to_winter_and_round_again() {
    assert_eq!(season_at(0, 12_345), None);
    let at = |tick| season_at(YEAR, tick).unwrap();
    assert_eq!(at(0).season, Season::Spring);
    assert_eq!(at(MINUTE - 1).season, Season::Spring);
    assert_eq!(at(MINUTE).season, Season::Summer);
    assert_eq!((at(MINUTE).into, at(MINUTE).length), (0, MINUTE));
    assert_eq!(at(2 * MINUTE + 5).season, Season::Autumn);
    assert_eq!(at(2 * MINUTE + 5).into, 5);
    assert_eq!(at(3 * MINUTE).season, Season::Winter);
    let next = at(YEAR + 1);
    assert_eq!((next.season, next.year), (Season::Spring, 1));
}

#[test]
fn the_weather_follows_the_seasons() {
    // Long seasons, so each has a good many spells.
    let year = 4 * 60 * MINUTE;
    let spells = seasonal(7, year, 4 * year);
    let of = |season: Season| -> Vec<WeatherKind> {
        (spells.iter())
            .filter(|s| season_at(year, s.at).unwrap().season == season)
            .map(|s| s.weather.kind)
            .collect()
    };
    let share = |kinds: &[WeatherKind], kind| {
        100 * kinds.iter().filter(|&&k| k == kind).count() / kinds.len().max(1)
    };
    // Snow falls only in the winter, and rain never does.
    let winter = of(Season::Winter);
    assert!(share(&winter, WeatherKind::Snow) > 20, "{winter:?}");
    assert_eq!(share(&winter, WeatherKind::Rain), 0);
    for season in [Season::Spring, Season::Summer, Season::Autumn] {
        assert_eq!(share(&of(season), WeatherKind::Snow), 0, "{season:?}");
    }
    // The summer is mostly clear, and clearer than the spring.
    let summer = share(&of(Season::Summer), WeatherKind::Clear);
    let spring = share(&of(Season::Spring), WeatherKind::Clear);
    assert!(
        summer > 50 && summer > spring,
        "summer {summer}, spring {spring}"
    );
    // A new season starts a new spell: no season's weather runs on.
    let short = 4 * 8 * MINUTE;
    let spells = seasonal(7, short, 4 * short);
    for quarter in 0..16 {
        assert!(
            spells.iter().any(|s| s.at == quarter * short / 4),
            "{quarter}"
        );
    }
    assert!(spells.windows(2).all(|w| w[0].at < w[1].at));
    // The autumn is the foggiest.
    let fog = |s| share(&of(s), WeatherKind::Fog);
    assert!(fog(Season::Autumn) > fog(Season::Spring));
    assert!(fog(Season::Autumn) > fog(Season::Summer));
}

/// A river under the water line across the middle, blocked but for a
/// ford, under four one-minute seasons.
fn world() -> World {
    let heights: Vec<u16> = (0..MAP * MAP)
        .map(|i| if RIVER.contains(&(i / MAP)) { 100 } else { 600 })
        .collect();
    let side = MAP * CELLS_PER_TILE;
    let nav: Vec<u8> = (0..side * side)
        .map(|c| {
            let (x, y) = (c % side / CELLS_PER_TILE, c / side / CELLS_PER_TILE);
            match () {
                _ if !RIVER.contains(&y) => 1,
                _ if (24..40).contains(&x) => 3,
                _ => BLOCKED,
            }
        })
        .collect();
    World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: MAP as u32,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &heights,
            water_level: 256,
            cover: &[],
            growth: &[],
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky: Sky {
            year_ticks: YEAR,
            ..Sky::STILL
        },
        generals: false,
    })
}

#[test]
fn the_winter_freezes_the_shallows_and_the_spring_thaws_them() {
    let mut w = world();
    let ford = FxVec2::new(Fx::from_int(32) + Fx::HALF, Fx::from_int(31) + Fx::HALF);
    assert!(w.terrain().wet(ford));
    let mut froze = None;
    let mut thawed = None;
    let mut told = Vec::new();
    while w.tick_count() < YEAR + 10 * SECOND {
        w.tick(&[]);
        for e in w.events() {
            if let gc_sim::events::Event::Season { season } = *e {
                told.push((season, w.tick_count() / MINUTE));
            }
        }
        match (froze, w.terrain().frozen()) {
            (None, true) => froze = Some(w.tick_count()),
            (Some(_), false) if thawed.is_none() => thawed = Some(w.tick_count()),
            _ => {}
        }
        if froze.is_some() && thawed.is_none() {
            assert!(!w.terrain().wet(ford), "the ice is walked as dry ground");
        }
    }
    // Within a second of the winter coming, and of it going.
    let froze = froze.expect("the winter froze the river");
    let thawed = thawed.expect("the spring thawed it");
    assert!(
        (3 * MINUTE..=3 * MINUTE + SECOND).contains(&froze),
        "{froze}"
    );
    assert!((YEAR..=YEAR + SECOND).contains(&thawed), "{thawed}");
    assert!(w.terrain().wet(ford));
    // Each new season is told of once, the first spring not.
    assert_eq!(
        told,
        [
            (Season::Summer, 1),
            (Season::Autumn, 2),
            (Season::Winter, 3),
            (Season::Spring, 4)
        ]
    );
}
