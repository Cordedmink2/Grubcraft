//! Melee, charges, flanks and volleys.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::combat::{CHARGE_TICKS, RUN_UP_TICKS};
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
    world_with(&fearless())
}

fn world_with(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 11,
        capacities: Capacities::default(),
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
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

fn until_engaged(w: &mut World, h: RegimentHandle) {
    for _ in 0..400 {
        if w.regiments.get(h).unwrap().engaged {
            return;
        }
        w.tick(&[]);
    }
    panic!("never made contact");
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
    // Each blow falls on the man across from its striker: the front rank
    // is wounded, and men from the back step up into the gaps whole.
    for h in [a, b] {
        let hp = w.regiments.model_hp(h).unwrap();
        assert!(
            hp[..8].iter().any(|&h| h < 10),
            "both sides are cut: {hp:?}"
        );
        assert!(hp[8..].iter().all(|&h| h == 10), "{hp:?}");
    }
    run(&mut w, 20 * 10);
    for h in [a, b] {
        let hp = w.regiments.model_hp(h).unwrap();
        assert!(hp.len() < 24, "both sides lose men: {hp:?}");
    }
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
/// attacker stands against the given side of it. The defenders are spearmen
/// who can't turn, so a flank or rear stays one for the whole fight.
fn losses_from(side_offset: (i32, i32), defender_facing: Angle, ticks: u32) -> u16 {
    let mut units = fearless().to_vec();
    units.push(UnitStats {
        turn_rate: 0,
        ..units[SPEARS as usize]
    });
    let mut w = world_with(&units);
    let d = spawn(&mut w, 1, units.len() as u16 - 1, 50, 50, defender_facing);
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
    let front = losses_from((0, -3), NORTH, 20 * 6);
    let flank = losses_from((4, 0), NORTH, 20 * 6);
    let rear = losses_from((0, 3), NORTH, 20 * 6);
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
    let world = || world_with(&units);
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
        until_engaged(&mut w, a);
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

/// Charge bonus the attacker carries when it hits a fresh block `gap` tiles
/// beyond where its first foe stood, sent in `wait` ticks after that foe
/// was taken off the field mid-fight.
fn charge_on_second_contact(wait: u32, gap: i32) -> u16 {
    let mut w = world();
    let first = spawn(&mut w, 1, SPEARS, 50, 50, SOUTH);
    let a = spawn(&mut w, 0, SPEARS, 50, 64, NORTH);
    let attack = |w: &mut World, target| {
        w.tick(&[PlayerCommand {
            player: 0,
            command: Command::Attack {
                regiment: a,
                target,
            },
        }]);
    };
    attack(&mut w, first);
    until_engaged(&mut w, a);
    assert!(
        w.regiments.get(a).unwrap().charge > 0,
        "the first contact was no charge"
    );
    // Fight until the charge has spent itself, then lose the foe.
    run(&mut w, u32::from(CHARGE_TICKS));
    w.tick(&[PlayerCommand {
        player: 1,
        command: Command::RemoveRegiment { regiment: first },
    }]);
    assert!(!w.regiments.get(a).unwrap().engaged);
    run(&mut w, wait);
    let second = spawn(&mut w, 1, SPEARS, 50, 50 - gap, SOUTH);
    attack(&mut w, second);
    until_engaged(&mut w, a);
    w.regiments.get(a).unwrap().charge
}

#[test]
fn only_a_run_up_makes_a_charge() {
    // Closing a two-tile gap at once, or after standing a while, is no
    // charge: 2 tiles is under a second's march.
    assert_eq!(charge_on_second_contact(0, 2), 0);
    assert_eq!(charge_on_second_contact(3 * u32::from(RUN_UP_TICKS), 2), 0);
    // A 6-tile run-up is.
    assert!(charge_on_second_contact(0, 6) > 0);
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

/// The melee blows regiment `h` strikes over `ticks`.
fn blows_by(w: &mut World, h: RegimentHandle, ticks: u32) -> Vec<gc_sim::combat::Blow> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        w.tick(&[]);
        out.extend(w.blows().iter().filter(|b| b.striker == h));
    }
    out
}

#[test]
fn each_soldier_strikes_the_man_across_from_him() {
    let mut units = fearless();
    units[SPEARS as usize].reach = true;
    let mut w = world_with(&units);
    let a = spawn(&mut w, 0, SPEARS, 50, 50, NORTH);
    let b = spawn(&mut w, 1, SPEARS, 50, 47, SOUTH);
    let blows = blows_by(&mut w, a, 20 * 3);
    assert!(!blows.is_empty());
    // Spears: the front rank and the second strike, at the foe's front.
    assert!(blows.iter().all(|x| x.target == b));
    assert!(
        blows
            .iter()
            .all(|x| x.striker_slot < 16 && x.target_slot < 8)
    );
    assert!(blows.iter().any(|x| x.striker_slot >= 8));
    // Level fronts: each man in the front rank has his own man.
    let first: Vec<_> = blows.iter().take_while(|x| x.striker_slot < 16).collect();
    let mut men: Vec<u8> = first
        .iter()
        .filter(|x| x.striker_slot < 8)
        .map(|x| x.target_slot)
        .collect();
    men.sort_unstable();
    men.dedup();
    assert_eq!(men.len(), 8, "{first:?}");
}

#[test]
fn the_men_who_fall_are_the_men_struck_down() {
    use gc_sim::combat::Outcome;
    let mut w = world();
    let a = spawn(&mut w, 0, SPEARS, 50, 50, NORTH);
    let b = spawn(&mut w, 1, SPEARS, 50, 47, SOUTH);
    let mut killed = 0;
    for _ in 0..20 * 30 {
        w.tick(&[]);
        for x in w.blows() {
            if let Outcome::Killed { men } = x.outcome
                && x.striker == a
            {
                assert!(x.target_slot < 8, "only the front rank is struck");
                killed += u16::from(men);
            }
        }
    }
    assert!(killed > 0);
    assert_eq!(24 - models(&w, b), killed, "every loss came from a blow");
    // The men who stepped up from the back came whole.
    let hp = w.regiments.model_hp(b).unwrap();
    assert!(hp[8..].iter().all(|&h| h == 10), "{hp:?}");
}

#[test]
fn men_gang_up_where_lines_overlap() {
    let mut units = fearless();
    // A wide block, and a narrow one four men across.
    units[1] = UnitStats {
        files: 4,
        models: 8,
        ..units[0]
    };
    units[0].files = 12;
    let mut w = world_with(&units);
    let a = spawn(&mut w, 0, 0, 50, 50, NORTH);
    let b = spawn(&mut w, 1, 1, 50, 48, SOUTH);
    let blows = blows_by(&mut w, a, 20 * 3);
    let turn: Vec<_> = blows
        .iter()
        .filter(|x| x.striker_slot < 12)
        .take(12)
        .collect();
    assert_eq!(turn.len(), 12);
    let mut on = [0; 4];
    for x in &turn {
        assert_eq!(x.target, b);
        on[x.target_slot as usize] += 1;
    }
    // All four are fought, and the men overlapping their line close in on
    // the two at its ends.
    assert!(on.iter().all(|&n| n >= 1), "{on:?}");
    assert!(on[0] >= 3 && on[3] >= 3, "{on:?}");
}
