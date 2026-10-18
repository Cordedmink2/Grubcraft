//! The six-player Conquest soak (docs/12-performance.md, "Conquest soak"):
//! an hour of game time on a 768-tile Wilds Conquest map, set up as the
//! lobby sets up Conquest, between six scripted sides that play a whole
//! match's worth: each gathers with up to 14 workers, keeps laying houses
//! and barracks, trains soldiers to its supply cap, upgrades its main
//! building and sends waves at its neighbours' bases, in fog of war.
//!
//! It reports each minute's tick times, the simulation's heap, all heap
//! and the process's memory, and fails if the process outgrows the 6 GB
//! budget or the simulation heap its big-map budget. The process here is
//! the simulation, the map and the game data with no window or GPU: the
//! renderer's share of the budget is measured in the game (docs/perf-log.md).
//!
//! ```text
//! cargo test --release -p gc_game conquest_soak -- --ignored --nocapture
//! SOAK_MINUTES=10 cargo test --release -p gc_game conquest_soak -- --ignored --nocapture
//! ```

use std::time::Instant;

use gc_data::Faction;
use gc_fixed::Fx;
use gc_map::Map;
use gc_platform::tagged_alloc::{self, Tag};
use gc_sim::structures::StructureView;
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, PlayerId, RegimentHandle, Resource,
    TICKS_PER_SECOND, Victories, World,
};
use gc_ui::lobby::{SHIPPED_CONQUEST_MAPS, Tempo};

use crate::buildings::Table;
use crate::conquest;
use crate::conquest_golden::{
    Kit, affords, main_of, nodes_near, own, regiments_of, site, trade_orders,
};

const SEED: u64 = 0x50AC_0006;
/// "Land: Mixed": some of every kind of ground.
const MAP: usize = SHIPPED_CONQUEST_MAPS + 2;
const PLAYERS: usize = 6;
/// Each faction twice but the Brood, whose soldiers come from larvae at
/// its Hive (not built yet), so the script has no barracks for it.
const FACTIONS: [Faction; PLAYERS] = [
    Faction::EmberClans,
    Faction::CarrionConcord,
    Faction::Choir,
    Faction::CarrionConcord,
    Faction::Choir,
    Faction::EmberClans,
];
/// Budgets: the whole game's 6 GB, and the simulation's heap on a big
/// map (its walking grid and fog grow with the map's area).
const PROCESS_BUDGET: u64 = 6 << 30;
const SIM_HEAP_BUDGET: usize = 48 << 20;

/// What each side builds up to.
const WORKERS: usize = 14;
const BARRACKS: usize = 3;
/// Caravans a trading side runs.
const CARAVANS: usize = 4;
const FIGHTERS: usize = 60;
/// Waves go out this often, from the first one on.
const FIRST_WAVE: u32 = 8 * 60 * TICKS_PER_SECOND;
const WAVE_EVERY: u32 = 4 * 60 * TICKS_PER_SECOND;

/// One side's orders so far.
#[derive(Clone, Copy)]
struct Side {
    /// Which side its next wave goes at (counting round from its own).
    target: usize,
    /// The building going up, how far along it was and when it got there:
    /// one stuck a minute (its builder can't reach it) is called off.
    going_up: Option<(gc_sim::structures::StructureHandle, u8, u32)>,
}

struct Soak {
    world: World,
    kits: [Kit; PLAYERS],
    sides: [Side; PLAYERS],
    starts: Vec<(i32, i32)>,
    /// Orders the sim turned down (the script doesn't check every rule).
    rejected: u32,
}

/// A minute's figures.
#[derive(Clone, Copy, Debug)]
struct Minute {
    minute: u32,
    regiments: usize,
    structures: usize,
    p50_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    sim_heap: usize,
    heap: usize,
    process: Option<u64>,
}

impl Soak {
    fn new(map: &Map) -> Soak {
        let data = crate::data::load();
        let table = Table::new(&data);
        let units = conquest::unit_stats(&data);
        let nav = map.nav_costs();
        let teams: Vec<u8> = (0..PLAYERS as u8).collect();
        let setup = MatchSetup {
            seed: SEED,
            capacities: Capacities::default(),
            units: &units,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &teams,
            ground: map.ground(),
            champions: false,
        };
        // Annihilation only, so the fog is on but nobody wins by holding
        // the Heart and the hour plays out.
        let victories = Victories {
            annihilation: true,
            heart: false,
        };
        let plan = conquest::plan(&table, &data, map, &FACTIONS, Tempo::Swift, victories);
        let mut world = tagged_alloc::scope(Tag::Sim, || World::new(&setup));
        assert!(tagged_alloc::scope(Tag::Sim, || world.conquest(&plan.setup())));
        let opening = conquest::opening(&data, &table, map, &world, &FACTIONS);
        let stats = tagged_alloc::scope(Tag::Sim, || world.tick(&opening));
        assert_eq!(stats.commands_applied as usize, opening.len());
        Soak {
            kits: FACTIONS.map(|f| Kit::of(&data, &table, f)),
            // Each side's first wave goes at the next side round.
            sides: core::array::from_fn(|p| Side {
                target: p,
                going_up: None,
            }),
            starts: conquest::player_starts(map, PLAYERS),
            world,
            rejected: 0,
        }
    }

    /// Play one minute and say how it went.
    fn minute(&mut self) -> Minute {
        let mut times = Vec::with_capacity((60 * TICKS_PER_SECOND) as usize);
        let mut out = Vec::new();
        // The sim heap's peak over the minute, read after every tick.
        let mut sim_heap = 0;
        for _ in 0..60 * TICKS_PER_SECOND {
            out.clear();
            for p in 0..PLAYERS as PlayerId {
                self.script(p, &mut out);
            }
            let began = Instant::now();
            let stats = tagged_alloc::scope(Tag::Sim, || self.world.tick(&out));
            times.push(began.elapsed().as_secs_f64() * 1000.0);
            sim_heap = sim_heap.max(tagged_alloc::live_bytes(Tag::Sim));
            self.rejected += stats.commands_rejected;
        }
        times.sort_by(f64::total_cmp);
        let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
        Minute {
            minute: self.world.tick_count() / (60 * TICKS_PER_SECOND),
            regiments: self.world.regiments.iter().count(),
            structures: self.world.structures().map_or(0, |s| s.iter().count()),
            p50_ms: at(0.5),
            p99_ms: at(0.99),
            max_ms: at(1.0),
            sim_heap,
            heap: tagged_alloc::total_live_bytes(),
            process: gc_platform::memory::process_memory().current,
        }
    }

    fn main_of(&self, p: PlayerId) -> Option<StructureView> {
        main_of(&self.world, &self.kits[usize::from(p)], p)
    }

    fn script(&mut self, p: PlayerId, out: &mut Vec<PlayerCommand>) {
        let t = self.world.tick_count();
        // Each side thinks once a second, the six on different ticks.
        if t % TICKS_PER_SECOND != u32::from(p) * 3 || self.world.is_out(p) {
            return;
        }
        let k = self.kits[usize::from(p)];
        let order = |command| PlayerCommand { player: p, command };
        let Some(main) = self.main_of(p) else {
            return;
        };
        let workers = regiments_of(&self.world, p, k.worker);
        let mut bank = self.world.bank(p).unwrap_or_default();
        let queue_of = |s: &StructureView| {
            self.world
                .production()
                .map_or(0, |q| q.queue_of(s.handle).len())
        };
        let barracks = own(&self.world, p, Some(k.barracks));
        let houses = own(&self.world, p, Some(k.house));
        let going_up = barracks.iter().chain(&houses).find(|b| !b.finished());
        if let Some(b) = going_up {
            let side = &mut self.sides[usize::from(p)];
            match side.going_up {
                Some((h, done, since)) if h == b.handle && done == b.built_percent => {
                    if t - since > 60 * TICKS_PER_SECOND {
                        out.push(order(Command::CallOff { structure: h }));
                        side.going_up = None;
                        return;
                    }
                }
                _ => side.going_up = Some((b.handle, b.built_percent, t)),
            }
        }
        // With hardly any workers, all of them gather.
        let going_up = going_up.filter(|_| workers.len() >= 3);

        // The first worker builds whatever is going up; every other idle
        // worker gathers, one in five Marrow.
        let idle: Vec<RegimentHandle> = self.world.idle_workers(p).collect();
        let start = self.starts[usize::from(p)];
        let grub = nodes_near(&self.world, start, Resource::Grub);
        let marrow = nodes_near(&self.world, start, Resource::Marrow);
        for (i, &w) in workers.iter().enumerate() {
            if i == 0
                && let Some(b) = going_up
            {
                if idle.contains(&w) || self.world.gathers_at(w).is_some() {
                    out.push(order(Command::Construct {
                        regiment: w,
                        structure: b.handle,
                    }));
                }
                continue;
            }
            // A worker that only builds (the Choir's) can't gather.
            if !idle.contains(&w) || self.world.builds_only(w) {
                continue;
            }
            // A trading faction's Clerks mine Marrow only.
            let node = if i % 5 == 4 || k.caravan.is_some() {
                marrow.get(i % 2)
            } else {
                grub.get(i % 3)
            };
            if let Some(&node) = node {
                out.push(order(Command::Gather { regiment: w, node }));
            }
        }

        // Workers from the main building, its upgrade from minute ten;
        // each order's cost comes off what the later ones can spend.
        // A trading faction hires its caravans first, whose Grub it lives on.
        let (unit, cost) = match k.caravan {
            Some(c) if regiments_of(&self.world, p, c).len() < CARAVANS => (c, k.caravan_cost),
            _ => (k.worker, k.worker_cost),
        };
        if queue_of(&main) == 0
            && (unit != k.worker || workers.len() < WORKERS)
            && affords(&bank, cost)
        {
            out.push(order(Command::Train {
                structure: main.handle,
                unit,
            }));
            spend(&mut bank, cost);
        }
        if k.caravan.is_some() {
            out.extend(trade_orders(&self.world, p, self.starts[usize::from(p)]));
        }
        let upgraded = main.upgrading.is_some() || Some(main.building) == k.keep;
        if t >= 10 * 60 * TICKS_PER_SECOND && !upgraded && affords(&bank, k.keep_cost) {
            out.push(order(Command::Upgrade {
                structure: main.handle,
            }));
            spend(&mut bank, k.keep_cost);
        }

        // A house when supply runs short, else another barracks, one
        // going up at a time.
        if going_up.is_none()
            && let Some(&w) = workers.first()
        {
            let lay = if bank.supply_cap < bank.supply_used + 8 {
                Some(k.house)
            } else {
                (barracks.len() < BARRACKS).then_some(k.barracks)
            };
            if let Some(building) = lay
                && let Some(tile) = site(&self.world, &k, p, building)
            {
                out.push(order(Command::Build {
                    regiment: w,
                    building,
                    tile,
                }));
            }
        }

        // Soldiers from every finished barracks (the Kit falls back on a
        // worker for a faction with none to train there).
        if k.fighter == k.worker {
            return;
        }
        let fighters = regiments_of(&self.world, p, k.fighter);
        for b in barracks.iter().filter(|b| b.finished()) {
            // Workers come first while there are few.
            if workers.len() >= WORKERS / 2
                && fighters.len() < FIGHTERS
                && queue_of(b) == 0
                && affords(&bank, k.fighter_cost)
            {
                out.push(order(Command::Train {
                    structure: b.handle,
                    unit: k.fighter,
                }));
                spend(&mut bank, k.fighter_cost);
            }
        }

        // A wave at the next side still standing, every few minutes:
        // soldiers march at its main building and attack it once seen.
        if t < FIRST_WAVE {
            return;
        }
        if (t - FIRST_WAVE) % WAVE_EVERY < TICKS_PER_SECOND {
            let was = self.sides[usize::from(p)].target;
            let next = (1..=PLAYERS)
                .map(|k| (was + k) % PLAYERS)
                .find(|&q| q != usize::from(p) && !self.world.is_out(q as PlayerId));
            self.sides[usize::from(p)].target = next.unwrap_or(was);
        }
        let target = self.sides[usize::from(p)].target as PlayerId;
        let Some(goal) = self.main_of(target) else {
            return;
        };
        let seen = goal.seen_by(p);
        let ready = self.world.regiments.iter().filter(|r| {
            r.owner == p && r.unit == k.fighter && !r.morale.broken() && !r.moving && !r.engaged
        });
        for r in ready {
            out.push(order(if seen {
                Command::AttackStructure {
                    regiment: r.handle,
                    structure: goal.handle,
                }
            } else {
                Command::MoveTo {
                    regiment: r.handle,
                    target: goal.centre(),
                    facing: None,
                    files: 0,
                    pace: Fx::ZERO,
                }
            }));
        }
    }
}

/// Take `cost` (Grub, Marrow, supply) out of `bank`, which affords it.
fn spend(bank: &mut gc_sim::Bank, [grub, marrow, supply]: [u32; 3]) {
    bank.grub -= grub;
    bank.marrow -= marrow;
    bank.supply_used += supply;
}

fn mb(bytes: f64) -> f64 {
    bytes / f64::from(1u32 << 20)
}

#[test]
#[ignore = "an hour of six-player Conquest: run with --release --ignored --nocapture"]
fn conquest_soak() {
    let minutes: u32 = std::env::var("SOAK_MINUTES")
        .ok()
        .and_then(|m| m.parse().ok())
        .unwrap_or(60);
    let made = Instant::now();
    let map = tagged_alloc::scope(Tag::Map, || conquest::make_map(MAP, SEED));
    println!(
        "map: {} ({} tiles, {} starts) made in {:.1} s",
        gc_ui::lobby::CONQUEST_MAPS[MAP],
        map.tiles,
        map.starts().len(),
        made.elapsed().as_secs_f64()
    );
    let mut soak = Soak::new(&map);
    println!("min  regts  bldgs   p50 ms   p99 ms   max ms  sim MB  heap MB  process MB");
    let mut rows = Vec::new();
    for _ in 0..minutes {
        let m = soak.minute();
        println!(
            "{:>3}  {:>5}  {:>5}  {:>7.2}  {:>7.2}  {:>7.2}  {:>6.1}  {:>7.1}  {:>10}",
            m.minute,
            m.regiments,
            m.structures,
            m.p50_ms,
            m.p99_ms,
            m.max_ms,
            mb(m.sim_heap as f64),
            mb(m.heap as f64),
            m.process
                .map_or("?".into(), |b| format!("{:.0}", mb(b as f64)))
        );
        rows.push(m);
    }
    let worst = rows.iter().map(|m| m.max_ms).fold(0.0, f64::max);
    let peak_sim = rows.iter().map(|m| m.sim_heap).max().unwrap_or(0);
    let peak_process = rows.iter().filter_map(|m| m.process).max().unwrap_or(0);
    let peak_os = gc_platform::memory::process_memory().peak;
    println!(
        "worst tick {worst:.2} ms; sim heap peak {:.1} MB (budget {:.0}); process peak {:.0} MB \
         (OS peak {}; budget {:.0}); orders turned down {}",
        mb(peak_sim as f64),
        mb(SIM_HEAP_BUDGET as f64),
        mb(peak_process as f64),
        peak_os.map_or("?".into(), |b| format!("{:.0} MB", mb(b as f64))),
        mb(PROCESS_BUDGET as f64),
        soak.rejected
    );
    // A match was played: the sides built up and trained armies.
    let most = |f: fn(&Minute) -> usize| rows.iter().map(f).max().unwrap_or(0);
    assert!(most(|m| m.regiments) > PLAYERS * WORKERS, "{rows:?}");
    assert!(most(|m| m.structures) > PLAYERS * 6, "{rows:?}");
    assert!(peak_sim <= SIM_HEAP_BUDGET, "simulation heap over budget");
    assert!(
        peak_os.unwrap_or(peak_process) <= PROCESS_BUDGET,
        "process over the 6 GB budget"
    );
}
