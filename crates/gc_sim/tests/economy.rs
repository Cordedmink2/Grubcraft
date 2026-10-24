//! The Conquest economy: workers gather Grub and Marrow and carry it home,
//! seeps and veins run to dregs, supply and upkeep (docs/04-economy.md).

use gc_fixed::{Fx, FxVec2};
use gc_sim::Refusal;
use gc_sim::economy::{GRUB_SLOTS, MARROW_SLOTS, Resource};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup, NodeSetup,
    PlayerCommand, RegimentHandle, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

const WORKER: u16 = 0;
const SPEARS: u16 = 1;
/// A worker type that takes 70 supply, to put a player into upkeep.
const HEAVY: u16 = 2;
const HALL: u16 = 0;
const HOUSE: u16 = 1;

const MINUTE: u32 = 60 * TICKS_PER_SECOND;

fn units() -> [UnitStats; 3] {
    // As the rosters' workers: one model, turning 180 degrees a second.
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    [worker, DRILL_UNITS[0], worker]
}

fn conquest_units() -> [ConquestUnit; 3] {
    let worker = ConquestUnit {
        worker: true,
        supply: 1,
        ..ConquestUnit::default()
    };
    [
        worker,
        ConquestUnit {
            supply: 2,
            ..ConquestUnit::default()
        },
        ConquestUnit {
            supply: 70,
            ..worker
        },
    ]
}

/// A hall that takes both resources, and a house that only adds supply.
fn buildings() -> [BuildingStats; 2] {
    let base = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 400,
        cost_marrow: 0,
        build_ticks: 500,
        hp: 3000,
        armour: Armour::Fortified,
        width: 5,
        depth: 5,
        supply: 15,
        flags: DROP_OFF_GRUB | DROP_OFF_MARROW,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
    };
    [
        base,
        BuildingStats {
            role: Role::Housing,
            width: 2,
            depth: 2,
            supply: 10,
            flags: 0,
            ..base
        },
    ]
}

fn grub(x: i32, y: i32, amount: u32) -> NodeSetup {
    NodeSetup {
        resource: Resource::Grub,
        at: FxVec2::from_ints(x, y),
        amount,
    }
}

fn marrow(x: i32, y: i32, amount: u32) -> NodeSetup {
    NodeSetup {
        resource: Resource::Marrow,
        at: FxVec2::from_ints(x, y),
        amount,
    }
}

const TILES: u32 = 64;

/// An open map 64 tiles across, in Conquest, with these nodes.
fn conquest(nodes: &[NodeSetup], gather_percent: u32) -> World {
    conquest_on(|_, _| false, nodes, gather_percent)
}

/// The same, with the tiles `rough` says no regiment can walk on.
fn conquest_on(
    rough: impl Fn(i32, i32) -> bool,
    nodes: &[NodeSetup],
    gather_percent: u32,
) -> World {
    let side = TILES as usize * CELLS_PER_TILE as usize;
    let cells = CELLS_PER_TILE;
    let nav: Vec<u8> = (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as i32, (i / side) as i32);
            if rough(x / cells, y / cells) {
                BLOCKED
            } else {
                1
            }
        })
        .collect();
    let mut w = World::new(&MatchSetup {
        seed: 4,
        capacities: Capacities { regiments: 64 },
        units: &units(),
        map_tiles: TILES,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    });
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (500, 100);
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &conquest_units(),
        nodes,
        start,
        gather_percent,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile: (x, y),
            turn: 0,
        }
    ));
    let s = w.structures().unwrap();
    s.iter()
        .find(|v| v.tile == (i32::from(x), i32::from(y)))
        .unwrap()
        .handle
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn gather(w: &mut World, player: u8, regiment: RegimentHandle, node: u16) -> bool {
    order(w, player, Command::Gather { regiment, node })
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Player 0's hall at (20, 20) to (25, 25), with `n` workers beside it
/// sent to node 0.
fn base_with_workers(w: &mut World, n: i32) -> Vec<RegimentHandle> {
    put_up(w, 0, HALL, 20, 20);
    (0..n)
        .map(|i| {
            let r = spawn(w, 0, WORKER, 27, 20 + i);
            assert!(gather(w, 0, r, 0));
            r
        })
        .collect()
}

/// What player 0 brings in a minute, over three minutes after one to
/// settle into the round.
fn income_per_minute(w: &mut World, of: Resource) -> u32 {
    let banked = |w: &World| {
        let b = w.bank(0).unwrap();
        match of {
            Resource::Grub => b.grub,
            Resource::Marrow => b.marrow,
        }
    };
    run(w, MINUTE);
    let before = banked(w);
    run(w, 3 * MINUTE);
    (banked(w) - before) / 3
}

#[test]
fn a_conquest_match_starts_with_its_stores_and_nodes() {
    let w = conquest(&[grub(35, 22, 7500), marrow(35, 30, 3000)], 100);
    let e = w.economy().unwrap();
    let b = w.bank(0).unwrap();
    assert_eq!(
        (b.grub, b.marrow, b.supply_used, b.supply_cap),
        (500, 100, 0, 0)
    );
    assert_eq!(w.bank(1).unwrap().grub, 0);
    let nodes: Vec<_> = e.nodes().collect();
    assert_eq!(nodes.len(), 2);
    assert_eq!((nodes[1].resource, nodes[1].left), (Resource::Marrow, 3000));
}

#[test]
fn five_workers_on_a_seep_bring_in_about_200_grub_a_minute() {
    // The seep's centre 7 tiles east of the hall's edge.
    let mut w = conquest(&[grub(32, 22, 7500)], 100);
    base_with_workers(&mut w, 5);
    let income = income_per_minute(&mut w, Resource::Grub);
    assert!((190..=225).contains(&income), "{income} Grub a minute");
    let left = w.economy().unwrap().nodes().next().unwrap().left;
    assert!(left <= 7500 - 4 * 190, "the seep holds {left}");
}

#[test]
fn three_workers_on_a_vein_bring_in_about_50_marrow_a_minute() {
    let mut w = conquest(&[marrow(32, 22, 3000)], 100);
    base_with_workers(&mut w, 3);
    let income = income_per_minute(&mut w, Resource::Marrow);
    assert!((45..=55).contains(&income), "{income} Marrow a minute");
}

#[test]
fn swift_tempo_gathers_faster() {
    let income = |tempo: u32| {
        let mut w = conquest(&[grub(32, 22, 7500)], tempo);
        base_with_workers(&mut w, 5);
        income_per_minute(&mut w, Resource::Grub)
    };
    // Loads fill in two thirds of the time; the walk takes as long.
    let (standard, swift) = (income(100), income(150));
    assert!(
        swift > standard * 115 / 100,
        "swift {swift}, standard {standard}"
    );
}

#[test]
fn a_node_takes_only_so_many_workers_at_once() {
    for (node, slots) in [
        (grub(35, 22, 7500), GRUB_SLOTS),
        (marrow(35, 22, 3000), MARROW_SLOTS),
    ] {
        let mut w = conquest(&[node], 100);
        base_with_workers(&mut w, 7);
        let mut most = 0;
        for _ in 0..2 * MINUTE {
            w.tick(&[]);
            most = most.max(w.economy().unwrap().nodes().next().unwrap().workers);
        }
        assert_eq!(most, slots);
    }
}

#[test]
fn a_dry_seep_still_yields_a_little() {
    let mut full = conquest(&[grub(35, 22, 7500)], 100);
    base_with_workers(&mut full, 5);
    let full = income_per_minute(&mut full, Resource::Grub);
    // Ten Grub: dry after the first load.
    let mut dry = conquest(&[grub(35, 22, 10)], 100);
    base_with_workers(&mut dry, 5);
    let dregs = income_per_minute(&mut dry, Resource::Grub);
    assert_eq!(dry.economy().unwrap().nodes().next().unwrap().left, 0);
    assert!(dregs > 0 && dregs < full / 2, "dregs {dregs}, full {full}");
}

#[test]
fn buildings_raise_the_supply_cap_up_to_150() {
    let mut w = conquest(&[], 100);
    put_up(&mut w, 0, HALL, 2, 2);
    put_up(&mut w, 0, HOUSE, 10, 2);
    spawn(&mut w, 0, WORKER, 30, 30);
    spawn(&mut w, 0, SPEARS, 30, 40);
    w.tick(&[]);
    let b = w.bank(0).unwrap();
    assert_eq!((b.supply_used, b.supply_cap), (3, 25));
    for i in 0..15 {
        put_up(
            &mut w,
            0,
            HOUSE,
            2 + 4 * (i % 5) as i16,
            20 + 4 * (i / 5) as i16,
        );
    }
    w.tick(&[]);
    assert_eq!(w.bank(0).unwrap().supply_cap, 150);
}

#[test]
fn upkeep_cuts_grub_but_not_marrow() {
    let income = |heavy: bool, of: Resource| {
        let node = match of {
            Resource::Grub => grub(35, 22, 7500),
            Resource::Marrow => marrow(35, 22, 3000),
        };
        let mut w = conquest(&[node], 100);
        // An idle worker out of the way, taking 70 supply (with the
        // gatherers' 3, past 60) or 1.
        spawn(&mut w, 0, if heavy { HEAVY } else { WORKER }, 10, 50);
        base_with_workers(&mut w, 3);
        income_per_minute(&mut w, of)
    };
    let (free, taxed) = (income(false, Resource::Grub), income(true, Resource::Grub));
    // Each load is cut to 7.5; a minute's count rounds down.
    assert!(
        taxed.abs_diff(free * 75 / 100) <= 1,
        "free {free}, taxed {taxed}"
    );
    assert_eq!(
        income(false, Resource::Marrow),
        income(true, Resource::Marrow)
    );
}

#[test]
fn a_worker_given_another_order_keeps_its_load() {
    let mut w = conquest(&[marrow(35, 22, 3000)], 100);
    let r = base_with_workers(&mut w, 1)[0];
    assert_eq!(w.gathers_at(r), Some(0));
    let mut ticks = 0;
    while w.carries(r).is_none() {
        w.tick(&[]);
        ticks += 1;
        assert!(ticks < MINUTE, "never filled a load");
    }
    assert_eq!(w.carries(r), Some((Resource::Marrow, 5)));
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: r,
            target: FxVec2::from_ints(50, 50),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    run(&mut w, 20 * TICKS_PER_SECOND);
    assert_eq!(w.gathers_at(r), None);
    assert_eq!(w.carries(r), Some((Resource::Marrow, 5)));
    assert_eq!(w.bank(0).unwrap().marrow, 100);
    // Sent back, it drops off what it carries first.
    assert!(gather(&mut w, 0, r, 0));
    let mut ticks = 0;
    while w.bank(0).unwrap().marrow == 100 {
        w.tick(&[]);
        ticks += 1;
        assert!(ticks < MINUTE, "never dropped off");
    }
    assert_eq!(w.bank(0).unwrap().marrow, 105);
    assert_eq!(w.economy().unwrap().nodes().next().unwrap().left, 3000 - 5);
}

#[test]
fn with_nowhere_to_take_it_a_worker_waits() {
    let mut w = conquest(&[marrow(35, 22, 3000)], 100);
    let r = spawn(&mut w, 0, WORKER, 30, 22);
    assert!(gather(&mut w, 0, r, 0));
    run(&mut w, MINUTE);
    assert_eq!(w.carries(r), Some((Resource::Marrow, 5)));
    assert_eq!(w.economy().unwrap().nodes().next().unwrap().left, 3000 - 5);
    put_up(&mut w, 0, HALL, 20, 20);
    run(&mut w, MINUTE);
    assert!(w.bank(0).unwrap().marrow > 100);
}

#[test]
fn only_a_players_own_workers_gather_at_a_node_that_exists() {
    let mut w = conquest(&[grub(35, 22, 7500)], 100);
    let worker = spawn(&mut w, 0, WORKER, 30, 22);
    let spears = spawn(&mut w, 0, SPEARS, 30, 40);
    assert!(!gather(&mut w, 1, worker, 0), "someone else's worker");
    assert!(!gather(&mut w, 0, spears, 0), "not a worker");
    assert!(!gather(&mut w, 0, worker, 1), "no such node");
    assert!(gather(&mut w, 0, worker, 0));
}

#[test]
fn gathering_is_refused_outside_conquest() {
    const TILES: u32 = 32;
    let side = TILES as usize * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 4,
        capacities: Capacities { regiments: 8 },
        units: &units(),
        map_tiles: TILES,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        generals: true,
    });
    let worker = spawn(&mut w, 0, WORKER, 10, 10);
    assert!(!gather(&mut w, 0, worker, 0));
    assert!(w.economy().is_none());
}

#[test]
fn the_economy_is_part_of_the_state_hash() {
    let play = |send: bool| {
        let mut w = conquest(&[grub(35, 22, 7500)], 100);
        put_up(&mut w, 0, HALL, 20, 20);
        let r = spawn(&mut w, 0, WORKER, 27, 20);
        if send {
            assert!(gather(&mut w, 0, r, 0));
        }
        run(&mut w, MINUTE);
        w.system_hashes()
    };
    let (sent, idle) = (play(true), play(false));
    let economy = gc_sim::SYSTEMS
        .iter()
        .position(|&s| s == "economy")
        .unwrap();
    assert_ne!(sent[economy], idle[economy]);
    assert_eq!(play(true), sent, "the same commands play out the same");
}

#[test]
fn a_seep_running_dry_gives_no_more_than_it_held_at_the_full_rate() {
    // Ten Grub: one load. The other four workers fill from dregs, four
    // times as slowly.
    let mut w = conquest(&[grub(32, 22, 10)], 100);
    base_with_workers(&mut w, 5);
    run(&mut w, 20 * TICKS_PER_SECOND);
    assert_eq!(w.bank(0).unwrap().grub, 500 + 10);
    assert_eq!(w.economy().unwrap().nodes().next().unwrap().left, 0);
}

#[test]
fn sent_again_to_its_seep_a_worker_keeps_filling() {
    let mut w = conquest(&[grub(32, 22, 7500)], 100);
    let r = base_with_workers(&mut w, 1)[0];
    let filling = |w: &World| w.economy().unwrap().nodes().next().unwrap().workers == 1;
    let mut ticks = 0;
    while !filling(&w) {
        w.tick(&[]);
        ticks += 1;
        assert!(ticks < MINUTE, "never began filling");
    }
    run(&mut w, 5 * TICKS_PER_SECOND);
    assert!(gather(&mut w, 0, r, 0));
    // Done in the 9 s it began with, not 9 s from the new order.
    run(&mut w, 5 * TICKS_PER_SECOND);
    assert_eq!(w.carries(r), Some((Resource::Grub, 10)));
}

#[test]
fn nothing_is_built_on_a_seep() {
    let mut w = conquest(&[grub(32, 22, 7500)], 100);
    assert_eq!(w.can_build(HOUSE, (31, 21), 0), Err(Refusal::Node));
    assert_eq!(
        w.can_build(HOUSE, (30, 22), 0),
        Err(Refusal::Node),
        "too near"
    );
    assert_eq!(w.can_build(HOUSE, (36, 22), 0), Ok(()));
    assert!(!order(
        &mut w,
        0,
        Command::SpawnStructure {
            building: HOUSE,
            tile: (31, 21),
            turn: 0,
        }
    ));
}

#[test]
fn a_worker_that_cannot_reach_its_node_gives_up() {
    // The vein stands in the middle of rough ground 8 tiles across.
    let rough = |x: i32, y: i32| (36..44).contains(&x) && (36..44).contains(&y);
    let mut w = conquest_on(rough, &[marrow(40, 40, 3000)], 100);
    let r = spawn(&mut w, 0, WORKER, 30, 30);
    assert!(gather(&mut w, 0, r, 0));
    let mut ticks = 0;
    while w.gathers_at(r).is_some() {
        w.tick(&[]);
        ticks += 1;
        assert!(ticks < 2 * MINUTE, "kept trying");
    }
    assert_eq!(w.carries(r), None);
}
