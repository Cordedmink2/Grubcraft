//! Gates: a wall its owner's side walks through and enemies don't, set
//! Auto, Locked or Open (docs/26-fortifications.md, stage 2).

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::gates::{GATE_HOLD, GateMode};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, GATE, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, MatchSetup, PlayerCommand, RegimentHandle,
    StructureHandle, TICKS_PER_SECOND, World,
};

const SPEARS: u16 = 0;
/// A stretch of wall five tiles long, east to west.
const WALL: u16 = 0;
/// A post: a tile of wall.
const POST: u16 = 1;
/// A gate a tile thick and three across, drawn north to south.
const GATE_3: u16 = 2;
/// A plain building, not a gate.
const HUT: u16 = 3;

fn building(width: u8, depth: u8, flags: u16) -> BuildingStats {
    BuildingStats {
        role: Role::Wall,
        tier: 1,
        upgrades_from: None,
        cost_grub: 20,
        cost_marrow: 0,
        build_ticks: 500,
        hp: 1000,
        armour: Armour::Fortified,
        width,
        depth,
        supply: 0,
        flags,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
    }
}

fn buildings() -> [BuildingStats; 4] {
    [
        building(5, 1, 0),
        building(1, 1, 0),
        building(1, 3, GATE),
        BuildingStats {
            role: Role::Housing,
            ..building(2, 2, 0)
        },
    ]
}

const TILES: u32 = 64;

/// An open map in Conquest with `teams`, walled right across along tile
/// row 30, but for party 0's gate, turned to run east to west, on tiles
/// 30 to 32.
fn walled(teams: &[u8]) -> (World, StructureHandle) {
    let side = TILES as usize * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 16 },
        units: &DRILL_UNITS,
        map_tiles: TILES,
        nav: &nav,
        teams,
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    });
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        structures: 32,
        ..ConquestSetup::default()
    }));
    for x in (0..30).step_by(5).chain((33..63).step_by(5)) {
        put_up(&mut w, 0, WALL, x, 30, 0);
    }
    put_up(&mut w, 0, POST, 63, 30, 0);
    let gate = put_up(&mut w, 0, GATE_3, 30, 30, 1);
    (w, gate)
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16, turn: u8) -> StructureHandle {
    assert!(
        order(
            w,
            player,
            Command::SpawnStructure {
                building,
                tile: (x, y),
                turn,
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

fn spawn(w: &mut World, player: u8, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit: SPEARS,
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

fn set_gate(w: &mut World, player: u8, gate: StructureHandle, mode: GateMode) -> bool {
    order(
        w,
        player,
        Command::SetGate {
            structure: gate,
            mode,
        },
    )
}

/// Run for up to `seconds`, until `r` is within a tile of (x, y); whether it
/// got there. It never stands on the wall.
fn reaches(w: &mut World, r: RegimentHandle, (x, y): (i32, i32), seconds: u32) -> bool {
    let goal = FxVec2::from_ints(x, y);
    for _ in 0..seconds * TICKS_PER_SECOND {
        w.tick(&[]);
        let p = w.regiments.position(r).unwrap();
        if (p - goal).length() < Fx::ONE {
            return true;
        }
    }
    false
}

fn open(w: &World, gate: StructureHandle) -> bool {
    w.structures().unwrap().get(gate).unwrap().open
}

#[test]
fn a_side_marches_through_its_own_gate() {
    let (mut w, gate) = walled(&[]);
    assert_eq!(
        w.structures().unwrap().get(gate).unwrap().gate,
        Some(GateMode::Auto)
    );
    let r = spawn(&mut w, 0, 31, 45, NORTH);
    move_to(&mut w, 0, r, 31, 15);
    assert!(
        reaches(&mut w, r, (31, 15), 60),
        "never came through the gate"
    );
}

#[test]
fn an_enemy_finds_a_shut_gate_a_wall() {
    let (mut w, _) = walled(&[]);
    let r = spawn(&mut w, 1, 31, 45, NORTH);
    move_to(&mut w, 1, r, 31, 15);
    assert!(!reaches(&mut w, r, (31, 15), 60), "an enemy came through");
    let p = w.regiments.position(r).unwrap();
    assert!(
        p.y > Fx::from_int(31),
        "it should stand south of the wall, not at {p:?}"
    );
}

#[test]
fn allies_march_through_each_others_gates() {
    let (mut w, _) = walled(&[0, 0]);
    let r = spawn(&mut w, 1, 31, 45, NORTH);
    move_to(&mut w, 1, r, 31, 15);
    assert!(
        reaches(&mut w, r, (31, 15), 60),
        "an ally never came through"
    );
}

#[test]
fn a_locked_gate_stops_its_own_side() {
    let (mut w, gate) = walled(&[]);
    assert!(set_gate(&mut w, 0, gate, GateMode::Locked));
    let r = spawn(&mut w, 0, 31, 45, NORTH);
    move_to(&mut w, 0, r, 31, 15);
    assert!(!reaches(&mut w, r, (31, 15), 45));
    // Unlocked, it goes through.
    assert!(set_gate(&mut w, 0, gate, GateMode::Auto));
    move_to(&mut w, 0, r, 31, 15);
    assert!(reaches(&mut w, r, (31, 15), 60));
}

#[test]
fn an_open_gate_lets_anyone_through() {
    let (mut w, gate) = walled(&[]);
    assert!(set_gate(&mut w, 0, gate, GateMode::Open));
    w.tick(&[]);
    assert!(open(&w, gate));
    let r = spawn(&mut w, 1, 31, 45, NORTH);
    move_to(&mut w, 1, r, 31, 15);
    assert!(
        reaches(&mut w, r, (31, 15), 60),
        "an enemy found an open gate shut"
    );
}

#[test]
fn an_auto_gate_opens_for_its_side_and_shuts_behind_it() {
    let (mut w, gate) = walled(&[]);
    w.tick(&[]);
    assert!(!open(&w, gate));
    let r = spawn(&mut w, 0, 31, 45, NORTH);
    move_to(&mut w, 0, r, 31, 15);
    let mut opened = false;
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        opened |= open(&w, gate);
        let p = w.regiments.position(r).unwrap();
        if (p - FxVec2::from_ints(31, 15)).length() < Fx::ONE {
            break;
        }
    }
    assert!(opened, "it never opened");
    // Standing well clear, it shuts once its hold runs out.
    for _ in 0..u32::from(GATE_HOLD) + 1 {
        w.tick(&[]);
    }
    assert!(!open(&w, gate));
}

#[test]
fn an_enemy_routes_through_an_auto_gate_while_it_stands_open() {
    // Party 1's regiment, well off to the east, is sent north: while the
    // gate is shut its route ends south of the wall; once party 0's column
    // marching in has opened it, its route runs through.
    let (mut w, gate) = walled(&[]);
    let enemy = spawn(&mut w, 1, 56, 50, NORTH);
    let ends_north = |w: &World| {
        w.route(enemy)
            .last()
            .is_some_and(|p| p.y < Fx::from_int(30))
    };
    move_to(&mut w, 1, enemy, 31, 15);
    w.tick(&[]);
    assert!(!ends_north(&w), "a route through a shut gate");
    let r = spawn(&mut w, 0, 31, 45, NORTH);
    move_to(&mut w, 0, r, 31, 10);
    let mut through = false;
    for _ in 0..30 * TICKS_PER_SECOND {
        w.tick(&[]);
        if open(&w, gate) {
            move_to(&mut w, 1, enemy, 31, 15);
            w.tick(&[]);
            through = ends_north(&w);
            break;
        }
    }
    assert!(through, "the enemy found no way through the open gate");
}

#[test]
fn a_gate_pulled_down_leaves_a_gap() {
    let (mut w, gate) = walled(&[]);
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: gate }
    ));
    let r = spawn(&mut w, 1, 31, 45, NORTH);
    move_to(&mut w, 1, r, 31, 15);
    assert!(reaches(&mut w, r, (31, 15), 60));
}

#[test]
fn only_a_gates_owner_sets_it_and_only_a_gate() {
    let (mut w, gate) = walled(&[0, 0]);
    let hut = put_up(&mut w, 0, HUT, 10, 10, 0);
    assert!(!set_gate(&mut w, 0, hut, GateMode::Locked));
    // Not even an ally.
    assert!(!set_gate(&mut w, 1, gate, GateMode::Locked));
    assert!(set_gate(&mut w, 0, gate, GateMode::Locked));
    assert_eq!(
        w.structures().unwrap().get(gate).unwrap().gate,
        Some(GateMode::Locked)
    );
}

#[test]
fn each_side_walks_through_its_own_gate_and_not_the_others() {
    // Party 1 swaps a stretch of party 0's wall for a gate of its own.
    let (mut w, _) = walled(&[0, 1]);
    let wall = w
        .structures()
        .unwrap()
        .iter()
        .find(|v| v.tile == (10, 30))
        .unwrap()
        .handle;
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: wall }
    ));
    put_up(&mut w, 1, GATE_3, 10, 30, 1);
    put_up(&mut w, 0, POST, 13, 30, 0);
    put_up(&mut w, 0, POST, 14, 30, 0);
    // Each in turn heads north from in front of the other's gate: it goes
    // round by its own.
    for (party, from, own) in [(0, 11, 31), (1, 31, 11)] {
        let r = spawn(&mut w, party, from, 45, NORTH);
        move_to(&mut w, party, r, from, 15);
        let mut through = false;
        for _ in 0..120 * TICKS_PER_SECOND {
            w.tick(&[]);
            let p = w.regiments.position(r).unwrap();
            if p.y < Fx::from_int(30) {
                assert!(
                    (p.x - Fx::from_int(own)).abs() < Fx::from_int(6),
                    "party {party} came through the wrong gate, at {p:?}"
                );
                through = true;
                break;
            }
        }
        assert!(through, "party {party} never came through");
        assert!(order(
            &mut w,
            party,
            Command::RemoveRegiment { regiment: r }
        ));
    }
}
