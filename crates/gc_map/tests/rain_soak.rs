//! Rain soaks the ground (docs/32-changing-land.md, "Rain soaks the
//! ground"): on real maps there is low ground by the water to soak, rain
//! turns it soft nearest the water first, and a dry spell firms it up.

use gc_map::{FIELD_MAPS, FieldMap, GROWN, Map};
use gc_sim::{Capacities, MatchSetup, Sky, Spell, Weather, WeatherKind, World};

fn world(map: &Map, nav: &[u8], spells: &[Spell]) -> World {
    World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities { regiments: 8 },
        units: &[],
        map_tiles: map.tiles,
        nav,
        teams: &[],
        ground: map.ground(),
        champions: false,
        sky: Sky {
            spells,
            ..Sky::STILL
        },
        generals: false,
    })
}

fn map(choice: usize, seed: u64) -> Map {
    FieldMap {
        choice,
        tiles: 192,
        seed,
    }
    .make()
}

/// How many tiles soak through in long, heavy rain.
fn soakable(map: &Map) -> usize {
    let rain = [Spell {
        at: 0,
        weather: Weather {
            kind: WeatherKind::Rain,
            strength: 255,
            ..Weather::FAIR
        },
    }];
    let nav = map.sim_nav_costs();
    let mut w = world(map, &nav, &rain);
    for _ in 0..20 * 60 * 8 {
        w.tick(&[]);
    }
    w.terrain().soaked() as usize
}

#[test]
#[ignore = "a survey: cargo test -p gc_map --release --test rain_soak -- --ignored --nocapture"]
fn how_much_ground_each_map_soaks() {
    for (choice, name) in FIELD_MAPS.iter().enumerate() {
        let counts: Vec<usize> = (1..=3).map(|seed| soakable(&map(choice, seed))).collect();
        println!("{name:<16} {counts:?}");
    }
}

#[test]
fn rain_soaks_the_banks_nearest_the_water_first_and_a_dry_spell_firms_them() {
    // Grown land, as a new lobby starts on.
    let map = map(GROWN, 1);
    let nav = map.sim_nav_costs();
    let minute = 20 * 60;
    let spells = [
        Spell {
            at: 0,
            weather: Weather {
                kind: WeatherKind::Rain,
                strength: 200,
                ..Weather::FAIR
            },
        },
        Spell {
            at: 8 * minute,
            weather: Weather::FAIR,
        },
    ];
    let mut w = world(&map, &nav, &spells);
    let mut soaked = Vec::new();
    for t in 0..20 * minute {
        w.tick(&[]);
        if t % minute == 0 {
            soaked.push(w.terrain().soaked());
        }
        if t == 8 * minute {
            // Each soaked tile is soft: slow going, and no charges.
            assert!(w.terrain().soaked_tiles().all(|(x, y)| {
                let at = |v: i32| gc_fixed::Fx::from_int(v) + gc_fixed::Fx::HALF;
                w.terrain().soft(gc_fixed::FxVec2::new(at(x), at(y)))
            }));
        }
    }
    println!("soaked by the minute: {soaked:?}");
    assert_eq!(soaked[0], 0, "dry at first");
    assert!(soaked[1] == 0, "a minute's rain soaks nothing yet");
    let most = *soaked.iter().max().unwrap();
    assert!(most > 50, "long rain soaks the banks: {most}");
    assert!(soaked[3] < most, "and spreads as it goes on");
    assert!(soaked[19] < most, "the dry spell firms it up");
}
