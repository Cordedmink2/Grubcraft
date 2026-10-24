//! Morale: wavering, routing, rallying and shattering.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, MoraleState, PlayerCommand, RegimentHandle, UnitStats, World,
};

const SOUTH: Angle = Angle(0x4000);
/// Drill unit 0: 24 spearmen, morale 60.
const SPEARS: u16 = 0;
/// Drill unit 1: 24 skitterlings, fragile, morale 45.
const SKITTERS: u16 = 1;

fn world_with(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 23,
        capacities: Capacities::default(),
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        // This rule on its own: generals have tests of their own.
        generals: false,
    })
}

fn world() -> World {
    world_with(&DRILL_UNITS)
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn state(w: &World, h: RegimentHandle) -> Option<MoraleState> {
    w.regiments.get(h).map(|r| r.morale)
}

/// Tick until `h` is in `want` (or gone); returns the ticks it took.
fn until(w: &mut World, h: RegimentHandle, want: MoraleState, max_ticks: u32) -> Option<u32> {
    for t in 0..max_ticks {
        match state(w, h) {
            Some(s) if s == want => return Some(t),
            None => return None,
            _ => {}
        }
        w.tick(&[]);
    }
    None
}

fn order(w: &mut World, player: u8, regiment: RegimentHandle, x: i32, y: i32) -> bool {
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::MoveTo {
            regiment,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    stats.commands_applied == 1
}

/// Spearmen (player 0) fight skitterlings (player 1) face to face; the
/// skitterlings break first. Returns (spears, skitterlings).
fn losing_fight(w: &mut World) -> (RegimentHandle, RegimentHandle) {
    let spears = spawn(w, 0, SPEARS, 100, 100, NORTH);
    let skitters = spawn(w, 1, SKITTERS, 100, 97, SOUTH);
    (spears, skitters)
}

#[test]
fn losing_regiments_rout_before_they_are_destroyed() {
    let mut w = world();
    let (_, skitters) = losing_fight(&mut w);
    assert_eq!(state(&w, skitters), Some(MoraleState::Steady));
    let broke = until(&mut w, skitters, MoraleState::Routing, 20 * 120);
    assert!(broke.is_some(), "the skitterlings never broke");
    let r = w.regiments.get(skitters).unwrap();
    assert!(r.models > 0 && !r.engaged, "{r:?}");
    assert!(r.models < 24);
}

#[test]
fn shaken_regiments_waver_first() {
    let mut w = world();
    let (_, skitters) = losing_fight(&mut w);
    assert!(until(&mut w, skitters, MoraleState::Wavering, 20 * 120).is_some());
}

#[test]
fn routing_regiments_ignore_orders_and_run() {
    let mut w = world();
    let (spears, skitters) = losing_fight(&mut w);
    until(&mut w, skitters, MoraleState::Routing, 20 * 120).unwrap();
    assert!(!order(&mut w, 1, skitters, 100, 100));
    // Keep the spearmen from chasing, then watch the skitterlings run.
    let start = w.regiments.get(skitters).unwrap().position;
    assert!(order(&mut w, 0, spears, 100, 120));
    for _ in 0..20 * 3 {
        w.tick(&[]);
    }
    let now = w.regiments.get(skitters).unwrap().position;
    assert!(
        (now - FxVec2::from_ints(100, 120)).length()
            > (start - FxVec2::from_ints(100, 120)).length(),
        "ran from {start:?} to {now:?}"
    );
}

#[test]
fn routers_rally_once_the_enemy_is_gone() {
    let mut w = world();
    let (spears, skitters) = losing_fight(&mut w);
    until(&mut w, skitters, MoraleState::Routing, 20 * 120).unwrap();
    // Send the spearmen far away.
    assert!(order(&mut w, 0, spears, 200, 200));
    let rallied = until(&mut w, skitters, MoraleState::Wavering, 20 * 60);
    assert!(rallied.is_some(), "{:?}", w.regiments.get(skitters));
    // They take orders again, and steady themselves out of the fight.
    assert!(order(&mut w, 1, skitters, 60, 60));
    assert!(until(&mut w, skitters, MoraleState::Steady, 20 * 30).is_some());
}

#[test]
fn routing_twice_shatters_a_regiment() {
    // Tougher skitterlings, so the second fight lasts long enough to break
    // them again rather than wipe them out.
    let mut units = DRILL_UNITS;
    units[SKITTERS as usize].hp_per_model = 24;
    let mut w = world_with(&units);
    let (spears, skitters) = losing_fight(&mut w);
    until(&mut w, skitters, MoraleState::Routing, 20 * 120).unwrap();
    assert!(order(&mut w, 0, spears, 200, 200));
    until(&mut w, skitters, MoraleState::Wavering, 20 * 60).unwrap();
    // Straight back into the fight while still shaken.
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Attack {
            regiment: spears,
            target: skitters,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    assert!(until(&mut w, skitters, MoraleState::Shattered, 20 * 90).is_some());
    assert!(!order(&mut w, 1, skitters, 60, 60));
    // No rally from shattered, even with the enemy gone.
    assert!(order(&mut w, 0, spears, 20, 20) || w.regiments.get(spears).is_none());
    for _ in 0..20 * 30 {
        w.tick(&[]);
    }
    if let Some(r) = w.regiments.get(skitters) {
        assert_eq!(r.morale, MoraleState::Shattered);
    }
}

#[test]
fn rear_attacks_break_a_regiment_faster() {
    let ticks_to_rout = |from_behind: bool| {
        let mut w = world();
        // Skitterlings face north; spearmen hit them from the front (north)
        // or from behind (south).
        let skitters = spawn(&mut w, 1, SKITTERS, 100, 100, NORTH);
        if from_behind {
            spawn(&mut w, 0, SPEARS, 100, 103, NORTH);
        } else {
            spawn(&mut w, 0, SPEARS, 100, 97, SOUTH);
        }
        until(&mut w, skitters, MoraleState::Routing, 20 * 200).expect("never broke")
    };
    let front = ticks_to_rout(false);
    let rear = ticks_to_rout(true);
    assert!(rear < front, "rear {rear} ticks, front {front} ticks");
}

#[test]
fn a_rout_shakes_friends_nearby() {
    let mut w = world();
    let (_, skitters) = losing_fight(&mut w);
    // A fresh friend of the skitterlings, watching from 10 tiles away.
    let friend = spawn(&mut w, 1, SKITTERS, 110, 90, SOUTH);
    let fresh = w.regiments.get(friend).unwrap().morale_points;
    until(&mut w, skitters, MoraleState::Routing, 20 * 120).unwrap();
    let after = w.regiments.get(friend).unwrap().morale_points;
    assert!(after < fresh, "friend morale {fresh} -> {after}");
}

#[test]
fn fearless_regiments_never_rout() {
    let mut units = DRILL_UNITS;
    units[SKITTERS as usize].morale = 0;
    let mut w = world_with(&units);
    let (_, skitters) = losing_fight(&mut w);
    for _ in 0..20 * 400 {
        match w.regiments.get(skitters) {
            Some(r) => assert!(!r.morale.broken()),
            None => return,
        }
        w.tick(&[]);
    }
    panic!("the fight never ended");
}

#[test]
fn routers_stay_on_the_map() {
    let mut w = world();
    // Skitterlings with their backs to the map's west edge.
    let skitters = spawn(&mut w, 1, SKITTERS, 6, 100, Angle(0));
    spawn(&mut w, 0, SPEARS, 9, 100, Angle(0x8000));
    until(&mut w, skitters, MoraleState::Routing, 20 * 120).unwrap();
    for _ in 0..20 * 20 {
        w.tick(&[]);
        if let Some(r) = w.regiments.get(skitters) {
            assert!(r.position.x >= Fx::ZERO, "{:?}", r.position);
        }
    }
}
