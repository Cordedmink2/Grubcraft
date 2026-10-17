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
//! - `field-8000pt-2v2`: the recorded grand battle in
//!   `replays/field-8000pt-2v2.gcr`, played back: four armies of 1,500
//!   soldiers on a generated 384-tile map, charging into one brawl.
//! - `conquest-6p`: six players in a Conquest match played in fog of war on
//!   a 768-tile ridged map, each with 100 regiments drilling about the map
//!   and 40 buildings keeping watch round its corner, laid one a tick at
//!   first. `--no-fog` plays it as the sandbox does, with no fog, to show
//!   what fog costs. Run only when asked for: the budgets below are for
//!   256-tile maps, and a 768-tile map's walking grid alone is bigger than
//!   the heap budget (big-map budgets come with the six-player soak).
//!
//! Usage: gc_bench [--scenario drill|pathing-storm|field-8000pt-2v2|conquest-6p]
//!                 [--ticks N] [--regiments-per-player N] [--seed N] [--open]
//!                 [--no-fog]
//! `--open` runs the scripted scenarios on an open plane with no obstacles
//! (no route searches); `--ticks` cuts the replay short. The grand battle
//! takes its map, armies and seed from the replay. With no `--scenario`,
//! runs the first three.

use std::time::Instant;

use gc_fixed::{Fx, FxVec2};
use gc_platform::tagged_alloc::{self, Tag, TaggedAlloc};
use gc_replay::Replay;
use gc_sim::scenario::{DRILL_UNITS, Drill, NORTH, ribs_map};
use gc_sim::structures::{Armour, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, MatchSetup, PlayerCommand, Victories, World,
};

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
/// The recorded grand battle, built into the benchmark so it runs from
/// any folder.
const GRAND_BATTLE: &[u8] = include_bytes!("../../../replays/field-8000pt-2v2.gcr");
const SCENARIOS: [&str; 4] = ["drill", "pathing-storm", "field-8000pt-2v2", "conquest-6p"];
/// The six-player Conquest map's edge, the largest map size.
const CONQUEST_TILES: u32 = 768;

struct Options {
    scenario: Option<String>,
    ticks: Option<u32>,
    per_player: Option<u32>,
    seed: u64,
    /// Run on an open plane instead of the ridged map (no pathfinding).
    open: bool,
    /// Play `conquest-6p` with no fog of war.
    no_fog: bool,
}

fn main() {
    let mut opts = Options {
        scenario: None,
        ticks: None,
        per_player: None,
        seed: 1,
        open: false,
        no_fog: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--open" {
            opts.open = true;
            continue;
        }
        if flag == "--no-fog" {
            opts.no_fog = true;
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

    let run_all = opts.scenario.is_none();
    let scripted = matches!(
        opts.scenario.as_deref(),
        None | Some("drill" | "pathing-storm")
    );
    let nav = if scripted {
        opts.nav(MAP_TILES)
    } else {
        Vec::new()
    };
    let mut failed = false;
    if run_all || opts.scenario.as_deref() == Some("drill") {
        failed |= drill(&opts, &nav);
    }
    if run_all || opts.scenario.as_deref() == Some("pathing-storm") {
        if run_all {
            println!();
        }
        failed |= pathing_storm(&opts, &nav);
    }
    if run_all || opts.scenario.as_deref() == Some("field-8000pt-2v2") {
        if run_all {
            println!();
        }
        failed |= grand_battle(&opts);
    }
    if opts.scenario.as_deref() == Some("conquest-6p") {
        failed |= conquest_six(&opts);
    }
    if !run_all && !SCENARIOS.contains(&opts.scenario.as_deref().unwrap_or("")) {
        usage();
    }
    if failed {
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: gc_bench [--scenario {}] [--ticks N] \
         [--regiments-per-player N] [--seed N] [--open] [--no-fog]",
        SCENARIOS.join("|")
    );
    std::process::exit(2);
}

impl Options {
    /// The walking grid of the map the scripted scenarios run on, `tiles`
    /// across (empty for the open plane).
    fn nav(&self, tiles: u32) -> Vec<u8> {
        if self.open {
            Vec::new()
        } else {
            ribs_map(tiles)
        }
    }

    fn map_name(&self) -> &'static str {
        if self.open {
            "open plane"
        } else {
            "ridged map"
        }
    }
}

fn new_world(seed: u64, regiments: u32, nav: &[u8], map_tiles: u32) -> World {
    let capacities = Capacities {
        regiments: regiments.max(Capacities::default().regiments),
    };
    tagged_alloc::scope(Tag::Sim, || {
        World::new(&MatchSetup {
            seed,
            capacities,
            units: &DRILL_UNITS,
            map_tiles,
            nav,
            teams: &[],
            ground: gc_sim::Ground::FLAT,
            champions: false,
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
    let mut world = new_world(opts.seed, per_player * 4, nav, MAP_TILES);
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
    let map = opts.map_name();
    println!("scenario     drill, 4 players x {per_player} regiments, {ticks} ticks, {map}");
    println!("route queue  {most_queued:>9}      (most waiting at once)");
    timings.report(&world)
}

fn pathing_storm(opts: &Options, nav: &[u8]) -> bool {
    let ticks = opts.ticks.unwrap_or(1_200);
    let per_player = opts.per_player.unwrap_or(30);
    let mut world = new_world(opts.seed, per_player * 4, nav, MAP_TILES);
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
                    unit: u16::from(player % 4),
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

fn grand_battle(opts: &Options) -> bool {
    let replay = Replay::decode(GRAND_BATTLE).expect("the grand battle replay is built in");
    let ticks = opts.ticks.unwrap_or(replay.ticks).min(replay.ticks);
    let mut world = tagged_alloc::scope(Tag::Sim, || replay.new_world());
    let mut timings = Timings::new(ticks);
    let mut most_in_melee = (0, 0);
    let (mut regiments, mut soldiers) = (0, 0);
    let last_check = replay
        .checkpoints
        .iter()
        .rev()
        .find(|&&(t, _)| t <= ticks)
        .copied();
    let mut golden = None;
    for tick in 0..ticks {
        timings.tick(&mut world, replay.commands_at(tick));
        if tick == 0 {
            regiments = world.regiments.len();
            soldiers = world
                .regiments
                .iter()
                .map(|r| u32::from(r.models))
                .sum::<u32>();
        }
        let in_melee: u32 = world
            .regiments
            .iter()
            .filter(|r| r.engaged)
            .map(|r| u32::from(r.models))
            .sum();
        if in_melee > most_in_melee.0 {
            most_in_melee = (in_melee, tick + 1);
        }
        if let Some((t, hash)) = last_check
            && t == tick + 1
        {
            golden = Some(world.state_hash() == hash);
        }
    }
    let at = most_in_melee.1 / gc_sim::TICKS_PER_SECOND;
    println!(
        "scenario     field-8000pt-2v2, replayed: {regiments} regiments, {soldiers} soldiers, \
         {ticks} ticks, {}-tile map",
        replay.map_tiles
    );
    println!(
        "in melee     {:>9}      (most soldiers fighting at once, at {}:{:02})",
        most_in_melee.0,
        at / 60,
        at % 60
    );
    if golden == Some(false) {
        // Not a failure here: `gc_replay verify` is the determinism check.
        println!("note         the simulation no longer plays this replay as recorded");
    }
    timings.report(&world)
}

/// Six players in Conquest on the largest map, in fog of war unless
/// `--no-fog`: the drill's orders, and buildings round each corner.
fn conquest_six(opts: &Options) -> bool {
    const PLAYERS: u8 = 6;
    const BUILDINGS_PER_PLAYER: i32 = 40;
    let ticks = opts.ticks.unwrap_or(6_000); // five minutes of game time
    let per_player = opts.per_player.unwrap_or(100);
    let nav = opts.nav(CONQUEST_TILES);
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 0,
        cost_marrow: 0,
        build_ticks: 1,
        hp: 3000,
        armour: Armour::Fortified,
        width: 3,
        depth: 3,
        supply: 10,
        flags: 0,
        attack: None,
    };
    let buildings = [
        hall,
        BuildingStats {
            role: Role::Housing,
            width: 2,
            depth: 2,
            ..hall
        },
        BuildingStats {
            role: Role::Defence,
            width: 2,
            depth: 2,
            ..hall
        },
    ];
    let fog = !opts.no_fog;
    let mut world = new_world(
        opts.seed,
        per_player * u32::from(PLAYERS),
        &nav,
        CONQUEST_TILES,
    );
    let set_up = tagged_alloc::scope(Tag::Sim, || {
        world.conquest(&ConquestSetup {
            buildings: &buildings,
            structures: 512,
            victories: Victories {
                annihilation: fog,
                heart: false,
            },
            ..ConquestSetup::default()
        })
    });
    assert!(set_up);
    let mut drill = Drill::new(opts.seed);
    drill.players = PLAYERS;
    drill.regiments_per_player = per_player;
    drill.map_tiles = CONQUEST_TILES as i32;
    let mut commands = Vec::with_capacity(4096);
    // Each player's buildings in rows south of where its regiments start
    // (the drill's corners): a hall, then houses and a tower in every five.
    let mut setup = Vec::new();
    for player in 0..PLAYERS {
        let (cx, cy) = drill.corner(player);
        for k in 0..BUILDINGS_PER_PLAYER {
            let building = match k {
                0 => 0,
                k if k % 5 == 0 => 2,
                _ => 1,
            };
            let tile = (cx - 40 + (k % 10) * 8, cy + 45 + (k / 10) * 8);
            setup.push(PlayerCommand {
                player,
                command: Command::SpawnStructure {
                    building,
                    tile: (tile.0 as i16, tile.1 as i16),
                },
            });
        }
    }
    // One building goes up each tick at first, as players lay them one by
    // one; each changes the walking grid.
    let mut setup = setup.into_iter();
    let mut timings = Timings::new(ticks);
    let mut laid_us = Vec::new();
    let standing = |w: &World| w.structures().map_or(0, |s| s.iter().count());
    for _ in 0..ticks {
        drill.commands_for(&world, &mut commands);
        commands.extend(setup.next());
        let before = standing(&world);
        timings.tick(&mut world, &commands);
        if standing(&world) > before {
            laid_us.extend(timings.times_us.last());
        }
    }
    let standing = standing(&world);
    let map = opts.map_name();
    println!(
        "scenario     conquest-6p, {PLAYERS} players x {per_player} regiments, {standing} buildings, \
         {ticks} ticks, {CONQUEST_TILES}-tile {map}, {}",
        if fog { "fog of war" } else { "no fog" }
    );
    laid_us.sort_by(f64::total_cmp);
    if let (Some(typical), Some(worst)) = (laid_us.get(laid_us.len() / 2), laid_us.last()) {
        println!(
            "laying       {typical:>9.1} us   (typical tick a building went up in; worst {worst:.1} us)"
        );
    }
    timings.report(&world)
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
