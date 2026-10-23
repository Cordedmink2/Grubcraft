//! Clearing land (idea 22, docs/32-changing-land.md): workers clear scrub,
//! woods and thicket, buildings wait for their ground to be cleared, and
//! cleared ground is open.

use gc_fixed::FxVec2;
use gc_sim::clearing::{MAX_CLEAR_SIDE, clear_ticks, timber};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Cover, Ground, Growth,
    MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const WORKER: u16 = 0;
const SPEARS: u16 = 1;
const HOUSE: u16 = 0;
const HALL: u16 = 1;
const MAP: u32 = 64;
const START_GRUB: u32 = 2000;

/// Woods on 20..24 × 20..24, scrub (under tall grass) on 30..33 × 20..23,
/// and a wall of thicket down the map at x 44..46.
fn ground() -> (Vec<u8>, Vec<u8>) {
    let n = MAP as usize;
    let mut cover = vec![Cover::Open as u8; n * n];
    let mut growth = vec![Growth::Open as u8; n * n];
    let mut patch =
        |xs: core::ops::Range<usize>, ys: core::ops::Range<usize>, c: Cover, g: Growth| {
            for y in ys {
                for x in xs.clone() {
                    cover[y * n + x] = c as u8;
                    growth[y * n + x] = g as u8;
                }
            }
        };
    patch(20..24, 20..24, Cover::Wood, Growth::Woods);
    patch(30..33, 20..23, Cover::Grass, Growth::Scrub);
    patch(44..46, 0..n, Cover::Wood, Growth::Thicket);
    (cover, growth)
}

fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let (cover, growth) = ground();
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 32 },
        units: &[worker, DRILL_UNITS[0]],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &growth,
        },
        champions: false,
    });
    let house = BuildingStats {
        role: Role::Housing,
        tier: 1,
        upgrades_from: None,
        cost_grub: 60,
        cost_marrow: 0,
        build_ticks: 100,
        hp: 600,
        armour: Armour::Structure,
        width: 2,
        depth: 2,
        supply: 10,
        flags: DROP_OFF_GRUB,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (START_GRUB, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            house,
            BuildingStats {
                role: Role::Main,
                width: 3,
                depth: 3,
                ..house
            },
        ],
        units: &[
            ConquestUnit {
                worker: true,
                supply: 1,
                tier: 1,
                ..ConquestUnit::default()
            },
            ConquestUnit::default(),
        ],
        structures: 16,
        start,
        ..ConquestSetup::default()
    }));
    // A hall, for the tier houses need.
    assert!(order(
        &mut w,
        Command::SpawnStructure {
            building: HALL,
            tile: (2, 2),
            turn: 0,
        }
    ));
    w
}

fn order(w: &mut World, command: Command) -> bool {
    w.tick(&[PlayerCommand { player: 0, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn grub(w: &World) -> u32 {
    w.bank(0).unwrap().grub
}

fn clear(w: &mut World, r: RegimentHandle, tile: (i16, i16), size: (u8, u8)) -> bool {
    order(
        w,
        Command::Clear {
            regiment: r,
            tile,
            size,
        },
    )
}

/// Ticks until `done`, up to `most`.
fn until(w: &mut World, most: u32, done: impl Fn(&World) -> bool) -> u32 {
    for t in 1..=most {
        w.tick(&[]);
        if done(w) {
            return t;
        }
    }
    panic!("not done in {most} ticks");
}

fn all_open(w: &World, [x, y, wide, deep]: [i32; 4]) -> bool {
    (y..y + deep).all(|ty| (x..x + wide).all(|tx| w.terrain().growth(tx, ty) == Growth::Open))
}

#[test]
fn a_worker_clears_an_area_of_scrub() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 28, 18);
    assert!(clear(&mut w, worker, (30, 20), (3, 3)));
    let ticks = until(&mut w, 4000, |w| all_open(w, [30, 20, 3, 3]));
    // Nine tiles of scrub, one at a time, and the walks between them.
    let work = 9 * clear_ticks(Growth::Scrub);
    assert!(
        ticks >= work && ticks < work + 20 * TICKS_PER_SECOND,
        "{ticks}"
    );
    // The tall grass went with it, and scrub pays no timber.
    assert_eq!(w.terrain().cover(FxVec2::from_ints(31, 21)), Cover::Open);
    assert_eq!(grub(&w), START_GRUB);
    // Done: the worker is free again.
    w.tick(&[]);
    assert!(w.clearing_of(worker).is_none());
}

#[test]
fn two_workers_share_the_work() {
    let one = {
        let mut w = world();
        let a = spawn(&mut w, WORKER, 18, 18);
        assert!(clear(&mut w, a, (20, 20), (4, 4)));
        until(&mut w, 10_000, |w| all_open(w, [20, 20, 4, 4]))
    };
    let two = {
        let mut w = world();
        let a = spawn(&mut w, WORKER, 18, 18);
        let b = spawn(&mut w, WORKER, 25, 25);
        assert!(clear(&mut w, a, (20, 20), (4, 4)));
        assert!(clear(&mut w, b, (20, 20), (4, 4)));
        until(&mut w, 10_000, |w| all_open(w, [20, 20, 4, 4]))
    };
    assert!(two * 10 < one * 7, "one {one}, two {two}");
}

#[test]
fn felled_woods_pay_timber() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 18, 18);
    assert!(clear(&mut w, worker, (20, 20), (2, 1)));
    until(&mut w, 4000, |w| all_open(w, [20, 20, 2, 1]));
    assert_eq!(grub(&w), START_GRUB + 2 * timber(Growth::Woods));
    assert_eq!(w.terrain().cover(FxVec2::from_ints(20, 20)), Cover::Open);
    // The rest of the wood still stands.
    assert_eq!(w.terrain().growth(22, 20), Growth::Woods);
}

#[test]
fn clear_orders_are_refused_where_nothing_grows_or_for_soldiers() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 10, 10);
    let spears = spawn(&mut w, SPEARS, 12, 10);
    assert!(!clear(&mut w, worker, (5, 5), (4, 4)));
    assert!(!clear(&mut w, spears, (20, 20), (2, 2)));
    let too_big = (MAX_CLEAR_SIDE + 1) as u8;
    assert!(!clear(&mut w, worker, (20, 20), (too_big, 1)));
    assert!(!clear(&mut w, worker, (20, 20), (0, 1)));
    assert!(clear(&mut w, worker, (20, 20), (2, 2)));
}

#[test]
fn a_building_on_woods_waits_for_its_ground_to_be_cleared() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 18, 18);
    assert!(w.overgrown([20, 20, 2, 2]));
    assert_eq!(w.can_lay(0, HOUSE, (20, 20), 0), Ok(()));
    assert!(order(
        &mut w,
        Command::Build {
            regiment: worker,
            building: HOUSE,
            tile: (20, 20),
            turn: 0,
        }
    ));
    // Paid for now, but nothing laid yet: the ground comes first.
    assert_eq!(grub(&w), START_GRUB - 60);
    assert_eq!(w.structures().unwrap().iter().count(), 1);
    assert_eq!(w.pledged().collect::<Vec<_>>(), [(0, HOUSE, (20, 20), 0)]);
    let job = w.clearing_of(worker).unwrap();
    assert_eq!(
        (job.area, job.then),
        ([20, 20, 2, 2], Some((HOUSE, (20, 20), 0)))
    );
    let ticks = until(&mut w, 4000, |w| {
        w.structures().unwrap().iter().count() == 2
    });
    assert!(ticks >= 4 * clear_ticks(Growth::Woods), "{ticks}");
    assert!(all_open(&w, [20, 20, 2, 2]));
    assert_eq!(w.terrain().growth(22, 20), Growth::Woods);
    // The timber, and the worker went on to build it.
    assert_eq!(grub(&w), START_GRUB - 60 + 4 * timber(Growth::Woods));
    assert!(w.pledged().next().is_none());
    until(&mut w, 2000, |w| {
        w.structures().unwrap().iter().all(|s| s.finished())
    });
}

#[test]
fn calling_a_building_off_before_its_ground_is_cleared_pays_it_all_back() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 18, 18);
    assert!(order(
        &mut w,
        Command::Build {
            regiment: worker,
            building: HOUSE,
            tile: (21, 21),
            turn: 0,
        }
    ));
    for _ in 0..(5 * TICKS_PER_SECOND) {
        w.tick(&[]);
    }
    assert_eq!(grub(&w), START_GRUB - 60);
    assert!(order(
        &mut w,
        Command::MoveTo {
            regiment: worker,
            target: FxVec2::from_ints(10, 10),
            facing: None,
            files: 0,
            pace: Default::default(),
        }
    ));
    assert_eq!(grub(&w), START_GRUB);
    assert!(w.pledged().next().is_none());
    assert!(w.clearing_of(worker).is_none());
}

#[test]
fn cleared_thicket_opens_a_way_through() {
    let mut w = world();
    let inside = FxVec2::new(
        gc_fixed::Fx::from_ratio(89, 2),
        gc_fixed::Fx::from_ratio(61, 2),
    );
    assert!(!w.nav().walkable(0, inside));
    let worker = spawn(&mut w, WORKER, 40, 30);
    assert!(clear(&mut w, worker, (44, 29), (2, 3)));
    until(&mut w, 8000, |w| all_open(w, [44, 29, 2, 3]));
    assert!(w.nav().walkable(0, inside));
    // A regiment marches through the gap to the far side.
    let spears = spawn(&mut w, SPEARS, 38, 30);
    let far = FxVec2::from_ints(52, 30);
    assert!(order(
        &mut w,
        Command::MoveTo {
            regiment: spears,
            target: far,
            facing: None,
            files: 0,
            pace: Default::default(),
        }
    ));
    until(&mut w, 4000, |w| {
        let at = w.regiments.get(spears).unwrap().position;
        (at - far).length() < gc_fixed::Fx::from_int(2)
    });
}

#[test]
fn a_building_can_go_up_on_cleared_thicket() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 40, 10);
    assert_eq!(w.can_lay(0, HOUSE, (44, 10), 0), Ok(()));
    assert!(order(
        &mut w,
        Command::Build {
            regiment: worker,
            building: HOUSE,
            tile: (44, 10),
            turn: 0,
        }
    ));
    until(&mut w, 8000, |w| {
        w.structures().unwrap().iter().count() == 2
    });
    assert!(all_open(&w, [44, 10, 2, 2]));
}

#[test]
fn clearing_plays_the_same_every_time_and_shows_in_the_hash() {
    let play = |clearing: bool| {
        let mut w = world();
        let worker = spawn(&mut w, WORKER, 18, 18);
        if clearing {
            assert!(clear(&mut w, worker, (20, 20), (4, 4)));
        }
        (0..600)
            .map(|_| {
                w.tick(&[]);
                w.state_hash()
            })
            .last()
            .unwrap()
    };
    assert_eq!(play(true), play(true));
    assert_ne!(play(true), play(false));
}
