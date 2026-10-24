//! Champions before battle: the chance they come out, the duel's stakes,
//! and duels cut short.

use gc_fixed::{Angle, FxVec2};
use gc_sim::champions::{CALL_TILES, HEART};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Challenge, Command, Cover, Ground, MatchSetup, PlayerCommand, RegimentHandle, World,
};

const SOUTH: Angle = Angle(0x4000);
/// Drill unit 0: 24 spearmen, attack 30, defence 45, morale 60.
const SPEARS: u16 = 0;
/// Drill unit 1: 24 skitterlings, attack 40, defence 20, morale 45.
const SKITTERS: u16 = 1;

fn world(seed: u64, champions: bool) -> World {
    world_in(seed, champions, &[])
}

/// A world whose cover map is `cover` (empty: open ground everywhere).
fn world_in(seed: u64, champions: bool, cover: &[u8]) -> World {
    World::new(&MatchSetup {
        seed,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        // Players 0 and 2 against player 1.
        teams: &[0, 1, 0],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover,
            growth: &[],
            footing: &[],
        },
        champions,
        // This rule on its own: generals have tests of their own.
        generals: false,
    })
}

/// A 256-tile cover map with a wood on the tiles `x0..x1` by `y0..y1`.
fn wood(x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<u8> {
    (0..256 * 256)
        .map(|i| {
            let (x, y) = (i % 256, i / 256);
            if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                Cover::Wood as u8
            } else {
                Cover::Open as u8
            }
        })
        .collect()
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

/// Two lines standing a little inside calling distance: player 0's
/// spearmen with player 2's skitterlings beside them, facing player 1's
/// spearmen. Returns the world after two seconds, and the three regiments.
fn face_off(seed: u64, champions: bool) -> (World, [RegimentHandle; 3]) {
    let mut w = world(seed, champions);
    let gap = CALL_TILES - 4;
    let ours = spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
    let ally = spawn(&mut w, 2, SKITTERS, 112, 100, NORTH);
    let theirs = spawn(&mut w, 1, SPEARS, 100, 100 - gap, SOUTH);
    for _ in 0..40 {
        w.tick(&[]);
    }
    (w, [ours, ally, theirs])
}

fn points(w: &World, h: RegimentHandle) -> u8 {
    w.regiments.get(h).unwrap().morale_points
}

/// The first seed from `from` whose face-off calls out champions that
/// fight to a kill.
fn deadly(from: u64) -> (u64, World, [RegimentHandle; 3], Challenge) {
    for seed in from.. {
        let (w, regiments) = face_off(seed, true);
        if let Some(&c) = w.challenge().filter(|c| c.winner.is_some()) {
            return (seed, w, regiments, c);
        }
    }
    unreachable!()
}

#[test]
fn champions_come_out_in_about_half_of_battles() {
    let called = (0..200)
        .filter(|&seed| face_off(seed, true).0.challenge().is_some())
        .count();
    assert!((70..=130).contains(&called), "{called} of 200");
}

#[test]
fn champions_stay_home_unless_the_match_allows_them() {
    for seed in 0..20 {
        let (w, _) = face_off(seed, false);
        assert!(w.challenge().is_none());
    }
}

#[test]
fn the_best_fighter_near_the_front_is_the_champion() {
    let (_, _, [ours, ally, theirs], c) = deadly(0);
    // Spearmen (30 + 45) outclass skitterlings (40 + 20).
    assert!(c.champions.contains(&ours) && c.champions.contains(&theirs));
    assert!(!c.champions.contains(&ally));
    assert!(c.meet > c.called);
}

#[test]
fn the_winners_side_takes_heart_and_the_loser_dies() {
    let (seed, mut w, regiments, c) = deadly(0);
    let winner = c.champions[c.winner.unwrap() as usize];
    let loser = c.champions[1 - c.winner.unwrap() as usize];
    let before: Vec<(u8, u16)> = regiments
        .iter()
        .map(|&h| {
            let r = w.regiments.get(h).unwrap();
            (r.morale_points, r.models)
        })
        .collect();
    while w.tick_count() <= c.ends() {
        // Nothing changes until the last blow.
        for (k, &h) in regiments.iter().enumerate() {
            assert_eq!(points(&w, h), before[k].0, "seed {seed}");
        }
        w.tick(&[]);
    }
    // A few morale checks later the change holds.
    for _ in 0..100 {
        w.tick(&[]);
    }
    for (k, &h) in regiments.iter().enumerate() {
        let r = w.regiments.get(h).unwrap();
        let on_winners_side = !w.hostile(r.owner, w.regiments.get(winner).unwrap().owner);
        let change = i32::from(r.morale_points) - i32::from(before[k].0);
        if on_winners_side {
            assert_eq!(change, i32::from(HEART), "seed {seed}, regiment {k}");
        } else {
            // His regiment also lost him, which lowers its ceiling a little.
            assert!(change <= -i32::from(HEART), "seed {seed}: {change}");
        }
        let lost = before[k].1 - r.models;
        assert_eq!(lost, u16::from(h == loser), "seed {seed}, regiment {k}");
    }
}

#[test]
fn a_duel_cut_short_changes_nothing() {
    let (seed, mut w, regiments, c) = deadly(0);
    let before: Vec<u8> = regiments.iter().map(|&h| points(&w, h)).collect();
    let player = w.regiments.get(c.champions[0]).unwrap().owner;
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::RemoveRegiment {
            regiment: c.champions[0],
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    while w.tick_count() <= c.ends() + 40 {
        w.tick(&[]);
    }
    let after = w.challenge().unwrap();
    assert!(after.abandoned.is_some(), "seed {seed}");
    for (k, &h) in regiments.iter().enumerate() {
        if let Some(r) = w.regiments.get(h) {
            assert_eq!(r.morale_points, before[k], "seed {seed}, regiment {k}");
            assert_eq!(r.models, 24);
        }
    }
}

#[test]
fn the_same_seed_calls_the_same_duel() {
    let (seed, w, _, c) = deadly(5);
    let (again, _) = face_off(seed, true);
    assert_eq!(again.challenge(), Some(&c));
    assert_eq!(again.state_hash(), w.state_hash());
}

#[test]
fn an_enemy_hidden_in_a_wood_calls_out_no_champion() {
    let gap = CALL_TILES - 4;
    let called = |cover: &[u8]| {
        (0..40)
            .filter(|&seed| {
                let mut w = world_in(seed, true, cover);
                spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
                spawn(&mut w, 1, SPEARS, 100, 100 - gap, SOUTH);
                for _ in 0..60 {
                    w.tick(&[]);
                }
                w.challenge().is_some()
            })
            .count()
    };
    assert!(called(&[]) > 0);
    let hidden = 100 - gap as u32;
    assert_eq!(called(&wood(88, 112, hidden - 12, hidden + 12)), 0);
}

#[test]
fn a_regiment_hiding_in_a_wood_sends_no_champion() {
    // Our spearmen, the better fighters, wait in a wood; the skitterlings
    // beside them stand in the open and answer the challenge.
    let cover = wood(88, 112, 88, 112);
    for seed in 0..40 {
        let mut w = world_in(seed, true, &cover);
        let ours = spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
        let ally = spawn(&mut w, 2, SKITTERS, 125, 100, NORTH);
        let theirs = spawn(&mut w, 1, SPEARS, 100, 50, SOUTH);
        for _ in 0..60 {
            w.tick(&[]);
        }
        if let Some(c) = w.challenge() {
            assert_eq!(c.champions, [ally, theirs], "seed {seed}");
            assert!(!c.champions.contains(&ours));
            return;
        }
    }
    panic!("no champions in 40 seeds");
}

#[test]
fn no_champions_once_the_battle_has_begun() {
    // A skirmish already under way on the flank: no challenge, ever.
    for seed in 0..40 {
        let mut w = world(seed, true);
        spawn(&mut w, 0, SPEARS, 30, 200, NORTH);
        spawn(&mut w, 1, SKITTERS, 30, 197, SOUTH);
        for _ in 0..10 {
            w.tick(&[]);
        }
        spawn(&mut w, 0, SPEARS, 150, 100, NORTH);
        spawn(&mut w, 1, SPEARS, 150, 100 - (CALL_TILES - 4), SOUTH);
        for _ in 0..200 {
            w.tick(&[]);
        }
        assert!(w.challenge().is_none(), "seed {seed}");
    }
}

#[test]
fn fighting_anywhere_on_either_side_ends_the_duel() {
    let (seed, mut w, [ours, _, _], c) = deadly(0);
    let before = points(&w, ours);
    // An enemy regiment falls on ours (standing at 100, 100) while the
    // champions fight.
    spawn(&mut w, 1, SKITTERS, 100, 97, SOUTH);
    while w.tick_count() <= c.ends() + 1 {
        w.tick(&[]);
    }
    let after = w.challenge().unwrap();
    assert!(after.abandoned.is_some(), "seed {seed}");
    assert_eq!(after.sides, c.sides);
    // Nobody took heart from it.
    assert!(points(&w, ours) <= before, "seed {seed}");
}
