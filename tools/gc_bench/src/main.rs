//! Headless simulation benchmark with budget gates (docs/12-performance.md).
//!
//! Runs scripted scenarios at maximum speed on the ridged test map and reports
//! per-tick time percentiles, simulation heap and process memory. Exits
//! non-zero when a budget is exceeded, so CI can enforce it.
//!
//! Scenarios:
//! - `drill`: four players with 300 regiments each, ordering moves and group
//!   moves and fighting, for ten minutes of game time.
//! - `pathing-storm`: 120 regiments all ordered across the map in the same
//!   tick; measures how long the last one waits for its route.
//!
//! Usage: gc_bench [--scenario drill|pathing-storm] [--ticks N]
//!                 [--regiments-per-player N] [--seed N] [--open]
//! `--open` runs on an open plane with no obstacles (no route searches).
//! With no `--scenario`, runs both.

use std::time::Instant;

use gc_fixed::{Fx, FxVec2};
use gc_platform::tagged_alloc::{self, Tag, TaggedAlloc};
use gc_sim::scenario::{DRILL_UNITS, Drill, NORTH, ribs_map};
use gc_sim::{Capacities, Command, MatchSetup, PlayerCommand, World};

#[global_allocator]
static ALLOC: TaggedAlloc = TaggedAlloc;

/// Simulation memory budget from the budget table.
const SIM_HEAP_BUDGET: usize = 8 * 1024 * 1024;
/// p99 tick budget. The real target is 2 ms on the low-tier laptop running a
/// full game; the benchmark must stay below it on any CI machine.
const P99_TICK_BUDGET_US: f64 = 2_000.0;
/// The pathing storm's last regiment must have its route within this many
/// ticks of the order (2 s).
const STORM_WAIT_BUDGET: u32 = 40;
const MAP_TILES: u32 = 256;

struct Options {
    scenario: Option<String>,
    ticks: Option<u32>,
    per_player: Option<u32>,
    seed: u64,
    /// Run on an open plane instead of the ridged map (no pathfinding).
    open: bool,
}

fn main() {
    let mut opts = Options {
        scenario: None,
        ticks: None,
        per_player: None,
        seed: 1,
        open: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--open" {
            opts.open = true;
            continue;
        }
        let value = args.next();
        let number = value.as_deref().and_then(|v| v.parse::<u64>().ok());
        match (flag.as_str(), number) {
            ("--scenario", _) if value.is_some() => opts.scenario = value,
            ("--ticks", Some(v)) => opts.ticks = Some(v as u32),
            ("--regiments-per-player", Some(v)) => opts.per_player = Some(v as u32),
            ("--seed", Some(v)) => opts.seed = v,
            _ => usage(),
        }
    }

    let nav = if opts.open {
        Vec::new()
    } else {
        ribs_map(MAP_TILES)
    };
    let mut failed = false;
    let run_all = opts.scenario.is_none();
    if run_all || opts.scenario.as_deref() == Some("drill") {
        failed |= drill(&opts, &nav);
    }
    if run_all || opts.scenario.as_deref() == Some("pathing-storm") {
        if run_all {
            println!();
        }
        failed |= pathing_storm(&opts, &nav);
    }
    if !run_all && !matches!(opts.scenario.as_deref(), Some("drill" | "pathing-storm")) {
        usage();
    }
    if failed {
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: gc_bench [--scenario drill|pathing-storm] [--ticks N] \
         [--regiments-per-player N] [--seed N] [--open]"
    );
    std::process::exit(2);
}

fn new_world(seed: u64, regiments: u32, nav: &[u8]) -> World {
    let capacities = Capacities {
        regiments: regiments.max(Capacities::default().regiments),
    };
    tagged_alloc::scope(Tag::Sim, || {
        World::new(&MatchSetup {
            seed,
            capacities,
            units: &DRILL_UNITS,
            map_tiles: MAP_TILES,
            nav,
            teams: &[],
            ground: gc_sim::Ground::FLAT,
        })
    })
}

/// Per-tick timings and allocation count for a run.
struct Timings {
    times_us: Vec<f64>,
    allocs: usize,
}

impl Timings {
    fn new(ticks: u32) -> Timings {
        Timings {
            times_us: Vec::with_capacity(ticks as usize),
            allocs: 0,
        }
    }

    fn tick(&mut self, world: &mut World, commands: &[PlayerCommand]) {
        let a0 = tagged_alloc::allocation_count();
        let t0 = Instant::now();
        world.tick(commands);
        self.times_us.push(t0.elapsed().as_secs_f64() * 1e6);
        self.allocs += tagged_alloc::allocation_count() - a0;
    }

    /// Print the common report; returns true if a budget was missed.
    fn report(mut self, world: &World) -> bool {
        self.times_us.sort_by(f64::total_cmp);
        let pct = |p: f64| self.times_us[((self.times_us.len() - 1) as f64 * p) as usize];
        let sim_heap = tagged_alloc::live_bytes(Tag::Sim);
        let mem = gc_platform::memory::process_memory();
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
        println!("tick allocs  {:>9}      (budget 0)", self.allocs);
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
        if self.allocs > 0 {
            eprintln!("FAIL: the simulation allocated during ticks");
            failed = true;
        }
        failed
    }
}

fn drill(opts: &Options, nav: &[u8]) -> bool {
    let ticks = opts.ticks.unwrap_or(12_000); // ten minutes of game time
    let per_player = opts.per_player.unwrap_or(300);
    let mut world = new_world(opts.seed, per_player * 4, nav);
    let mut drill = Drill::new(opts.seed);
    drill.regiments_per_player = per_player;
    let mut commands = Vec::with_capacity(4096);
    let mut timings = Timings::new(ticks);
    let mut most_queued = 0;
    for _ in 0..ticks {
        drill.commands_for(&world, &mut commands);
        timings.tick(&mut world, &commands);
        most_queued = most_queued.max(world.nav().queued());
    }
    let map = if opts.open {
        "open plane"
    } else {
        "ridged map"
    };
    println!("scenario     drill, 4 players x {per_player} regiments, {ticks} ticks, {map}");
    println!("route queue  {most_queued:>9}      (most waiting at once)");
    timings.report(&world)
}

fn pathing_storm(opts: &Options, nav: &[u8]) -> bool {
    let ticks = opts.ticks.unwrap_or(1_200);
    let per_player = opts.per_player.unwrap_or(30);
    let mut world = new_world(opts.seed, per_player * 4, nav);
    // Each player's regiments stand in a block in their corner...
    let corner = |player: u8| {
        let near = MAP_TILES as i32 / 8;
        let far = MAP_TILES as i32 - near;
        (
            if player & 1 == 0 { near } else { far },
            if player & 2 == 0 { near } else { far },
        )
    };
    let mut spawns = Vec::new();
    for player in 0..4u8 {
        let (cx, cy) = corner(player);
        for k in 0..per_player as i32 {
            spawns.push(PlayerCommand {
                player,
                command: Command::SpawnRegiment {
                    unit: (player % 4) as u16,
                    position: FxVec2::from_ints(cx + (k % 6) * 8 - 20, cy + (k / 6) * 6 - 15),
                    facing: NORTH,
                },
            });
        }
    }
    let mut timings = Timings::new(ticks);
    timings.tick(&mut world, &spawns);
    // ...and all are ordered to the opposite corner in the same tick.
    let orders: Vec<_> = world
        .regiments
        .iter()
        .map(|r| {
            let (cx, cy) = corner(3 - r.owner);
            let k = r.handle.index as i32;
            PlayerCommand {
                player: r.owner,
                command: Command::MoveTo {
                    regiment: r.handle,
                    target: FxVec2::from_ints(cx + (k % 6) * 8 - 20, cy + (k / 6 % 5) * 6 - 15),
                    facing: None,
                    files: 0,
                    pace: Fx::ZERO,
                },
            }
        })
        .collect();
    timings.tick(&mut world, &orders);
    let mut wait = None;
    for t in 1..ticks.saturating_sub(1) {
        if wait.is_none() && world.nav().queued() == 0 {
            wait = Some(t);
        }
        timings.tick(&mut world, &[]);
    }
    let wait = wait.unwrap_or(u32::MAX);
    println!(
        "scenario     pathing-storm, {} regiments ordered across the map at once, {ticks} ticks",
        per_player * 4
    );
    println!(
        "route wait   {wait:>9} ticks (budget {STORM_WAIT_BUDGET}; last regiment's wait for its route)"
    );
    let mut failed = timings.report(&world);
    if wait > STORM_WAIT_BUDGET {
        eprintln!("FAIL: routes took too long to find");
        failed = true;
    }
    failed
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
