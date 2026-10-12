//! Melee, charges, flanks and volleys.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World};

const SOUTH: Angle = Angle(0x4000);
const EAST: Angle = Angle(0);
/// Drill unit 0: 24 spearmen, 10 hp each, melee only.
const SPEARS: u16 = 0;
/// Drill unit 2: 12 archers with a 25-tile range.
const ARCHERS: u16 = 2;

/// The drill's units with morale switched off (they never rout), so these
/// tests measure the fighting alone; tests/morale.rs covers breaking.
fn fearless() -> [UnitStats; 4] {
    DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u })
}

fn world() -> World {
    World::new(&MatchSetup {
        seed: 11,
        capacities: Capacities::default(),
        units: &fearless(),
        map_tiles: 256,
        nav: &[],
    })
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

fn models(w: &World, h: RegimentHandle) -> u16 {
    w.regiments.get(h).map_or(0, |r| r.models)
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

#[test]
fn blocks_in_contact_fight_until_one_falls() {
    let mut w = world();
    // Two spear blocks face to face, a tile apart (each is 2.55 tiles deep).
    let a = spawn(&mut w, 0, SPEARS, 50, 50, NORTH);
    let b = spawn(&mut w, 1, SPEARS, 50, 47, SOUTH);
    w.tick(&[]);
    assert!(w.regiments.get(a).unwrap().engaged);
    assert!(w.regiments.get(b).unwrap().engaged);
    run(&mut w, 20 * 10);
    assert!(models(&w, a) < 24 && models(&w, b) < 24, "no casualties");
    // Casualties come off the back: the front rank keeps full health.
    let hp = w.regiments.model_hp(a).unwrap();
    assert!(hp[..8].iter().all(|&h| h == 10), "{hp:?}");
    run(&mut w, 20 * 300);
    let alive = [a, b]
        .iter()
        .filter(|&&h| w.regiments.get(h).is_some())
        .count();
    assert_eq!(alive, 1, "the fight should end with one regiment destroyed");
}

#[test]
fn engaged_regiments_ignore_move_orders() {
    let mut w = world();
    let a = spawn(&mut w, 0, SPEARS, 50, 50, NORTH);
    let _b = spawn(&mut w, 1, SPEARS, 50, 47, SOUTH);
    w.tick(&[]);
    let start = w.regiments.get(a).unwrap().position;
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: a,
            target: FxVec2::from_ints(50, 90),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    run(&mut w, 40);
    assert_eq!(w.regiments.get(a).unwrap().position, start);
}

/// Casualties a defender takes from one spear block in `ticks`, when the
/// attacker stands against the given side of it.
fn losses_from(side_offset: (i32, i32), defender_facing: Angle, ticks: u32) -> u16 {
    let mut w = world();
    let d = spawn(&mut w, 1, SPEARS, 50, 50, defender_facing);
    let facing = Angle::of_vec(FxVec2::from_ints(-side_offset.0, -side_offset.1));
    let _a = spawn(
        &mut w,
        0,
        SPEARS,
        50 + side_offset.0,
        50 + side_offset.1,
        facing,
    );
    run(&mut w, ticks);
    24 - models(&w, d)
}

#[test]
fn flank_and_rear_hits_hurt_more() {
    // The defender faces north. Attackers stand in front, to its east, and behind.
    // Short fights: a flanked block turns to face its attacker within seconds.
    let front = losses_from((0, -3), NORTH, 20 * 4);
    let flank = losses_from((4, 0), NORTH, 20 * 4);
    let rear = losses_from((0, 3), NORTH, 20 * 4);
    assert!(
        front < flank && flank <= rear,
        "front {front}, flank {flank}, rear {rear}"
    );
}

#[test]
fn charges_hit_harder_than_standing_fights() {
    // Spearmen with a big charge bonus, so the effect stands out from the dice.
    let mut units = fearless();
    units[0].charge_bonus = 200;
    let world = || {
        World::new(&MatchSetup {
            seed: 11,
            capacities: Capacities::default(),
            units: &units,
            map_tiles: 256,
            nav: &[],
        })
    };
    // Charge in from 14 tiles away, then fight for 3 s.
    let charge = {
        let mut w = world();
        let d = spawn(&mut w, 1, SPEARS, 50, 50, SOUTH);
        let a = spawn(&mut w, 0, SPEARS, 50, 64, NORTH);
        w.tick(&[PlayerCommand {
            player: 0,
            command: Command::Attack {
                regiment: a,
                target: d,
            },
        }]);
        let mut ticks = 0;
        while !w.regiments.get(a).unwrap().engaged {
            w.tick(&[]);
            ticks += 1;
            assert!(ticks < 400, "never made contact");
        }
        run(&mut w, 60);
        24 - models(&w, d)
    };
    // The same blocks starting in contact.
    let standing = {
        let mut w = world();
        let d = spawn(&mut w, 1, SPEARS, 50, 50, SOUTH);
        let _a = spawn(&mut w, 0, SPEARS, 50, 53, NORTH);
        run(&mut w, 61);
        24 - models(&w, d)
    };
    assert!(charge > standing, "charge {charge}, standing {standing}");
}

#[test]
fn archers_shoot_enemies_in_range() {
    let mut w = world();
    let archers = spawn(&mut w, 0, ARCHERS, 50, 70, NORTH);
    let target = spawn(&mut w, 1, SPEARS, 50, 50, SOUTH);
    let mut saw_volley = false;
    for _ in 0..20 * 30 {
        w.tick(&[]);
        saw_volley |= w.volleys().next().is_some();
    }
    assert!(saw_volley);
    assert!(models(&w, target) < 24);
    assert_eq!(models(&w, archers), 12, "archers took losses at range");
}

#[test]
fn archers_out_of_range_hold_fire_until_ordered() {
    let mut w = world();
    let archers = spawn(&mut w, 0, ARCHERS, 50, 90, NORTH);
    let target = spawn(&mut w, 1, SPEARS, 50, 50, EAST);
    run(&mut w, 100);
    assert_eq!(models(&w, target), 24);
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Attack {
            regiment: archers,
            target,
        },
    }]);
    run(&mut w, 20 * 30);
    let a = w.regiments.get(archers).unwrap();
    let dist = (a.position - FxVec2::from_ints(50, 50)).length();
    assert!(dist <= Fx::from_int(25), "archers stopped at {dist:?}");
    assert!(models(&w, target) < 24);
}

#[test]
fn players_cannot_attack_their_own_regiments() {
    let mut w = world();
    let a = spawn(&mut w, 0, SPEARS, 50, 50, NORTH);
    let b = spawn(&mut w, 0, SPEARS, 70, 50, NORTH);
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Attack {
            regiment: a,
            target: b,
        },
    }]);
    assert_eq!(stats.commands_rejected, 1);
}
