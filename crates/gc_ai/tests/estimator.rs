//! The battle estimator against the real simulation (docs/18-ai.md, "Engage
//! or withdraw"): it must call the winner of at least 80% of drill battles.
//!
//! A drill battle is a small fight between two groups of one to five
//! regiments drawn from the drill units (spears, fast troops, archers,
//! pikes), set out facing each other and both charging straight in; half
//! of them have as many regiments on each side. The
//! estimator sees each side as the AI would at the start, and its call is
//! the sign of its margin. Battles nobody wins inside the time limit, and
//! mirror matches (a coin toss), don't count.
//!
//! `AI_BATTLES=400 cargo test -p gc_ai --release --test estimator calls -- --ignored --nocapture`
//! prints every miss over more battles, for tuning.

use gc_ai::army::{Placement, spawn_commands};
use gc_ai::estimator::{self, Approach, Force};
use gc_ai::scripted::ChargeBot;
use gc_ai::{Perception, holding_the_field};
use gc_fixed::Pcg32;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{Capacities, MatchSetup, PlayerCommand, PlayerId, UnitId, World};
use glam::Vec2;

const MAP: u32 = 256;
/// A battle is won once one side has held the field this long, ticks.
const HELD: u32 = 10 * 20;
const LIMIT: u32 = 4 * 60 * 20;

struct Battle {
    seed: u64,
    armies: [Vec<UnitId>; 2],
    gap: f32,
    /// The estimator's margin for side 0.
    margin: f32,
    winner: Option<PlayerId>,
}

/// The two armies of drill battle `seed`.
fn armies(seed: u64) -> ([Vec<UnitId>; 2], f32) {
    let mut rng = Pcg32::new(seed, 0xE5);
    // Every other battle has the same number of regiments a side: the
    // close calls.
    let n = 1 + rng.below(5) as usize;
    let m = if seed.is_multiple_of(2) {
        n
    } else {
        1 + rng.below(5) as usize
    };
    let mut pick = |n: usize| -> Vec<UnitId> { (0..n).map(|_| rng.below(4) as UnitId).collect() };
    let a = pick(n);
    let b = pick(m);
    let gap = 20.0 + rng.below(21) as f32;
    ([a, b], gap)
}

/// Set an army out in one row centred on `centre`, facing `forward`.
fn row(army: &[UnitId], centre: Vec2, forward: Vec2) -> Vec<Placement> {
    let right = Vec2::new(-forward.y, forward.x);
    let width = |u: UnitId| {
        let s = &DRILL_UNITS[u as usize];
        s.block_size_in(gc_sim::Formation::Line, s.files, s.models)
            .0
            .raw() as f32
            / 65536.0
    };
    let total: f32 = army.iter().map(|&u| width(u) + 2.0).sum::<f32>() - 2.0;
    let mut x = -total * 0.5;
    army.iter()
        .map(|&u| {
            let w = width(u);
            let p = centre + right * (x + w * 0.5);
            x += w + 2.0;
            Placement {
                unit: u,
                position: p,
                facing: forward,
            }
        })
        .collect()
}

fn fight(seed: u64) -> Battle {
    let (armies, gap) = armies(seed);
    let mut world = World::new(&MatchSetup {
        seed,
        capacities: Capacities { regiments: 16 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
    });
    let mid = Vec2::splat(128.0);
    let mut out: Vec<PlayerCommand> = Vec::new();
    for party in 0..2u8 {
        let forward = if party == 0 { Vec2::Y } else { -Vec2::Y };
        // `gap` tiles between the fronts, give or take the blocks' depths.
        let centre = mid - forward * (gap * 0.5 + 1.5);
        spawn_commands(
            party,
            &row(&armies[party as usize], centre, forward),
            &mut out,
        );
    }
    world.tick(&out);
    let mut view = Perception::new();
    view.capture(&world);
    let side = |p: PlayerId| Force::of(&view, view.regiments.iter().filter(|r| r.owner == p));
    let (a, b) = (side(0), side(1));
    let est = estimator::estimate(
        &a,
        &b,
        Approach {
            gap,
            closing: a.speed + b.speed,
        },
    );
    let mut bots = [ChargeBot::new(0), ChargeBot::new(1)];
    let mut held: Option<(PlayerId, u32)> = None;
    let mut winner = None;
    while world.tick_count() < LIMIT {
        out.clear();
        view.capture(&world);
        for bot in &mut bots {
            bot.tick(&view, &mut out);
        }
        world.tick(&out);
        if world.tick_count().is_multiple_of(20) {
            view.capture(&world);
            match (holding_the_field(&view), held) {
                (Some(p), Some((q, since))) if p == q => {
                    if world.tick_count() >= since + HELD {
                        winner = Some(p);
                        break;
                    }
                }
                (Some(p), _) => held = Some((p, world.tick_count())),
                (None, _) => held = None,
            }
        }
    }
    Battle {
        seed,
        armies,
        gap,
        margin: est.margin,
        winner,
    }
}

/// Battles decided, and how many of those the estimator called.
fn score(seeds: std::ops::RangeInclusive<u64>, show_misses: bool) -> (u32, u32) {
    let (mut decided, mut called) = (0, 0);
    for seed in seeds {
        let b = fight(seed);
        let Some(w) = b.winner else {
            continue;
        };
        // The same regiments on both sides: a coin toss nobody can call.
        let sorted = |a: &[UnitId]| {
            let mut a = a.to_vec();
            a.sort_unstable();
            a
        };
        if sorted(&b.armies[0]) == sorted(&b.armies[1]) {
            continue;
        }
        decided += 1;
        let right = (b.margin > 0.0) == (w == 0);
        called += u32::from(right);
        if show_misses && !right {
            println!(
                "seed {}: {:?} v {:?} at {} tiles: margin {:+.2}, but side {w} won",
                b.seed, b.armies[0], b.armies[1], b.gap, b.margin
            );
        }
    }
    (decided, called)
}

#[test]
fn the_estimator_calls_eight_drill_battles_in_ten() {
    let (decided, called) = score(1..=60, true);
    assert!(decided >= 50, "only {decided} of 60 battles were decided");
    assert!(
        called * 10 >= decided * 8,
        "called {called} of {decided} decided battles"
    );
}

#[test]
#[ignore = "slow: many arena battles, run by hand"]
fn calls() {
    let n: u64 = std::env::var("AI_BATTLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let from: u64 = std::env::var("AI_FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let (decided, called) = score(from..=from + n - 1, true);
    println!(
        "called {called} of {decided} decided ({:.0}%), {} undecided",
        100.0 * called as f32 / decided.max(1) as f32,
        n as u32 - decided
    );
}
