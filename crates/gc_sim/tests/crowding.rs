//! Friendly regiments queue in tight ground: a marching block with no room
//! to pass beside a friend holding its ground waits behind it.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const MAP: i32 = 64;

/// The drill units with morale off, so nobody runs.
fn fearless() -> [UnitStats; 4] {
    DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u })
}

/// Open ground, or a corridor 9 tiles wide running north to south: room
/// for one block, not two side by side.
fn nav(corridor: bool) -> Vec<u8> {
    let side = MAP * CELLS_PER_TILE;
    (0..side * side)
        .map(|c| {
            let tile_x = (c % side) / CELLS_PER_TILE;
            if corridor && !(28..37).contains(&tile_x) {
                BLOCKED
            } else {
                1
            }
        })
        .collect()
}

fn world(nav: &[u8]) -> World {
    World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 64 },
        units: &fearless(),
        map_tiles: MAP as u32,
        nav,
        teams: &[],
        ground: Ground::FLAT,
    })
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit: SPEARS,
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

fn order(w: &mut World, command: Command) {
    let stats = w.tick(&[PlayerCommand { player: 0, command }]);
    assert_eq!(stats.commands_applied, 1);
}

fn move_to(regiment: RegimentHandle, x: i32, y: i32) -> Command {
    Command::MoveTo {
        regiment,
        target: FxVec2::from_ints(x, y),
        facing: None,
        files: 0,
        pace: Fx::ZERO,
    }
}

/// How deep two spear blocks overlap, in tiles (0 if apart): the least
/// they would have to move apart along any of their edges.
fn overlap(w: &World, a: RegimentHandle, b: RegimentHandle) -> Fx {
    let stats = &w.units()[SPEARS as usize];
    let block = |h: RegimentHandle| {
        let r = w.regiments.get(h).unwrap();
        let (width, depth) = stats.block_size_in(r.formation, r.files, r.models);
        let forward = r.facing.to_vec();
        let right = FxVec2::new(-forward.y, forward.x);
        (r.position, forward, right, width / 2, depth / 2)
    };
    let (pa, fa, ra, wa, da) = block(a);
    let (pb, fb, rb, wb, db) = block(b);
    let d = pb - pa;
    [fa, ra, fb, rb]
        .iter()
        .map(|&axis| {
            let ext =
                |f: FxVec2, r: FxVec2, w: Fx, d: Fx| w * r.dot(axis).abs() + d * f.dot(axis).abs();
            ext(fa, ra, wa, da) + ext(fb, rb, wb, db) - d.dot(axis).abs()
        })
        .min()
        .unwrap()
        .max(Fx::ZERO)
}

/// A friend fights an enemy; `b` is sent at the enemy from straight
/// behind the friend. Returns the world, the friend, the enemy and `b`.
fn fight_ahead(nav: &[u8]) -> (World, RegimentHandle, RegimentHandle, RegimentHandle) {
    let mut w = world(nav);
    let enemy = spawn(&mut w, 1, 32, 20, SOUTH);
    let friend = spawn(&mut w, 0, 32, 23, NORTH);
    let b = spawn(&mut w, 0, 32, 40, NORTH);
    w.tick(&[]);
    assert!(w.regiments.get(friend).unwrap().engaged);
    order(
        &mut w,
        Command::Attack {
            regiment: b,
            target: enemy,
        },
    );
    (w, friend, enemy, b)
}

#[test]
fn a_column_queues_behind_a_friend_fighting_in_a_narrow_pass() {
    let (mut w, friend, _, b) = fight_ahead(&nav(true));
    let mut worst = Fx::ZERO;
    for _ in 0..20 * 20 {
        w.tick(&[]);
        worst = worst.max(overlap(&w, friend, b));
    }
    // Stopped short (a quarter of a tile of overlap is allowed, a little
    // more as it wheels on the spot), close behind, and not fighting:
    // there is no way past.
    assert!(worst <= Fx::HALF, "overlapped by {worst:?}");
    let rb = w.regiments.get(b).unwrap();
    assert!(!rb.engaged);
    let gap = rb.position.y - w.regiments.get(friend).unwrap().position.y;
    assert!(gap < Fx::from_int(5), "waiting {gap:?} tiles back");
}

#[test]
fn on_open_ground_it_passes_by_the_friend_into_the_fight() {
    // With room beside the friend, blocks pass through each other as they
    // always have (local avoidance in the open comes later).
    let (mut w, _, _, b) = fight_ahead(&nav(false));
    let mut fighting = false;
    for _ in 0..30 * 20 {
        w.tick(&[]);
        fighting |= w.regiments.get(b).is_some_and(|r| r.engaged);
    }
    assert!(fighting, "never reached the enemy");
}

#[test]
fn it_waits_only_so_long_behind_a_friend_standing_in_the_way() {
    let mut w = world(&nav(true));
    let _standing = spawn(&mut w, 0, 32, 30, NORTH);
    let b = spawn(&mut w, 0, 32, 40, NORTH);
    order(&mut w, move_to(b, 32, 15));
    for _ in 0..30 * 20 {
        w.tick(&[]);
    }
    let rb = w.regiments.get(b).unwrap();
    assert!(!rb.moving, "never got past, at {:?}", rb.position);
    assert_eq!(rb.position, FxVec2::from_ints(32, 15));
}

#[test]
fn marching_friends_pass_through_each_other() {
    let mut w = world(&nav(false));
    let a = spawn(&mut w, 0, 32, 20, SOUTH);
    let b = spawn(&mut w, 0, 32, 40, NORTH);
    order(&mut w, move_to(a, 32, 40));
    order(&mut w, move_to(b, 32, 20));
    for _ in 0..20 * 20 {
        w.tick(&[]);
    }
    assert_eq!(w.regiments.position(a), Some(FxVec2::from_ints(32, 40)));
    assert_eq!(w.regiments.position(b), Some(FxVec2::from_ints(32, 20)));
}
