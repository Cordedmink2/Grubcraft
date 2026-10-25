//! Hidden traps (docs/26-fortifications.md, "Field works"): only their
//! owner sees them, and the first enemy regiment onto one springs it.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::structures::{Armour, Refusal, Role, Trap, WallTwist};
use gc_sim::traps::{HANDS_TICKS, MINE_TICKS, NEST_TICKS};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup, PlayerCommand,
    RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const SWORDS: u16 = 0;
const SKITTERS: u16 = 1;
const HALL: u16 = 0;
const PIT: u16 = 1;
const HANDS: u16 = 2;
const MINE: u16 = 3;
const NEST: u16 = 4;
const MAP: u32 = 64;
/// Where the trap is laid.
const AT: (i16, i16) = (30, 30);

fn world() -> World {
    let skitters = UnitStats {
        models: 16,
        files: 8,
        ..DRILL_UNITS[0]
    };
    let side = (MAP as i32 * gc_sim::nav::CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        map_tiles: MAP,
        capacities: Capacities { regiments: 16 },
        units: &[DRILL_UNITS[0], skitters],
        seed: 5,
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
        trap: Trap::None,
    };
    let trap = |trap| BuildingStats {
        role: Role::Trap,
        cost_grub: 20,
        build_ticks: 80,
        hp: 60,
        armour: Armour::Structure,
        width: 1,
        depth: 1,
        trap,
        ..hall
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (2000, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            hall,
            trap(Trap::Pit),
            trap(Trap::Hands),
            trap(Trap::Mine),
            trap(Trap::Nest(SKITTERS)),
        ],
        units: &[ConquestUnit::default(), ConquestUnit::default()],
        structures: 64,
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

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: Angle(0x4000),
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn lay(w: &mut World, building: u16) {
    assert!(order(
        w,
        0,
        Command::SpawnStructure {
            building,
            tile: AT,
            turn: 0,
        }
    ));
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

fn traps(w: &World) -> usize {
    (w.structures().unwrap().iter())
        .filter(|v| v.building != HALL)
        .count()
}

fn position(w: &World, r: RegimentHandle) -> FxVec2 {
    w.regiments.iter().find(|v| v.handle == r).unwrap().position
}

/// Walk regiment `r` of `player` across the trap, north to south, and
/// tick until it is sprung (or give up).
fn cross(w: &mut World, player: u8, r: RegimentHandle) -> Option<Event> {
    move_to(w, player, r, i32::from(AT.0), i32::from(AT.1) + 12);
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
        if let Some(e) = w.events().iter().find(|e| matches!(e, Event::Sprung(_))) {
            return Some(*e);
        }
    }
    None
}

#[test]
fn only_its_owner_sees_a_trap_and_its_own_side_walks_over_it() {
    let mut w = world();
    lay(&mut w, PIT);
    let v = (w.structures().unwrap().iter())
        .find(|v| v.building == PIT)
        .unwrap();
    assert!(v.seen_by(0) && !v.seen_by(1), "hidden from the enemy");
    let own = spawn(&mut w, 0, SWORDS, i32::from(AT.0), i32::from(AT.1) - 8);
    assert!(
        cross(&mut w, 0, own).is_none(),
        "its own side walks over it"
    );
    assert_eq!(traps(&w), 1);
    // Nothing is built over it.
    assert_eq!(
        w.can_lay(0, HALL, (i32::from(AT.0) - 1, i32::from(AT.1) - 1), 0),
        Err(Refusal::Built)
    );
}

#[test]
fn a_spike_pit_hurts_the_first_enemy_onto_it_and_is_gone() {
    let mut w = world();
    lay(&mut w, PIT);
    let foe = spawn(&mut w, 1, SWORDS, i32::from(AT.0), i32::from(AT.1) - 8);
    let hp = |w: &World| -> u32 {
        (w.regiments.model_hp(foe).unwrap().iter())
            .map(|&h| u32::from(h))
            .sum()
    };
    let before = hp(&w);
    let Some(Event::Sprung(s)) = cross(&mut w, 1, foe) else {
        panic!("not sprung");
    };
    assert_eq!((s.trap, s.on, s.owner), (Trap::Pit, foe, 0));
    assert!(hp(&w) < before, "hurt");
    let r = w.regiments.iter().find(|v| v.handle == foe).unwrap();
    assert!(r.disorder > 0, "in disorder");
    assert_eq!(traps(&w), 0, "gone");
}

#[test]
fn grave_hands_hold_a_regiment_and_a_shard_mine_stuns_one() {
    for (trap, ticks, halts) in [(HANDS, HANDS_TICKS, false), (MINE, MINE_TICKS, true)] {
        let mut w = world();
        lay(&mut w, trap);
        let foe = spawn(&mut w, 1, SWORDS, i32::from(AT.0), i32::from(AT.1) - 8);
        assert!(cross(&mut w, 1, foe).is_some(), "sprung");
        let held = position(&w, foe);
        for _ in 0..ticks - 2 {
            w.tick(&[]);
        }
        assert_eq!(position(&w, foe), held, "held fast");
        for _ in 0..3 * TICKS_PER_SECOND {
            w.tick(&[]);
        }
        let r = w.regiments.iter().find(|v| v.handle == foe).unwrap();
        // The hands let it go on its way; the mine left it standing.
        assert_eq!(r.position != held, !halts, "trap {trap}");
    }
}

#[test]
fn a_burrow_nest_bursts_out_its_owners_regiment_for_a_while() {
    let mut w = world();
    lay(&mut w, NEST);
    let foe = spawn(&mut w, 1, SWORDS, i32::from(AT.0), i32::from(AT.1) - 8);
    let Some(Event::Sprung(s)) = cross(&mut w, 1, foe) else {
        panic!("not sprung");
    };
    let raised = s.raised.expect("a regiment bursts out");
    let r = w.regiments.iter().find(|v| v.handle == raised).unwrap();
    assert_eq!((r.owner, r.unit), (0, SKITTERS));
    assert!(w.regiments.summoned_left(raised) > NEST_TICKS - 5);
    assert_eq!(traps(&w), 0);
}
