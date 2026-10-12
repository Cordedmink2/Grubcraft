//! Headless simulation benchmark with budget gates (docs/12-performance.md).
//!
//! Runs the scripted Drill scenario at maximum speed and reports per-tick time
//! percentiles, simulation heap and process memory. Exits non-zero when a
//! budget is exceeded, so CI can enforce it.
//!
//! Usage: gc_bench [--ticks N] [--regiments-per-player N] [--seed N]

use std::time::Instant;

use gc_platform::tagged_alloc::{self, Tag, TaggedAlloc};
use gc_sim::scenario::Drill;
use gc_sim::{Capacities, MatchSetup, World};

#[global_allocator]
static ALLOC: TaggedAlloc = TaggedAlloc;

/// Simulation memory budget from the budget table.
const SIM_HEAP_BUDGET: usize = 8 * 1024 * 1024;
/// p99 tick budget. The real target is 2 ms on the low-tier laptop running a
/// full game; this skeleton must be far below it on any CI machine.
const P99_TICK_BUDGET_US: f64 = 2_000.0;

fn main() {
    let mut ticks: u32 = 12_000; // ten minutes of game time
    let mut per_player: u32 = 300;
    let mut seed: u64 = 1;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().and_then(|v| v.parse::<u64>().ok());
        match (flag.as_str(), value) {
            ("--ticks", Some(v)) => ticks = v as u32,
            ("--regiments-per-player", Some(v)) => per_player = v as u32,
            ("--seed", Some(v)) => seed = v,
            _ => {
                eprintln!("usage: gc_bench [--ticks N] [--regiments-per-player N] [--seed N]");
                std::process::exit(2);
            }
        }
    }

    let capacities = Capacities {
        regiments: (per_player * 4).max(Capacities::default().regiments),
    };
    let mut world = tagged_alloc::scope(Tag::Sim, || World::new(&MatchSetup { seed, capacities }));
    let mut drill = Drill::new(seed);
    drill.regiments_per_player = per_player;
    let mut commands = Vec::with_capacity(4096);
    let mut times_us = Vec::with_capacity(ticks as usize);

    let mut tick_allocs = 0;
    for _ in 0..ticks {
        drill.commands_for(&world, &mut commands);
        let a0 = tagged_alloc::allocation_count();
        let t0 = Instant::now();
        world.tick(&commands);
        times_us.push(t0.elapsed().as_secs_f64() * 1e6);
        tick_allocs += tagged_alloc::allocation_count() - a0;
    }

    times_us.sort_by(f64::total_cmp);
    let pct = |p: f64| times_us[((times_us.len() - 1) as f64 * p) as usize];
    let sim_heap = tagged_alloc::live_bytes(Tag::Sim);
    let mem = gc_platform::memory::process_memory();

    println!("scenario     drill, 4 players x {per_player} regiments, {ticks} ticks");
    println!("tick p50     {:>9.1} us", pct(0.50));
    println!(
        "tick p99     {:>9.1} us   (budget {P99_TICK_BUDGET_US:.0} us)",
        pct(0.99)
    );
    println!("tick max     {:>9.1} us", pct(1.0));
    println!(
        "sim heap     {:>9.2} MB   (budget {:.0} MB)",
        mb(sim_heap),
        mb(SIM_HEAP_BUDGET)
    );
    println!("tick allocs  {tick_allocs:>9}      (budget 0)");
    if let Some(c) = mem.current {
        println!("process      {:>9.2} MB", mb(c as usize));
    }
    println!("final hash   {:#018x}", world.state_hash());

    let mut failed = false;
    if pct(0.99) > P99_TICK_BUDGET_US {
        eprintln!("FAIL: p99 tick time over budget");
        failed = true;
    }
    if sim_heap > SIM_HEAP_BUDGET {
        eprintln!("FAIL: simulation heap over budget");
        failed = true;
    }
    if tick_allocs > 0 {
        eprintln!("FAIL: the simulation allocated during ticks");
        failed = true;
    }
    if failed {
        std::process::exit(1);
    }
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
