//! Field works (docs/26-fortifications.md, "Field works"): stakes slow
//! whoever walks through them and stop a heavy charge, spearmen and
//! archers plant their own, and barricades are walls troops shelter
//! behind.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::fieldworks::{PLANT_TICKS, STAKE_PACE_PERCENT};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::structures::{Armour, LINE, Role, WallTwist};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Mass, MatchSetup,
    PlayerCommand, RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const WORKER: u16 = 0;
const SPEARS: u16 = 1;
const RIDERS: u16 = 2;
const SWORDS: u16 = 3;
const HALL: u16 = 0;
const STAKES: u16 = 1;
const BARRICADE: u16 = 2;
const MAP: u32 = 64;
/// The row the works are drawn along, west to east.
const ROW: i32 = 30;

fn world() -> World {
    let worker = UnitStats {
        models: 1,
        files: 1,
        ..DRILL_UNITS[0]
    };
    let spears = UnitStats {
        reach: true,
        ..DRILL_UNITS[0]
    };
    let riders = UnitStats {
        mass: Mass::Heavy,
        speed: Fx::from_int(6),
        models: 12,
        files: 6,
        hp_per_model: 30,
        ..DRILL_UNITS[1]
    };
    let side = (MAP as i32 * gc_sim::nav::CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        map_tiles: MAP,
        capacities: Capacities { regiments: 16 },
        units: &[worker, spears, riders, DRILL_UNITS[0]],
        seed: 3,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    });
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 100,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 1000,
        armour: Armour::Fortified,
        width: 3,
        depth: 3,
        supply: 0,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    let stakes = BuildingStats {
        role: Role::Stakes,
        cost_grub: 4,
        build_ticks: 10,
        hp: 120,
        armour: Armour::Structure,
        width: 1,
        depth: 1,
        flags: LINE,
        ..hall
    };
    let barricade = BuildingStats {
        role: Role::Barricade,
        cost_grub: 8,
        hp: 250,
        ..stakes
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (2000, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall, stakes, barricade],
        units: &[
            ConquestUnit {
                worker: true,
                ..ConquestUnit::default()
            },
            ConquestUnit::default(),
            ConquestUnit::default(),
            ConquestUnit::default(),
        ],
        structures: 512,
        start,
        ..ConquestSetup::default()
    }));
    for (player, tile) in [(0, (2, 2)), (1, (58, 58))] {
        order(
            &mut w,
            player,
            Command::SpawnStructure {
                building: HALL,
                tile,
                turn: 0,
            },
        );
    }
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: u16) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: Angle(facing),
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

/// Put up finished tiles of `building` for side 0 along `ROW`, `xs`.
fn line(w: &mut World, building: u16, xs: core::ops::RangeInclusive<i32>) {
    for x in xs {
        assert!(order(
            w,
            0,
            Command::SpawnStructure {
                building,
                tile: (x as i16, ROW as i16),
                turn: 0,
            }
        ));
    }
    w.tick(&[]);
}

fn centre(x: i32) -> FxVec2 {
    FxVec2::new(Fx::from_ratio(2 * x + 1, 2), Fx::from_ratio(2 * ROW + 1, 2))
}

fn move_to(w: &mut World, player: u8, r: RegimentHandle, x: i32, y: i32) {
    assert!(order(
        w,
        player,
        Command::MoveTo {
            regiment: r,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
}

#[test]
fn a_worker_draws_stakes_that_are_walked_through_slowly_and_bar_charges() {
    let mut w = world();
    let worker = spawn(&mut w, 0, WORKER, 20, ROW - 2, 0);
    assert!(order(
        &mut w,
        0,
        Command::BuildWall {
            regiment: Some(worker),
            building: STAKES,
            from: (20, ROW as i16),
            to: (27, ROW as i16),
        }
    ));
    // Laid, not yet up: walkable all the same, but not yet stakes.
    assert!(w.nav().walkable(0, centre(22)));
    assert!(!w.terrain().staked(centre(22)));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let s = w.structures().unwrap();
    assert!(
        s.iter()
            .filter(|v| v.building == STAKES)
            .all(|v| v.finished()),
        "all up"
    );
    let t = w.terrain();
    assert!(t.staked(centre(22)) && w.nav().walkable(1, centre(22)));
    assert!(!t.bears_charge(centre(22)));
    let step = Fx::from_int(1);
    assert_eq!(
        t.going(
            step,
            centre(22),
            FxVec2::new(Fx::ZERO, Fx::ONE),
            Mass::Medium
        ),
        step * Fx::from_ratio(STAKE_PACE_PERCENT, 100)
    );
}

#[test]
fn riders_charging_into_stakes_are_stopped_and_hurt() {
    let mut w = world();
    line(&mut w, STAKES, 20..=34);
    let riders = spawn(&mut w, 1, RIDERS, 27, ROW + 12, 0xC000);
    let hp = |w: &World| -> u32 {
        (w.regiments.model_hp(riders).unwrap().iter())
            .map(|&h| u32::from(h))
            .sum()
    };
    let before = hp(&w);
    let stakes_hp = |w: &World| -> u32 {
        (w.structures().unwrap().iter())
            .filter(|v| v.building == STAKES)
            .map(|v| v.hp)
            .sum()
    };
    let stakes_before = stakes_hp(&w);
    move_to(&mut w, 1, riders, 27, ROW - 12);
    let mut shaken = false;
    for _ in 0..12 * TICKS_PER_SECOND {
        w.tick(&[]);
        let r = w.regiments.iter().find(|r| r.handle == riders).unwrap();
        shaken |= r.disorder > 0;
    }
    assert!(hp(&w) < before, "hurt on the stakes");
    let r = w.regiments.iter().find(|r| r.handle == riders).unwrap();
    assert!(
        r.position.y > Fx::from_int(ROW),
        "stopped short: {:?}",
        r.position
    );
    assert!(shaken, "thrown into disorder");
    assert!(stakes_hp(&w) < stakes_before, "the stakes wear");
}

#[test]
fn spearmen_plant_stakes_across_their_front_and_a_move_calls_it_off() {
    let mut w = world();
    let spears = spawn(&mut w, 0, SPEARS, 20, ROW, 0x4000);
    let swords = spawn(&mut w, 0, SWORDS, 40, ROW, 0x4000);
    assert!(w.plant_check(swords, STAKES).is_err(), "swordsmen don't");
    assert!(w.plant_check(spears, HALL).is_err(), "only stakes");
    assert!(order(
        &mut w,
        0,
        Command::PlantStakes {
            regiment: spears,
            building: STAKES,
        }
    ));
    let count = |w: &World| {
        (w.structures().unwrap().iter())
            .filter(|v| v.building == STAKES && v.finished())
            .count()
    };
    for _ in 0..PLANT_TICKS - 2 {
        w.tick(&[]);
    }
    assert_eq!(count(&w), 0, "still hammering");
    for _ in 0..4 {
        w.tick(&[]);
    }
    let planted: Vec<(i32, i32)> = (w.structures().unwrap().iter())
        .filter(|v| v.building == STAKES)
        .map(|v| v.tile)
        .collect();
    assert!(planted.len() >= 4, "{planted:?}");
    // In front of it (it faces +y), not under it.
    assert!(planted.iter().all(|&(_, y)| y > ROW), "{planted:?}");
    assert_eq!(w.bank(0).unwrap().grub, 2000, "free");
    // A second regiment told to move before it is done plants nothing.
    let more = spawn(&mut w, 0, SPEARS, 40, ROW - 10, 0x4000);
    assert!(order(
        &mut w,
        0,
        Command::PlantStakes {
            regiment: more,
            building: STAKES,
        }
    ));
    move_to(&mut w, 0, more, 50, ROW - 10);
    let before = count(&w);
    for _ in 0..PLANT_TICKS + 20 {
        w.tick(&[]);
    }
    assert_eq!(count(&w), before);
}

#[test]
fn a_barricade_is_a_wall_its_own_side_shelters_behind() {
    let mut w = world();
    line(&mut w, BARRICADE, 20..=28);
    assert!(!w.nav().walkable(0, centre(24)), "nobody walks through");
    let behind = FxVec2::new(Fx::from_ratio(49, 2), Fx::from_int(ROW - 1));
    assert!(w.terrain().sheltered(0, behind));
    assert!(!w.terrain().sheltered(1, behind), "only its own side");
    let far = FxVec2::new(Fx::from_ratio(49, 2), Fx::from_int(ROW - 6));
    assert!(!w.terrain().sheltered(0, far));
}
