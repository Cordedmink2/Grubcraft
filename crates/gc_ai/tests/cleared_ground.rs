//! The minds' ground follows the simulation's: thicket a worker clears
//! mid-match stops being a wall to the computer players, and cleared woods
//! stop hiding anyone (docs/32-changing-land.md).

use gc_ai::Terrain;
use gc_fixed::FxVec2;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Cover, Ground, Growth,
    MatchSetup, PlayerCommand, UnitStats, World,
};
use glam::Vec2;

const MAP: u32 = 64;

/// A wall of thicket down the map at x 44..46, and woods on 20..24 × 20..24.
fn ground() -> (Vec<u8>, Vec<u8>) {
    let n = MAP as usize;
    let mut cover = vec![Cover::Open as u8; n * n];
    let mut growth = vec![Growth::Open as u8; n * n];
    for y in 0..n {
        for x in 0..n {
            let g = if (44..46).contains(&x) {
                Growth::Thicket
            } else if (20..24).contains(&x) && (20..24).contains(&y) {
                Growth::Woods
            } else {
                continue;
            };
            cover[y * n + x] = Cover::Wood as u8;
            growth[y * n + x] = g as u8;
        }
    }
    (cover, growth)
}

fn order(w: &mut World, command: Command) -> bool {
    w.tick(&[PlayerCommand { player: 0, command }])
        .commands_applied
        == 1
}

#[test]
fn the_minds_see_cleared_ground() {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let (cover, growth) = ground();
    let worker = UnitStats {
        models: 1,
        files: 1,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 8 },
        units: &[worker],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &growth,
            footing: &[],
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 0,
        cost_marrow: 0,
        build_ticks: 100,
        hp: 600,
        armour: Armour::Structure,
        width: 3,
        depth: 3,
        supply: 10,
        flags: DROP_OFF_GRUB,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
    };
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall],
        units: &[ConquestUnit {
            worker: true,
            supply: 1,
            tier: 1,
            ..ConquestUnit::default()
        }],
        structures: 4,
        ..ConquestSetup::default()
    }));
    let mut minds = Terrain::new(&nav, MAP)
        .with_cover(cover.clone())
        .with_growth(growth.clone());
    let (wall, wood) = (Vec2::new(44.5, 30.5), Vec2::new(21.5, 21.5));
    assert!(minds.blocked(wall));
    assert_eq!(minds.cover(wood), Cover::Wood);
    // The first look reads what is soft underfoot; nothing is cleared yet.
    assert!(minds.stale(w.terrain()));
    minds.follow(w.terrain());
    assert!(!minds.stale(w.terrain()));
    assert!(minds.blocked(wall));
    // Minds told only the cover still see woods go.
    let mut cover_only = Terrain::new(&nav, MAP).with_cover(cover.clone());
    assert_eq!(cover_only.cover(wall), Cover::Wood);

    assert!(order(
        &mut w,
        Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(40, 30),
            facing: NORTH,
        }
    ));
    let h = w.regiments.iter().next().unwrap().handle;
    assert!(order(
        &mut w,
        Command::Clear {
            regiment: h,
            tile: (44, 29),
            size: (2, 3),
        }
    ));
    for _ in 0..8000 {
        if w.terrain().growth(44, 30) == Growth::Open {
            break;
        }
        w.tick(&[]);
    }
    assert_eq!(w.terrain().growth(44, 30), Growth::Open);
    assert!(minds.stale(w.terrain()));
    minds.follow(w.terrain());
    assert!(cover_only.stale(w.terrain()));
    cover_only.follow(w.terrain());
    assert_eq!(cover_only.cover(wall), Cover::Open);
    assert_eq!(cover_only.cover(wood), Cover::Wood);
    assert!(!minds.stale(w.terrain()));
    assert!(!minds.blocked(wall));
    assert!(minds.walkable(wall));
    // The rest of the wall still stands, and the woods weren't touched.
    assert!(minds.blocked(Vec2::new(44.5, 10.5)));
    assert_eq!(minds.cover(wood), Cover::Wood);
    assert_eq!(minds.cover(wall), Cover::Open);
    // The same as ground made afresh from the land as it now is.
    let n = MAP as i32;
    let now: Vec<u8> = (0..n * n)
        .map(|t| w.terrain().growth(t % n, t / n) as u8)
        .collect();
    let fresh = Terrain::new(&nav, MAP).with_growth(now);
    let step = 1.0 / CELLS_PER_TILE as f32;
    for cy in 0..side {
        for cx in 0..side {
            let p = Vec2::new((cx as f32 + 0.5) * step, (cy as f32 + 0.5) * step);
            assert_eq!(minds.walkable(p), fresh.walkable(p), "{p}");
            assert_eq!(minds.blocked(p), fresh.blocked(p), "{p}");
        }
    }
}
