//! Golden determinism test. CI runs this on macOS (arm64), Windows (x64) and
//! Linux (x64); the hashes must match everywhere, which is what lets Mac and
//! Windows players share a lockstep match.
//!
//! If a change intentionally alters simulation behaviour, update the golden
//! values in the same pull request and say why in its description.

use gc_sim::scenario::{DRILL_UNITS, Drill, ribs_map};
use gc_sim::{Capacities, MatchSetup, World};

const SEED: u64 = 0x6772_7562; // "grub"
const TICKS: u32 = 2_400; // two minutes of game time

/// Expected state hash every 600 ticks (30 s of game time).
const GOLDEN: [(u32, u64); 4] = [
    (600, 0x84c8_849d_0746_9ce7),
    (1200, 0x1592_2df6_5357_96a1),
    (1800, 0xee6a_7651_856a_4247),
    (2400, 0x2c09_ff88_e17f_eb54),
];

/// The same drill on the ridged test map, where most orders need routes.
const GOLDEN_RIBS: [(u32, u64); 4] = [
    (600, 0x00e2_2478_5eda_46c5),
    (1200, 0x469b_5641_102d_a326),
    (1800, 0x3522_6542_6cdd_2c4b),
    (2400, 0xf7ab_8dd6_2098_59ab),
];

fn run() -> Vec<(u32, u64)> {
    run_on(&[])
}

fn run_on(nav: &[u8]) -> Vec<(u32, u64)> {
    let mut world = World::new(&MatchSetup {
        seed: SEED,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav,
    });
    let mut drill = Drill::new(SEED);
    let mut commands = Vec::with_capacity(512);
    let mut checkpoints = Vec::new();
    for _ in 0..TICKS {
        drill.commands_for(&world, &mut commands);
        world.tick(&commands);
        if world.tick_count().is_multiple_of(600) {
            checkpoints.push((world.tick_count(), world.state_hash()));
        }
    }
    checkpoints
}

#[test]
fn same_inputs_same_hashes() {
    assert_eq!(run(), run());
}

#[test]
fn matches_golden_hashes() {
    let got = run();
    for (g, (tick, hash)) in GOLDEN.iter().zip(&got) {
        assert_eq!(g.0, *tick);
        assert_eq!(
            g.1, *hash,
            "state hash at tick {tick} is {hash:#018x}; full run: {got:#x?}"
        );
    }
}

#[test]
fn matches_golden_hashes_with_pathfinding() {
    let nav = ribs_map(256);
    let got = run_on(&nav);
    assert_eq!(got, run_on(&nav));
    for (g, (tick, hash)) in GOLDEN_RIBS.iter().zip(&got) {
        assert_eq!(g.0, *tick);
        assert_eq!(
            g.1, *hash,
            "state hash at tick {tick} is {hash:#018x}; full run: {got:#x?}"
        );
    }
}

#[test]
fn different_seed_different_hash() {
    let mut a = World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
    });
    let b = World::new(&MatchSetup {
        seed: 2,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
    });
    assert_ne!(a.state_hash(), b.state_hash());
    a.tick(&[]);
    assert_ne!(a.state_hash(), b.state_hash());
}
