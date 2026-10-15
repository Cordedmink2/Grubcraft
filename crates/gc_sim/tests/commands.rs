//! Command validation: bad orders are rejected, never applied.

use gc_fixed::{Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{Capacities, Command, MatchSetup, PlayerCommand, World};

fn world() -> World {
    World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
    })
}

#[test]
fn spawn_uses_the_unit_type() {
    let mut w = world();
    let stats = w.tick(&[PlayerCommand {
        player: 1,
        command: Command::SpawnRegiment {
            unit: 1,
            position: FxVec2::from_ints(10, 10),
            facing: NORTH,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    let r = w.regiments.iter().next().unwrap();
    assert_eq!((r.owner, r.unit), (1, 1));
}

#[test]
fn unknown_unit_type_is_rejected() {
    let mut w = world();
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: DRILL_UNITS.len() as u16,
            position: FxVec2::from_ints(10, 10),
            facing: NORTH,
        },
    }]);
    assert_eq!(stats.commands_rejected, 1);
    assert!(w.regiments.is_empty());
}

#[test]
fn players_cannot_order_other_players_regiments() {
    let mut w = world();
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(10, 10),
            facing: NORTH,
        },
    }]);
    let regiment = w.regiments.iter().next().unwrap().handle;
    let stats = w.tick(&[PlayerCommand {
        player: 2,
        command: Command::MoveTo {
            regiment,
            target: FxVec2::from_ints(50, 50),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    assert_eq!(stats.commands_rejected, 1);
}
