//! Golden determinism test. CI runs this on macOS (arm64), Windows (x64) and
//! Linux (x64); the hashes must match everywhere, which is what lets Mac and
//! Windows players share a lockstep match.
//!
//! If a change intentionally alters simulation behaviour, update the golden
//! values in the same pull request and say why in its description.

use gc_sim::scenario::{DRILL_UNITS, Drill, ribs_map};
use gc_sim::{Capacities, Ground, MatchSetup, World};

const SEED: u64 = 0x6772_7562; // "grub"
const TICKS: u32 = 2_400; // two minutes of game time

/// Expected state hash every 600 ticks (30 s of game time).
const GOLDEN: [(u32, u64); 4] = [
    (600, 0x0e6e_2383_ec10_9501),
    (1200, 0x9ed5_46ad_0d5b_8d09),
    (1800, 0x78ca_0061_8a86_acc1),
    (2400, 0xb88a_b0bf_5bd3_82e0),
];

/// The same drill on the ridged test map, where most orders need routes.
const GOLDEN_RIBS: [(u32, u64); 4] = [
    (600, 0x4b09_f76c_8659_d554),
    (1200, 0xa61c_2972_b879_ca88),
    (1800, 0x40d9_ef52_9d0b_8f6b),
    (2400, 0x3082_21ee_01f5_590d),
];

/// The same drill on terraced, river-cut ground, where high ground and
/// fords change the fighting.
const GOLDEN_GROUND: [(u32, u64); 4] = [
    (600, 0x8352_1690_1569_5a13),
    (1200, 0x1214_9870_a59c_508a),
    (1800, 0x8d7f_62e7_2713_7697),
    (2400, 0x0e17_6d27_b657_e828),
];

/// Terraces a little under a tile high, every 16 tiles, crossed by shallow
/// rivers four tiles wide; the water line is a tile up.
fn terraces() -> Vec<u16> {
    (0..256u32 * 256)
        .map(|i| {
            let (x, y) = (i % 256, i / 256);
            if (30..34).contains(&(y % 64)) {
                0
            } else {
                (512 + ((x / 16 + y / 16) % 3) * 200) as u16
            }
        })
        .collect()
}

fn run() -> Vec<(u32, u64)> {
    run_on(&[], Ground::FLAT)
}

fn run_on(nav: &[u8], ground: Ground) -> Vec<(u32, u64)> {
    let mut world = World::new(&MatchSetup {
        seed: SEED,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav,
        teams: &[],
        ground,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
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
    let got = run_on(&nav, Ground::FLAT);
    assert_eq!(got, run_on(&nav, Ground::FLAT));
    for (g, (tick, hash)) in GOLDEN_RIBS.iter().zip(&got) {
        assert_eq!(g.0, *tick);
        assert_eq!(
            g.1, *hash,
            "state hash at tick {tick} is {hash:#018x}; full run: {got:#x?}"
        );
    }
}

#[test]
fn matches_golden_hashes_on_terrain() {
    let heights = terraces();
    let ground = Ground {
        heights: &heights,
        water_level: 256,
        cover: &[],
        growth: &[],
        footing: &[],
    };
    let got = run_on(&[], ground);
    assert_eq!(got, run_on(&[], ground));
    assert_ne!(got, run(), "the ground changed nothing");
    for (g, (tick, hash)) in GOLDEN_GROUND.iter().zip(&got) {
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
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let b = World::new(&MatchSetup {
        seed: 2,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    assert_ne!(a.state_hash(), b.state_hash());
    a.tick(&[]);
    assert_ne!(a.state_hash(), b.state_hash());
}
