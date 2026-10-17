//! The Brood's larvae (docs/04-economy.md, docs/03-factions.md): a finished
//! Hive holds up to three larvae, a unit that hatches takes one to queue,
//! units that hatch train all at once, and a consumed worker (the Gnawer)
//! starts a building that then grows on its own.

use gc_fixed::FxVec2;
use gc_sim::events::Event;
use gc_sim::larvae::{LARVA_CAP, LARVA_TICKS};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, LARVAE, Role, StructureView};
use gc_sim::{
    Bank, BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup,
    PlayerCommand, Refusal, RegimentHandle, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

/// The Gnawer: a worker used up by the building it starts.
const WORKER: u16 = 0;
/// A hatching fighter, trained at the Hive.
const FIGHTER: u16 = 1;
/// A plain spearman, trained at the barracks.
const SPEARS: u16 = 2;
/// A worker that is not used up: it builds like any worker, and hatches.
const DRONE: u16 = 3;

const HIVE: u16 = 0;
const BARRACKS: u16 = 1;
/// A small building a Gnawer starts.
const NEST: u16 = 2;
/// A small building that holds larvae.
const PIT: u16 = 3;

const SECOND: u32 = TICKS_PER_SECOND;
/// Ticks a fighter or spearman takes to train at Standard tempo.
const FIGHTER_TICKS: u32 = 400;

fn units() -> [UnitStats; 4] {
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    [worker, DRILL_UNITS[1], DRILL_UNITS[1], worker]
}

fn conquest_units() -> [ConquestUnit; 4] {
    [
        ConquestUnit {
            worker: true,
            hatches: true,
            consumed: true,
            supply: 1,
            cost_grub: 50,
            train_ticks: 240,
            trained_at: Some(HIVE),
            tier: 1,
            ..ConquestUnit::default()
        },
        ConquestUnit {
            hatches: true,
            supply: 2,
            cost_grub: 100,
            cost_marrow: 20,
            train_ticks: 400,
            trained_at: Some(HIVE),
            tier: 1,
            ..ConquestUnit::default()
        },
        ConquestUnit {
            supply: 2,
            cost_grub: 100,
            cost_marrow: 20,
            train_ticks: 400,
            trained_at: Some(BARRACKS),
            tier: 1,
            ..ConquestUnit::default()
        },
        ConquestUnit {
            worker: true,
            hatches: true,
            supply: 1,
            cost_grub: 50,
            train_ticks: 240,
            trained_at: Some(HIVE),
            tier: 1,
            ..ConquestUnit::default()
        },
    ]
}

fn buildings() -> [BuildingStats; 4] {
    let hive = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 400,
        cost_marrow: 0,
        build_ticks: 1000,
        hp: 3000,
        armour: Armour::Fortified,
        width: 5,
        depth: 5,
        supply: 40,
        flags: DROP_OFF_GRUB | DROP_OFF_MARROW | LARVAE,
        attack: None,
        field: 0,
        tunes: None,
    };
    let small = |role: Role, cost_grub: u16, build_ticks: u16, side: u8, flags: u8| BuildingStats {
        role,
        tier: 1,
        cost_grub,
        build_ticks,
        hp: 1500,
        armour: Armour::Structure,
        width: side,
        depth: side,
        supply: 0,
        flags,
        ..hive
    };
    [
        hive,
        small(Role::Production, 150, 600, 3, 0),
        small(Role::Production, 60, 400, 2, 0),
        small(Role::Production, 80, 300, 2, LARVAE),
    ]
}

/// An open map 64 tiles across, in Conquest at `build_percent` tempo.
/// Player 0 starts with 2000 Grub and 500 Marrow, player 1 with 100 Grub.
fn conquest(build_percent: u32) -> World {
    let side = 64 * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 6,
        capacities: Capacities { regiments: 64 },
        units: &units(),
        map_tiles: 64,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    });
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (2000, 500);
    start[1] = (100, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &conquest_units(),
        structures: 32,
        start,
        build_percent,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn at(w: &World, x: i32, y: i32) -> Option<StructureHandle> {
    w.structures()?
        .iter()
        .find(|v| v.tile == (x, y))
        .map(|v| v.handle)
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile: (x, y)
        }
    ));
    at(w, i32::from(x), i32::from(y)).unwrap()
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

/// Worker `r` lays a building of type `building` at `(x, y)`.
fn lay(w: &mut World, r: RegimentHandle, building: u16, x: i16, y: i16) -> Option<StructureHandle> {
    order(
        w,
        0,
        Command::Build {
            regiment: r,
            building,
            tile: (x, y),
        },
    )
    .then(|| at(w, i32::from(x), i32::from(y)).unwrap())
}

fn view(w: &World, h: StructureHandle) -> StructureView {
    w.structures().unwrap().get(h).unwrap()
}

fn bank(w: &World) -> Bank {
    w.bank(0).unwrap()
}

fn train(w: &mut World, structure: StructureHandle, unit: u16) -> bool {
    order(w, 0, Command::Train { structure, unit })
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Ticks until `done`, up to `most`.
fn until(w: &mut World, most: u32, done: impl Fn(&World) -> bool) -> u32 {
    for t in 1..=most {
        w.tick(&[]);
        if done(w) {
            return t;
        }
    }
    panic!("not done in {most} ticks");
}

/// Player 0's regiments of type `unit`.
fn count(w: &World, unit: u16) -> usize {
    w.regiments
        .iter()
        .filter(|r| r.owner == 0 && r.unit == unit)
        .count()
}

/// A Hive put up whole holds its full three larvae, and the bank shows
/// them as ready and as room for three.
#[test]
fn a_hive_put_up_whole_holds_three_larvae() {
    let mut w = conquest(100);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    let b = bank(&w);
    assert_eq!((b.larvae, b.larva_room), (3, u32::from(LARVA_CAP)));
    assert_eq!(w.larvae_in(hive), Some(3));
}

/// Queueing a unit that hatches takes a larva from the building it is queued
/// at. With none left, the next such unit is refused for want of a larva
/// (Refusal::Larva), not taken, and the queue stays as it was.
#[test]
fn a_hatching_unit_takes_a_larva_and_none_left_refuses() {
    let mut w = conquest(100);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    for queued in 1..=3u8 {
        assert!(train(&mut w, hive, FIGHTER));
        assert_eq!(w.larvae_in(hive), Some(3 - queued));
    }
    assert_eq!(bank(&w).larvae, 0);
    assert_eq!(w.can_train(0, hive, FIGHTER), Err(Refusal::Larva));
    let grub = bank(&w).grub;
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Train {
            structure: hive,
            unit: FIGHTER,
        },
    }]);
    assert_eq!(stats.commands_rejected, 1);
    assert_eq!(bank(&w).grub, grub, "a refused unit costs nothing");
    assert_eq!(w.production().unwrap().queue_of(hive).len(), 3);
}

/// A larva grows back LARVA_TICKS after it was taken, counting the tick it
/// was taken on as the first of those ticks (the larva starts growing in
/// the tick it is queued).
fn regrows_in(build_percent: u32, ticks: u32) {
    let mut w = conquest(build_percent);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    // The train order's tick is the first of the growth.
    assert!(train(&mut w, hive, FIGHTER));
    assert_eq!(w.larvae_in(hive), Some(2));
    run(&mut w, ticks - 2);
    assert_eq!(w.larvae_in(hive), Some(2), "still growing");
    run(&mut w, 1);
    assert_eq!(w.larvae_in(hive), Some(3), "grown back");
    assert_eq!(bank(&w).larvae, 3);
}

/// At Standard tempo a taken larva grows back in LARVA_TICKS (12 seconds).
#[test]
fn a_taken_larva_grows_back_after_twelve_seconds() {
    assert_eq!(LARVA_TICKS, 12 * SECOND);
    regrows_in(100, LARVA_TICKS);
}

/// At build_percent 150 a larva takes 150% of its Standard time: 18 seconds.
#[test]
fn at_tempo_150_a_larva_takes_eighteen_seconds() {
    regrows_in(150, 18 * SECOND);
}

/// Three fighters queued in one tick all start at once (supply allowing),
/// so they all come out together one train time later, not one after another.
#[test]
fn hatching_units_come_out_together() {
    let mut w = conquest(100);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    let train_one = PlayerCommand {
        player: 0,
        command: Command::Train {
            structure: hive,
            unit: FIGHTER,
        },
    };
    let t0 = w.tick_count();
    assert_eq!(w.tick(&[train_one; 3]).commands_applied, 3);
    let t = until(&mut w, 2 * FIGHTER_TICKS, |w| count(w, FIGHTER) > 0);
    assert_eq!(count(&w, FIGHTER), 3, "all three on the field together");
    assert!(
        (FIGHTER_TICKS..=FIGHTER_TICKS + 2).contains(&(w.tick_count() - t0)),
        "out after {t} more ticks"
    );
    assert!(w.production().unwrap().queue_of(hive).is_empty());
}

/// A barracks trains its spearmen one at a time: three queued in one tick,
/// only one is out after a train time, and the next only once it is done.
#[test]
fn units_that_do_not_hatch_come_out_one_at_a_time() {
    let mut w = conquest(100);
    put_up(&mut w, 0, HIVE, 20, 20);
    let barracks = put_up(&mut w, 0, BARRACKS, 30, 20);
    let train_one = PlayerCommand {
        player: 0,
        command: Command::Train {
            structure: barracks,
            unit: SPEARS,
        },
    };
    let t0 = w.tick_count();
    assert_eq!(w.tick(&[train_one; 3]).commands_applied, 3);
    until(&mut w, 2 * FIGHTER_TICKS, |w| count(w, SPEARS) > 0);
    assert_eq!(count(&w, SPEARS), 1);
    assert!(
        (FIGHTER_TICKS..=FIGHTER_TICKS + 2).contains(&(w.tick_count() - t0)),
        "the first out after one train time"
    );
    run(&mut w, 2);
    assert_eq!(count(&w, SPEARS), 1, "the second has not come out");
    until(&mut w, 2 * FIGHTER_TICKS, |w| count(w, SPEARS) == 2);
    let second = w.tick_count() - t0;
    assert!(
        (2 * FIGHTER_TICKS - 2..=2 * FIGHTER_TICKS + 2).contains(&second),
        "the second out after {second} ticks"
    );
}

/// Calling off a hatching unit still queued gives its larva back to the
/// building, and its price with it.
#[test]
fn untraining_a_hatching_unit_gives_its_larva_back() {
    let mut w = conquest(100);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    let grub = bank(&w).grub;
    assert!(train(&mut w, hive, FIGHTER));
    assert_eq!(bank(&w).larvae, 2);
    assert!(order(&mut w, 0, Command::Untrain { structure: hive }));
    assert_eq!(bank(&w).larvae, 3);
    assert_eq!(w.larvae_in(hive), Some(3));
    assert_eq!(bank(&w).grub, grub);
}

/// A Gnawer is used up on reaching the foundation it was sent to: the tick
/// says so with an `Event::Consumed` and no loss, the building then grows on
/// its own in about its build time, and a second Gnawer can't help it grow.
#[test]
fn a_consumed_worker_starts_a_building_that_then_grows_on_its_own() {
    let mut w = conquest(100);
    put_up(&mut w, 0, HIVE, 20, 20);
    let gnawer = spawn(&mut w, 0, WORKER, 36, 31);
    let second = spawn(&mut w, 0, WORKER, 50, 40);
    let nest = lay(&mut w, gnawer, NEST, 40, 30).unwrap();

    let mut used = false;
    for _ in 0..600 {
        w.tick(&[]);
        if w.regiments.get(gnawer).is_none() {
            used = true;
            break;
        }
    }
    assert!(used, "the gnawer reached its building");
    let events = w.events();
    assert!(
        events.contains(&Event::Consumed {
            worker: gnawer,
            structure: nest
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::Losses { regiment, .. } if *regiment == gnawer)),
        "a used-up worker is not a loss: {events:?}"
    );
    assert!(!view(&w, nest).finished());

    // Used up to start it, not to help it: the second Gnawer is turned away.
    assert!(!order(
        &mut w,
        0,
        Command::Construct {
            regiment: second,
            structure: nest
        }
    ));
    assert!(w.regiments.get(second).is_some());

    // Grown in about its build time: the nest takes 400 ticks at Standard
    // tempo, and the tick it was used up on is not counted.
    let grown = until(&mut w, 600, |w| view(w, nest).finished());
    assert!((380..=420).contains(&grown), "grew in {grown} ticks");

    // Finished, it is no more for a Gnawer to help.
    assert!(!order(
        &mut w,
        0,
        Command::Construct {
            regiment: second,
            structure: nest
        }
    ));
}

/// A larvae building laid as a foundation and built by a worker starts with
/// no larvae once finished, and grows its first only LARVA_TICKS later.
#[test]
fn a_larvae_building_built_from_a_foundation_starts_empty() {
    let mut w = conquest(100);
    put_up(&mut w, 0, HIVE, 20, 20);
    let drone = spawn(&mut w, 0, DRONE, 26, 33);
    let pit = lay(&mut w, drone, PIT, 30, 30).unwrap();
    assert_eq!(w.larvae_in(pit), Some(0), "a foundation holds none");
    until(&mut w, 2000, |w| view(w, pit).finished());
    assert_eq!(w.larvae_in(pit), Some(0), "finished, and empty");
    assert_eq!(bank(&w).larvae, 3, "only the Hive's");
    assert_eq!(bank(&w).larva_room, 2 * u32::from(LARVA_CAP));
    // The tick it finished is the first of its growth.
    run(&mut w, LARVA_TICKS - 2);
    assert_eq!(w.larvae_in(pit), Some(0), "still growing");
    run(&mut w, 1);
    assert_eq!(w.larvae_in(pit), Some(1), "its first larva");
}

/// With the Hive empty a fourth fighter takes its larva from the Pit, and
/// taking it back off the queue gives the larva to the fullest building
/// with room (the Pit), so it can't set the empty Hive growing an extra one.
#[test]
fn an_untrained_larva_goes_back_to_the_fullest_building() {
    let mut w = conquest(100);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    let pit = put_up(&mut w, 0, PIT, 30, 20);
    let train_one = PlayerCommand {
        player: 0,
        command: Command::Train {
            structure: hive,
            unit: FIGHTER,
        },
    };
    assert_eq!(w.tick(&[train_one; 4]).commands_applied, 4);
    assert_eq!((w.larvae_in(hive), w.larvae_in(pit)), (Some(0), Some(2)));
    assert!(order(&mut w, 0, Command::Untrain { structure: hive }));
    assert_eq!((w.larvae_in(hive), w.larvae_in(pit)), (Some(0), Some(3)));
}

/// Two worlds given the same orders stay alike tick by tick through
/// larvae taken, regrown and given back, hatching, and a Gnawer used up.
#[test]
fn larvae_play_alike_twice() {
    let play = || {
        let mut w = conquest(100);
        let hive = put_up(&mut w, 0, HIVE, 20, 20);
        let gnawer = spawn(&mut w, 0, WORKER, 36, 31);
        lay(&mut w, gnawer, NEST, 40, 30).unwrap();
        let mut hashes = Vec::new();
        for t in 0..900u32 {
            let orders: &[PlayerCommand] = match t {
                0 | 1 | 300 => &[PlayerCommand {
                    player: 0,
                    command: Command::Train {
                        structure: hive,
                        unit: FIGHTER,
                    },
                }],
                2 => &[PlayerCommand {
                    player: 0,
                    command: Command::Untrain { structure: hive },
                }],
                _ => &[],
            };
            w.tick(orders);
            hashes.push(w.state_hash());
        }
        assert!(count(&w, FIGHTER) >= 1);
        hashes
    };
    assert_eq!(play(), play());
}
