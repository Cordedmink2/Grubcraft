//! Buildings on the field: armies walk round them, regiments knock them
//! down, and the ground is free again once they fall.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, Breaker, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup, PlayerCommand,
    RegimentHandle, StructureHandle, TICKS_PER_SECOND, World,
};

const SPEARS: u16 = 0;
const ARCHERS: u16 = 2;
const FARMSTEAD: u16 = 0;
const HALL: u16 = 1;
const WALL: u16 = 2;

fn building(hp: u16, armour: Armour, width: u8, depth: u8) -> BuildingStats {
    BuildingStats {
        role: Role::Housing,
        tier: 1,
        upgrades_from: None,
        cost_grub: 80,
        cost_marrow: 0,
        build_ticks: 500,
        hp,
        armour,
        width,
        depth,
        supply: 10,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
    }
}

/// A Farmstead, a Clan Hall (Fortified) and a long wall.
fn buildings() -> [BuildingStats; 3] {
    [
        building(600, Armour::Structure, 3, 3),
        BuildingStats {
            role: Role::Main,
            ..building(3000, Armour::Fortified, 5, 5)
        },
        BuildingStats {
            role: Role::Wall,
            ..building(5000, Armour::Fortified, 5, 5)
        },
    ]
}

/// An open map `tiles` across, in Conquest.
fn conquest(tiles: u32, breakers: &[Breaker]) -> World {
    let side = tiles as usize * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 64 },
        units: &DRILL_UNITS,
        map_tiles: tiles,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    });
    let units: Vec<ConquestUnit> = breakers
        .iter()
        .map(|&breaker| ConquestUnit {
            breaker,
            ..ConquestUnit::default()
        })
        .collect();
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &units,
        structures: 32,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    assert!(
        order(
            w,
            player,
            Command::SpawnStructure {
                building,
                tile: (x, y)
            }
        ),
        "building {building} at ({x}, {y}) refused"
    );
    let s = w.structures().unwrap();
    s.iter()
        .find(|v| v.tile == (i32::from(x), i32::from(y)))
        .unwrap()
        .handle
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        }
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn move_to(w: &mut World, player: u8, regiment: RegimentHandle, x: i32, y: i32) {
    assert!(order(
        w,
        player,
        Command::MoveTo {
            regiment,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
}

fn inside(p: FxVec2, (x, y): (i32, i32), w: i32, d: i32) -> bool {
    p.x > Fx::from_int(x)
        && p.y > Fx::from_int(y)
        && p.x < Fx::from_int(x + w)
        && p.y < Fx::from_int(y + d)
}

#[test]
fn armies_march_round_a_building() {
    let mut w = conquest(64, &[]);
    // A wall of three walls, 15 tiles wide, across the way.
    for x in [20, 25, 30] {
        put_up(&mut w, 1, WALL, x, 30);
    }
    let r = spawn(&mut w, 0, SPEARS, 32, 50, NORTH);
    move_to(&mut w, 0, r, 32, 12);
    let mut arrived = false;
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        let p = w.regiments.position(r).unwrap();
        assert!(!inside(p, (20, 30), 15, 5), "walked into the wall at {p:?}");
        if (p - FxVec2::from_ints(32, 12)).length() < Fx::ONE {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "never got round the wall");
}

#[test]
fn a_regiment_knocks_a_farmstead_down() {
    let mut w = conquest(64, &[]);
    let farm = put_up(&mut w, 1, FARMSTEAD, 30, 20);
    let r = spawn(&mut w, 0, SPEARS, 31, 40, NORTH);
    assert!(order(
        &mut w,
        0,
        Command::AttackStructure {
            regiment: r,
            structure: farm
        }
    ));
    assert_eq!(w.siege_target(r), Some(farm));
    // 8 files of 3-damage blows: 24 a second, so 600 hit points last 25 s
    // once it is there.
    let mut fell = None;
    let mut first_blow = None;
    for t in 0..90 * TICKS_PER_SECOND {
        w.tick(&[]);
        let hp = w.structures().unwrap().get(farm).map(|v| v.hp);
        if first_blow.is_none() && hp.is_some_and(|hp| hp < 600) {
            first_blow = Some(t);
        }
        if w.events().contains(&Event::Fallen { structure: farm }) {
            fell = Some(t);
            break;
        }
    }
    let (first, fell) = (first_blow.expect("never struck"), fell.expect("never fell"));
    let seconds = (fell - first) / TICKS_PER_SECOND;
    assert!((23..=26).contains(&seconds), "took {seconds} s of blows");
    assert!(w.structures().unwrap().get(farm).is_none());
    assert_eq!(w.siege_target(r), None);
    // Gone from the field: its slot and its ground are free again.
    assert!(w.structures().unwrap().is_empty());
    assert!(w.nav().walkable(FxVec2::from_ints(31, 21)));
    put_up(&mut w, 1, FARMSTEAD, 30, 20);
}

#[test]
fn a_regiment_that_cannot_get_there_gives_up() {
    let mut w = conquest(64, &[]);
    // A Farmstead in a ring of walls with no way in.
    for (x, y) in [
        (25, 25),
        (30, 25),
        (35, 25),
        (25, 30),
        (35, 30),
        (25, 35),
        (30, 35),
        (35, 35),
    ] {
        put_up(&mut w, 1, WALL, x, y);
    }
    let farm = put_up(&mut w, 1, FARMSTEAD, 31, 31);
    let r = spawn(&mut w, 0, SPEARS, 32, 55, NORTH);
    assert!(order(
        &mut w,
        0,
        Command::AttackStructure {
            regiment: r,
            structure: farm
        }
    ));
    let mut gave_up = None;
    for t in 0..90 * TICKS_PER_SECOND {
        w.tick(&[]);
        if w.siege_target(r).is_none() {
            gave_up = Some(t);
            break;
        }
    }
    let gave_up = gave_up.expect("kept trying");
    // After walking up to the ring and trying again.
    assert!(gave_up > 5 * TICKS_PER_SECOND, "gave up at tick {gave_up}");
    assert_eq!(w.structures().unwrap().get(farm).unwrap().hp, 600);
}

#[test]
fn fortified_halls_take_half_and_rams_triple() {
    // Spears are plain regiments here; type 3 is a siege engine.
    let breakers = [
        Breaker::Regiment,
        Breaker::Regiment,
        Breaker::Regiment,
        Breaker::Siege,
    ];
    let hp_after = |unit: u16| {
        let mut w = conquest(64, &breakers);
        let hall = put_up(&mut w, 1, HALL, 30, 20);
        let r = spawn(&mut w, 0, unit, 32, 31, NORTH);
        assert!(order(
            &mut w,
            0,
            Command::AttackStructure {
                regiment: r,
                structure: hall
            }
        ));
        for _ in 0..30 * TICKS_PER_SECOND {
            w.tick(&[]);
        }
        3000 - w.structures().unwrap().get(hall).unwrap().hp
    };
    // Unit 0 (8 files, 3 a blow) against unit 3 (10 files, 3 a blow, siege).
    let spears = hp_after(SPEARS);
    let ram = hp_after(3);
    assert!(spears > 0);
    // Per second: 24 × 0.5 = 12 against 30 × 3 = 90.
    let ratio = ram * 10 / spears;
    assert!((70..=80).contains(&ratio), "ram/spears ×10 = {ratio}");
}

#[test]
fn archers_shoot_from_range() {
    let mut w = conquest(64, &[]);
    let farm = put_up(&mut w, 1, FARMSTEAD, 30, 10);
    let r = spawn(&mut w, 0, ARCHERS, 31, 50, NORTH);
    assert!(order(
        &mut w,
        0,
        Command::AttackStructure {
            regiment: r,
            structure: farm
        }
    ));
    for _ in 0..40 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let hp = w.structures().unwrap().get(farm).unwrap().hp;
    assert!(hp < 600, "never hit");
    // They stood off at their range (25 tiles), not at the wall.
    let p = w.regiments.position(r).unwrap();
    let gap = p.y - Fx::from_int(13);
    assert!(gap > Fx::from_int(15), "walked up to it: {gap:?} away");
}

#[test]
fn a_fallen_building_frees_its_ground() {
    let mut w = conquest(64, &[]);
    let hall = put_up(&mut w, 1, HALL, 30, 30);
    assert!(!w.nav().walkable(FxVec2::from_ints(32, 32)));
    assert!(order(
        &mut w,
        1,
        Command::RemoveStructure { structure: hall }
    ));
    assert!(w.nav().walkable(FxVec2::from_ints(32, 32)));
    // And another fits there now.
    put_up(&mut w, 0, HALL, 30, 30);
}

#[test]
fn buildings_go_up_only_where_they_fit() {
    let mut w = conquest(64, &[]);
    put_up(&mut w, 0, HALL, 10, 10);
    let refused =
        |w: &mut World, building, tile| !order(w, 0, Command::SpawnStructure { building, tile });
    // On another, off the map, half off it, on a regiment.
    assert!(refused(&mut w, FARMSTEAD, (12, 12)));
    assert!(refused(&mut w, FARMSTEAD, (-1, 5)));
    assert!(refused(&mut w, HALL, (61, 5)));
    assert!(refused(&mut w, 9, (40, 40)));
    spawn(&mut w, 1, SPEARS, 41, 41, NORTH);
    assert!(refused(&mut w, FARMSTEAD, (40, 40)));
    // Touching is fine.
    put_up(&mut w, 0, FARMSTEAD, 15, 10);
}

#[test]
fn own_and_allied_buildings_are_not_attacked() {
    let mut w = conquest(64, &[]);
    let farm = put_up(&mut w, 0, FARMSTEAD, 30, 20);
    let r = spawn(&mut w, 0, SPEARS, 31, 40, NORTH);
    assert!(!order(
        &mut w,
        0,
        Command::AttackStructure {
            regiment: r,
            structure: farm
        }
    ));
    // Nor someone else's regiment, ordered by another player.
    let theirs = put_up(&mut w, 2, FARMSTEAD, 40, 20);
    assert!(!order(
        &mut w,
        1,
        Command::AttackStructure {
            regiment: r,
            structure: theirs
        }
    ));
}

#[test]
fn a_new_order_calls_off_the_attack() {
    let mut w = conquest(64, &[]);
    let farm = put_up(&mut w, 1, FARMSTEAD, 30, 20);
    let r = spawn(&mut w, 0, SPEARS, 31, 40, NORTH);
    assert!(order(
        &mut w,
        0,
        Command::AttackStructure {
            regiment: r,
            structure: farm
        }
    ));
    move_to(&mut w, 0, r, 10, 50);
    assert_eq!(w.siege_target(r), None);
    for _ in 0..40 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert_eq!(w.structures().unwrap().get(farm).unwrap().hp, 600);
}

#[test]
fn only_conquest_has_buildings() {
    let mut w = World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 8 },
        units: &DRILL_UNITS,
        map_tiles: 64,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    });
    assert!(!order(
        &mut w,
        0,
        Command::SpawnStructure {
            building: 0,
            tile: (5, 5)
        }
    ));
    assert!(w.structures().is_none());
    // Too late to make it a Conquest match.
    assert!(!w.conquest(&ConquestSetup {
        buildings: &buildings(),
        ..ConquestSetup::default()
    }));
}
