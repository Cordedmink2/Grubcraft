//! Teams: allies don't fight each other and stand together when it comes to
//! morale (docs/18-ai.md, engine hook 1).

use gc_fixed::{Angle, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, MoraleState, PlayerCommand, RegimentHandle, UnitStats, World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const ARCHERS: u16 = 2;

/// Two against two: players 0 and 2 are one team, 1 and 3 the other.
const TWO_V_TWO: [u8; 4] = [0, 1, 0, 1];

fn world(units: &[UnitStats], teams: &[u8]) -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities { regiments: 64 },
        units,
        map_tiles: 256,
        nav: &[],
        teams,
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        // This rule on its own: generals have tests of their own.
        generals: false,
    })
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

#[test]
fn allies_in_contact_do_not_fight() {
    let mut w = world(&DRILL_UNITS, &TWO_V_TWO);
    assert!(!w.hostile(0, 2) && w.hostile(0, 1));
    let a = spawn(&mut w, 0, SPEARS, 50, 50, NORTH);
    let b = spawn(&mut w, 2, SPEARS, 50, 47, SOUTH);
    run(&mut w, 20 * 10);
    for h in [a, b] {
        let r = w.regiments.get(h).unwrap();
        assert!(!r.engaged);
        assert_eq!(r.models, 24);
    }
    // The same blocks on opposing teams fight at once.
    let mut w = world(&DRILL_UNITS, &TWO_V_TWO);
    let a = spawn(&mut w, 0, SPEARS, 50, 50, NORTH);
    spawn(&mut w, 3, SPEARS, 50, 47, SOUTH);
    w.tick(&[]);
    assert!(w.regiments.get(a).unwrap().engaged);
}

#[test]
fn an_ally_cannot_be_attacked_or_shot() {
    let mut w = world(&DRILL_UNITS, &TWO_V_TWO);
    let archers = spawn(&mut w, 0, ARCHERS, 50, 50, NORTH);
    let ally = spawn(&mut w, 2, SPEARS, 50, 35, SOUTH);
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Attack {
            regiment: archers,
            target: ally,
        },
    }]);
    assert_eq!(stats.commands_rejected, 1);
    run(&mut w, 20 * 10);
    assert_eq!(w.volleys().count(), 0);
    assert_eq!(w.regiments.get(ally).unwrap().models, 24);
    // An enemy at the same spot is shot.
    let enemy = spawn(&mut w, 1, SPEARS, 60, 35, SOUTH);
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Attack {
            regiment: archers,
            target: enemy,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    run(&mut w, 20 * 10);
    assert!(w.regiments.get(enemy).unwrap().models < 24);
}

#[test]
fn allies_count_as_friends_when_outnumbered() {
    // One spear block of player 0 fights one of player 1, with two more of
    // player 1 standing close by. Alone, player 0 is outnumbered three to
    // one and its morale drains; with two allied blocks of player 2 beside
    // it the odds are even and the drain stops.
    let mut units: [UnitStats; 4] = DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u });
    units[1] = UnitStats {
        morale: 100,
        ..DRILL_UNITS[0]
    };
    let morale_after = |with_allies: bool| {
        let mut w = world(&units, &TWO_V_TWO);
        let me = spawn(&mut w, 0, 1, 100, 100, NORTH);
        spawn(&mut w, 1, SPEARS, 100, 97, SOUTH);
        spawn(&mut w, 1, SPEARS, 94, 95, SOUTH);
        spawn(&mut w, 1, SPEARS, 106, 95, SOUTH);
        if with_allies {
            spawn(&mut w, 2, SPEARS, 94, 105, NORTH);
            spawn(&mut w, 2, SPEARS, 106, 105, NORTH);
        }
        run(&mut w, 20 * 6);
        let r = w.regiments.get(me).unwrap();
        assert!(r.engaged);
        r.morale_points
    };
    let alone = morale_after(false);
    let together = morale_after(true);
    assert!(together > alone, "alone {alone}, with allies {together}");
}

#[test]
fn an_allys_rout_shakes_its_friends() {
    // Player 2's block routs next to player 0's: player 0's morale drops.
    let mut units = DRILL_UNITS;
    units[0].morale = 90;
    units[1].morale = 1;
    let mut w = world(&units, &TWO_V_TWO);
    let mine = spawn(&mut w, 0, 0, 60, 60, NORTH);
    let ally = spawn(&mut w, 2, 1, 70, 60, NORTH);
    // An enemy archer block shoots the ally until it breaks.
    let archers = spawn(&mut w, 1, ARCHERS, 70, 45, SOUTH);
    w.tick(&[PlayerCommand {
        player: 1,
        command: Command::Attack {
            regiment: archers,
            target: ally,
        },
    }]);
    let before = w.regiments.get(mine).unwrap().morale_points;
    for _ in 0..20 * 30 {
        w.tick(&[]);
        if w.regiments
            .get(ally)
            .is_some_and(|r| r.morale == MoraleState::Routing)
        {
            break;
        }
    }
    assert_eq!(
        w.regiments.get(ally).map(|r| r.morale),
        Some(MoraleState::Routing)
    );
    // The morale check that follows the rout has run by now.
    run(&mut w, 2);
    let after = w.regiments.get(mine).unwrap().morale_points;
    assert!(after + 5 <= before, "before {before}, after {after}");
}

#[test]
fn a_player_hands_their_army_to_an_ally_and_takes_it_back() {
    let mut w = world(&DRILL_UNITS, &TWO_V_TWO);
    let spears = spawn(&mut w, 2, SPEARS, 50, 50, NORTH);
    let enemy = spawn(&mut w, 1, SPEARS, 50, 20, SOUTH);
    let order = |player, command| PlayerCommand { player, command };
    let hand = |player, to| order(player, Command::HandOver { to });
    let march = |player| {
        order(
            player,
            Command::MoveTo {
                regiment: spears,
                target: FxVec2::from_ints(60, 50),
                facing: None,
                files: 0,
                pace: gc_fixed::Fx::ZERO,
            },
        )
    };
    let untouched = w.state_hash();
    // Nobody else orders player 2's spears, and an army can't go to an
    // enemy; the refusals change nothing that is hashed.
    let stats = w.tick(&[march(0), hand(2, 1), hand(2, 9)]);
    assert_eq!(stats.commands_rejected, 3);
    let mut same = world(&DRILL_UNITS, &TWO_V_TWO);
    spawn(&mut same, 2, SPEARS, 50, 50, NORTH);
    spawn(&mut same, 1, SPEARS, 50, 20, SOUTH);
    same.tick(&[]);
    assert_eq!(w.state_hash(), same.state_hash());
    assert_ne!(untouched, w.state_hash());

    // Handed to player 0, who now gives the orders and player 2 doesn't.
    assert_eq!(w.tick(&[hand(2, 0)]).commands_applied, 1);
    assert_eq!(w.stances().commander(2), 0);
    same.tick(&[]);
    assert_ne!(w.state_hash(), same.state_hash(), "who commands is hashed");
    assert_eq!(w.tick(&[march(2)]).commands_rejected, 1);
    let attack = order(
        0,
        Command::Attack {
            regiment: spears,
            target: enemy,
        },
    );
    assert_eq!(w.tick(&[march(0), attack]).commands_applied, 2);

    // Taken back.
    assert_eq!(w.tick(&[hand(2, 2)]).commands_applied, 1);
    assert_eq!(w.tick(&[march(0)]).commands_rejected, 1);
    assert_eq!(w.tick(&[march(2)]).commands_applied, 1);
}
