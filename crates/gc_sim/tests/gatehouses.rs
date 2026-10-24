//! Gatehouses (docs/26-fortifications.md, stage 4c): a gate upgrades to
//! its faction's gatehouse, whose top is a walkway joined to the raised
//! wall beside it, which foot and archers man like the wall.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, GATE, LINE, Refusal, Role, WallTwist};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Cover, Ground, Growth,
    MatchSetup, PlayerCommand, RegimentHandle, RegimentView, StructureHandle, TICKS_PER_SECOND,
    World,
};

const FOOT: u16 = 0;
const HALL: u16 = 1;
const STONE: u16 = 2;
const GATE_KIND: u16 = 3;
const GATEHOUSE: u16 = 4;
const MAP: u32 = 64;
/// The wall's column: [`STONE`] tiles from y = 10 to 39, then a gate from
/// y = 40 to 42.
const WALL_X: i32 = 20;

/// Open ground; each side has a hall, and side 0 has a stone wall down
/// x = [`WALL_X`] from y = 10 to 39 with a gate (a gatehouse if
/// `gatehouse`) at its south end, and a lone gate at (40, 20).
fn world(gatehouse: bool) -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let n = MAP as usize;
    let cover = vec![Cover::Open as u8; n * n];
    let growth = vec![Growth::Open as u8; n * n];
    let mut w = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 16 },
        units: &[DRILL_UNITS[0]],
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
        generals: false,
    });
    let palisade = BuildingStats {
        role: Role::Wall,
        tier: 1,
        upgrades_from: None,
        cost_grub: 10,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 250,
        armour: Armour::Fortified,
        width: 1,
        depth: 1,
        supply: 0,
        flags: LINE,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
    };
    let gate = BuildingStats {
        cost_grub: 60,
        build_ticks: 100,
        hp: 1000,
        depth: 3,
        flags: GATE,
        ..palisade
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (2000, 500);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            palisade,
            BuildingStats {
                role: Role::Main,
                width: 3,
                depth: 3,
                flags: 0,
                ..palisade
            },
            BuildingStats {
                upgrades_from: Some(0),
                hp: 625,
                ..palisade
            },
            gate,
            BuildingStats {
                upgrades_from: Some(GATE_KIND),
                cost_grub: 50,
                hp: 1500,
                ..gate
            },
        ],
        units: &[ConquestUnit {
            mans_walls: true,
            ..ConquestUnit::default()
        }],
        structures: 1024,
        start,
        ..ConquestSetup::default()
    }));
    for (player, tile) in [(0, (2, 2)), (1, (58, 58))] {
        spawn_structure(&mut w, player, HALL, tile);
    }
    for y in 10..40 {
        spawn_structure(&mut w, 0, STONE, (WALL_X as i16, y));
    }
    let kind = if gatehouse { GATEHOUSE } else { GATE_KIND };
    spawn_structure(&mut w, 0, kind, (WALL_X as i16, 40));
    spawn_structure(&mut w, 0, kind, (40, 20));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn_structure(w: &mut World, player: u8, building: u16, tile: (i16, i16)) {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile,
            turn: 0,
        }
    ));
}

fn spawn(w: &mut World, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        0,
        Command::SpawnRegiment {
            unit: FOOT,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn tile(w: &World, at: (i32, i32)) -> StructureHandle {
    let s = w.structures().unwrap();
    s.iter().find(|v| v.tile == at).unwrap().handle
}

fn view(w: &World, h: RegimentHandle) -> RegimentView {
    w.regiments.iter().find(|r| r.handle == h).unwrap()
}

fn man(w: &mut World, r: RegimentHandle, at: (i32, i32)) -> bool {
    let structure = tile(w, at);
    order(
        w,
        0,
        Command::ManWall {
            regiment: r,
            structure,
        },
    )
}

/// Tick until `r` is on the wall, failing after a minute.
fn climb(w: &mut World, r: RegimentHandle) {
    for _ in 0..60 * TICKS_PER_SECOND {
        if view(w, r).on_wall {
            return;
        }
        w.tick(&[]);
    }
    panic!("it never got onto the wall");
}

#[test]
fn a_gate_upgrades_to_its_gatehouse_and_only_that_is_manned() {
    let mut w = world(false);
    let gate = tile(&w, (40, 20));
    let foot = spawn(&mut w, 34, 21);
    assert_eq!(w.man_check(foot, gate), Err(Refusal::NoWalkway));
    assert_eq!(w.can_upgrade(0, gate), Ok(GATEHOUSE));
    assert!(order(&mut w, 0, Command::Upgrade { structure: gate }));
    assert_eq!(w.can_upgrade(0, gate), Err(Refusal::Busy));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let s = w.structures().unwrap();
    let v = s.get(gate).unwrap();
    // Grown in place, a gate still, keeping the hit points it gained.
    assert_eq!(v.building, GATEHOUSE);
    assert!(v.gate.is_some());
    assert_eq!((v.hp, v.max_hp), (1500, 1500));
    assert_eq!(w.man_check(foot, gate), Ok(()));
    // The top level has nothing above it.
    assert_eq!(w.can_upgrade(0, gate), Err(Refusal::Unknown));
}

#[test]
fn a_lone_gatehouse_holds_a_rank_along_its_top_facing_out() {
    let mut w = world(true);
    let foot = spawn(&mut w, 34, 21);
    assert!(man(&mut w, foot, (40, 20)));
    climb(&mut w, foot);
    assert_eq!(w.wall_tiles(foot), &[tile(&w, (40, 20))]);
    let v = view(&w, foot);
    // Three tiles of room: three soldiers a rank, on its middle.
    assert_eq!(v.frontage, 3);
    let middle = FxVec2::new(Fx::from_ratio(81, 2), Fx::from_ratio(43, 2));
    assert!(
        (v.position - middle).length() < Fx::from_ratio(1, 4),
        "{v:?}"
    );
    // Square to it, away from where it was sent from: east.
    assert_eq!(v.facing, Angle::of_vec(FxVec2::from_ints(1, 0)));
}

#[test]
fn a_gatehouse_joins_the_walkway_of_the_wall_beside_it() {
    let mut w = world(true);
    let house = tile(&w, (WALL_X, 40));
    // Sent to the wall by the gate, it takes the gatehouse too.
    let a = spawn(&mut w, 14, 38);
    assert!(man(&mut w, a, (WALL_X, 38)));
    climb(&mut w, a);
    let held = w.wall_tiles(a).to_vec();
    assert!(held.contains(&house), "{held:?}");
    // Twenty-four soldiers: the gatehouse's three and twenty-one tiles.
    assert_eq!(held.len(), 22);
    assert_eq!(view(&w, a).frontage, 24);
    // When the gatehouse falls, down they come.
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: house }
    ));
    let v = view(&w, a);
    assert!(!v.on_wall);
    assert!(v.disorder > 0);
}

#[test]
fn a_gate_the_wall_was_raised_beside_is_not_part_of_its_walkway() {
    let mut w = world(false);
    let gate = tile(&w, (WALL_X, 40));
    let a = spawn(&mut w, 14, 38);
    assert!(man(&mut w, a, (WALL_X, 38)));
    climb(&mut w, a);
    let held = w.wall_tiles(a).to_vec();
    assert!(!held.contains(&gate));
    assert_eq!(held.len(), 24);
}

#[test]
fn those_up_on_a_gatehouse_dont_hold_its_door_open() {
    let mut w = world(true);
    let house = tile(&w, (40, 20));
    let foot = spawn(&mut w, 34, 21);
    assert!(man(&mut w, foot, (40, 20)));
    climb(&mut w, foot);
    for _ in 0..5 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let v = w.structures().unwrap().get(house).unwrap();
    assert!(!v.open, "shut to enemies under the men on top");
}

#[test]
fn when_a_gatehouse_falls_its_men_come_down_in_its_middle() {
    let mut w = world(true);
    let house = tile(&w, (40, 20));
    let foot = spawn(&mut w, 34, 21);
    assert!(man(&mut w, foot, (40, 20)));
    climb(&mut w, foot);
    let models = view(&w, foot).models;
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: house }
    ));
    let v = view(&w, foot);
    assert!(!v.on_wall);
    assert_eq!(v.models, models, "hurt, not killed");
    let middle = FxVec2::new(Fx::from_ratio(81, 2), Fx::from_ratio(43, 2));
    assert!((v.position - middle).length() < Fx::from_int(2), "{v:?}");
}
