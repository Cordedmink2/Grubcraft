//! Sandbox commands: removing regiments and tuning unit stats mid-match.

use gc_fixed::{Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World};

fn world() -> World {
    World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 64 },
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    })
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32) -> RegimentHandle {
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        },
    }]);
    w.regiments.iter().last().unwrap().handle
}

fn tune(w: &mut World, stats: UnitStats) -> bool {
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::TuneUnit { unit: 0, stats },
    }])
    .commands_applied
        == 1
}

#[test]
fn removed_regiments_leave_the_field() {
    let mut w = world();
    let a = spawn(&mut w, 0, 50, 50);
    let b = spawn(&mut w, 1, 50, 47);
    w.tick(&[]);
    assert!(w.regiments.get(a).unwrap().engaged);
    // Any player may clear any regiment in the sandbox.
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::RemoveRegiment { regiment: b },
    }]);
    assert_eq!(stats.commands_applied, 1);
    assert!(w.regiments.get(b).is_none());
    w.tick(&[]);
    assert!(!w.regiments.get(a).unwrap().engaged);
    // A stale handle is rejected.
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::RemoveRegiment { regiment: b },
    }]);
    assert_eq!(stats.commands_rejected, 1);
}

#[test]
fn tuned_speed_applies_to_regiments_on_the_field() {
    let mut w = world();
    let r = spawn(&mut w, 0, 50, 50);
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: r,
            target: FxVec2::from_ints(50, 10),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    let walked = |w: &mut World| {
        let y0 = w.regiments.get(r).unwrap().position.y;
        for _ in 0..20 {
            w.tick(&[]);
        }
        y0 - w.regiments.get(r).unwrap().position.y
    };
    let before = walked(&mut w);
    let hash = w.state_hash();
    assert!(tune(
        &mut w,
        UnitStats {
            speed: DRILL_UNITS[0].speed * 2,
            ..DRILL_UNITS[0]
        }
    ));
    assert_ne!(w.state_hash(), hash, "the unit table is part of the state");
    assert_eq!(w.units()[0].speed, DRILL_UNITS[0].speed * 2);
    let after = walked(&mut w);
    assert_eq!(after, before * 2, "{before:?} then {after:?}");
}

#[test]
fn tuning_an_unknown_unit_is_rejected() {
    let mut w = world();
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::TuneUnit {
            unit: 99,
            stats: DRILL_UNITS[0],
        },
    }]);
    assert_eq!(stats.commands_rejected, 1);
}

#[test]
fn tuned_stats_are_kept_in_range() {
    let mut w = world();
    assert!(tune(
        &mut w,
        UnitStats {
            melee_attack: 250,
            models: 9999,
            ..DRILL_UNITS[0]
        }
    ));
    assert_eq!(w.units()[0].melee_attack, 100);
    assert_eq!(w.units()[0].models, gc_sim::formation::MAX_MODELS);
}
