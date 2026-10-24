//! The Field Battle deployment phase (docs/06-game-modes.md): zones, placing,
//! time standing still, hidden enemies, and Ready.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::deploy::DEPLOY_SECONDS;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, World,
};

const SOUTH: Angle = Angle(0x4000);

fn world() -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[0, 1],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    })
}

fn pc(player: u8, command: Command) -> PlayerCommand {
    PlayerCommand { player, command }
}

fn zone(x: i32, y: i32) -> Command {
    Command::DeployZone {
        center: FxVec2::from_ints(x, y),
        radius: Fx::from_int(20),
    }
}

fn spawn(x: i32, y: i32, facing: Angle) -> Command {
    Command::SpawnRegiment {
        unit: 0,
        position: FxVec2::from_ints(x, y),
        facing,
    }
}

/// A move order; while deploying it places the regiment.
fn place(regiment: RegimentHandle, x: i32, y: i32) -> Command {
    Command::MoveTo {
        regiment,
        target: FxVec2::from_ints(x, y),
        facing: Some(NORTH),
        files: 6,
        pace: Fx::ZERO,
    }
}

/// Two armies facing each other 40 tiles apart, each in its own zone, the
/// way a lobby battle starts. Returns the world and each side's regiment.
fn deployed() -> (World, RegimentHandle, RegimentHandle) {
    let mut w = world();
    let stats = w.tick(&[
        pc(0, zone(100, 140)),
        pc(0, spawn(100, 140, NORTH)),
        pc(1, zone(100, 100)),
        pc(1, spawn(100, 100, SOUTH)),
    ]);
    assert_eq!(stats.commands_rejected, 0);
    let mine = w.regiments.iter().find(|r| r.owner == 0).unwrap().handle;
    let theirs = w.regiments.iter().find(|r| r.owner == 1).unwrap().handle;
    (w, mine, theirs)
}

#[test]
fn time_stands_still_and_orders_place_regiments() {
    let (mut w, mine, theirs) = deployed();
    assert!(w.deploying());
    let stats = w.tick(&[
        pc(
            0,
            Command::Attack {
                regiment: mine,
                target: theirs,
            },
        ),
        pc(0, Command::RemoveRegiment { regiment: theirs }),
        // One zone each.
        pc(0, zone(10, 10)),
        // Your own army goes inside your zone.
        pc(0, spawn(100, 100, NORTH)),
    ]);
    assert_eq!(stats.commands_rejected, 4);
    // A move order puts the regiment straight down inside your own zone;
    // outside it, off the map, or someone else's regiment, it is refused.
    let stats = w.tick(&[
        pc(0, place(mine, 110, 150)),
        pc(0, place(mine, 100, 100)),
        pc(1, place(mine, 100, 100)),
    ]);
    assert_eq!(stats.commands_applied, 1);
    let r = w.regiments.get(mine).unwrap();
    assert_eq!(r.position, FxVec2::from_ints(110, 150));
    assert_eq!((r.facing, r.files), (NORTH, 6));
    for _ in 0..100 {
        w.tick(&[]);
    }
    assert_eq!(
        w.regiments.get(mine).unwrap().position,
        FxVec2::from_ints(110, 150)
    );
}

#[test]
fn an_ally_handed_the_army_places_it_in_its_owners_zone() {
    let mut w = World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[0, 1, 0],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    });
    let stats = w.tick(&[
        pc(0, zone(100, 140)),
        pc(2, zone(160, 140)),
        pc(2, spawn(160, 140, NORTH)),
        pc(2, Command::HandOver { to: 0 }),
    ]);
    assert_eq!(stats.commands_rejected, 0);
    let theirs = w.regiments.iter().find(|r| r.owner == 2).unwrap().handle;
    // Player 0 now places it, inside player 2's zone and not its own;
    // player 2 no longer can.
    let stats = w.tick(&[
        pc(0, place(theirs, 100, 140)),
        pc(2, place(theirs, 165, 140)),
        pc(0, place(theirs, 155, 140)),
    ]);
    assert_eq!(stats.commands_applied, 1);
    assert_eq!(
        w.regiments.get(theirs).unwrap().position,
        FxVec2::from_ints(155, 140)
    );
}

#[test]
fn a_setup_spread_over_several_turns_still_deploys() {
    let mut w = world();
    // The host's setup arrives a few turns in, the second side's zone a
    // turn after the first, and the computer's Ready after both.
    w.tick(&[]);
    w.tick(&[pc(0, zone(100, 140)), pc(0, spawn(100, 140, NORTH))]);
    w.tick(&[pc(1, zone(100, 100)), pc(1, spawn(100, 100, SOUTH))]);
    w.tick(&[pc(1, Command::Ready)]);
    assert!(w.deploying() && w.deploy_zone(1).is_some());
    assert_eq!(w.deploy_ticks_left(), DEPLOY_SECONDS * TICKS_PER_SECOND - 3);
    w.tick(&[pc(0, Command::Ready)]);
    assert!(!w.deploying());
    // No new zones once the battle is on.
    assert_eq!(w.tick(&[pc(2, zone(50, 50))]).commands_rejected, 1);
}

#[test]
fn enemies_are_hidden_until_the_battle() {
    let (mut w, mine, theirs) = deployed();
    assert!(w.sees(0, mine) && !w.sees(0, theirs) && !w.sees(1, mine));
    w.tick(&[pc(0, Command::Ready), pc(1, Command::Ready)]);
    assert!(!w.deploying());
    assert!(w.sees(0, theirs) && w.sees(1, mine));
}

#[test]
fn the_battle_starts_when_everyone_is_ready() {
    let (mut w, mine, _) = deployed();
    let stats = w.tick(&[pc(0, Command::Ready), pc(0, Command::Ready)]);
    assert_eq!((stats.commands_applied, stats.commands_rejected), (1, 1));
    assert!(w.deploying() && w.deploy_ready(0) && !w.deploy_ready(1));
    // Still placing after you are ready.
    assert_eq!(w.tick(&[pc(0, place(mine, 100, 130))]).commands_applied, 1);
    w.tick(&[pc(1, Command::Ready)]);
    assert!(!w.deploying());
    // Now orders march.
    let at = w.regiments.get(mine).unwrap().position;
    w.tick(&[pc(
        0,
        Command::MoveTo {
            regiment: mine,
            target: FxVec2::from_ints(100, 110),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    )]);
    for _ in 0..20 {
        w.tick(&[]);
    }
    assert!(w.regiments.get(mine).unwrap().position.y < at.y);
    assert_eq!(w.tick(&[pc(0, Command::Ready)]).commands_rejected, 1);
}

#[test]
fn the_battle_starts_when_time_runs_out() {
    let (mut w, _, _) = deployed();
    let limit = DEPLOY_SECONDS * TICKS_PER_SECOND;
    assert_eq!(w.deploy_ticks_left(), limit - 1);
    while w.tick_count() < limit {
        assert!(w.deploying());
        w.tick(&[]);
    }
    assert!(!w.deploying());
}

#[test]
fn deploying_is_deterministic_and_leaves_other_worlds_alone() {
    let run = || {
        let (mut w, mine, _) = deployed();
        w.tick(&[pc(0, place(mine, 95, 145)), pc(0, Command::Ready)]);
        w.tick(&[pc(1, Command::Ready)]);
        for _ in 0..200 {
            w.tick(&[]);
        }
        w.state_hash()
    };
    assert_eq!(run(), run());
    // A world with no deployment hashes as it did before deployment existed:
    // the same as one where the zone was refused.
    let plain = || {
        let mut w = world();
        w.tick(&[pc(0, spawn(100, 140, NORTH))]);
        w.state_hash()
    };
    let refused = || {
        let mut w = world();
        w.tick(&[pc(9, zone(100, 140)), pc(0, spawn(100, 140, NORTH))]);
        w.state_hash()
    };
    assert_eq!(plain(), refused());
}
