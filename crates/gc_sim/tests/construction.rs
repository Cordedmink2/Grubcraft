//! Conquest construction, training and tiers (docs/21-buildings.md):
//! workers lay and build buildings, buildings train regiments, and the main
//! building's upgrades raise its owner's tier.

use gc_fixed::FxVec2;
use gc_sim::Refusal;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, Role, StructureView};
use gc_sim::{
    Bank, BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup,
    PlayerCommand, RegimentHandle, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

const WORKER: u16 = 0;
const SPEARS: u16 = 1;
/// Tier 2, from the barracks.
const KNIGHTS: u16 = 2;

const HALL: u16 = 0;
const BARRACKS: u16 = 1;
/// The hall's tier 2 upgrade.
const KEEP: u16 = 2;
const HOUSE: u16 = 3;
/// Tier 2.
const TOWER: u16 = 4;

const SECOND: u32 = TICKS_PER_SECOND;

fn units() -> [UnitStats; 3] {
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    [worker, DRILL_UNITS[0], DRILL_UNITS[1]]
}

fn conquest_units() -> [ConquestUnit; 3] {
    [
        ConquestUnit {
            worker: true,
            supply: 1,
            cost_grub: 50,
            train_ticks: 240,
            trained_at: Some(HALL),
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
            supply: 3,
            cost_grub: 150,
            cost_marrow: 50,
            train_ticks: 600,
            trained_at: Some(BARRACKS),
            tier: 2,
            ..ConquestUnit::default()
        },
    ]
}

fn buildings() -> [BuildingStats; 5] {
    let hall = BuildingStats {
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
        supply: 10,
        flags: DROP_OFF_GRUB | DROP_OFF_MARROW,
        attack: None,
    };
    let small = |role, tier, cost_grub, build_ticks, hp, side, supply| BuildingStats {
        role,
        tier,
        cost_grub,
        build_ticks,
        hp,
        armour: Armour::Structure,
        width: side,
        depth: side,
        supply,
        flags: 0,
        ..hall
    };
    [
        hall,
        small(Role::Production, 1, 150, 600, 1500, 3, 0),
        BuildingStats {
            tier: 2,
            upgrades_from: Some(HALL),
            cost_grub: 300,
            cost_marrow: 100,
            build_ticks: 900,
            hp: 4000,
            supply: 15,
            ..hall
        },
        small(Role::Housing, 1, 60, 300, 600, 2, 10),
        small(Role::Defence, 2, 100, 400, 1000, 2, 0),
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

/// Ticks for `n` workers to build a hall at (20, 20), the first laying it
/// and the rest sent to help.
fn build_time(build_percent: u32, n: i32) -> u32 {
    let mut w = conquest(build_percent);
    let workers: Vec<_> = (0..n)
        .map(|i| spawn(&mut w, 0, WORKER, 30, 18 + i))
        .collect();
    let hall = lay(&mut w, workers[0], HALL, 20, 20).unwrap();
    for &r in &workers[1..] {
        assert!(order(
            &mut w,
            0,
            Command::Construct {
                regiment: r,
                structure: hall
            }
        ));
    }
    until(&mut w, 4000, |w| view(w, hall).finished())
}

#[test]
fn a_worker_lays_a_hall_pays_for_it_and_builds_it() {
    let mut w = conquest(100);
    let r = spawn(&mut w, 0, WORKER, 30, 22);
    let hall = lay(&mut w, r, HALL, 20, 20).unwrap();
    assert_eq!(bank(&w).grub, 1600);
    let v = view(&w, hall);
    // A foundation at a tenth of its hit points, that counts for nothing
    // yet.
    assert_eq!((v.built_percent, v.hp, v.finished()), (0, 300, false));
    assert_eq!(w.building_target(r), Some(hall));
    assert_eq!(
        (bank(&w).supply_cap, w.structures().unwrap().tier_of(0)),
        (0, 0)
    );
    // Half built, half of the rest of its hit points are up.
    until(&mut w, 2000, |w| view(w, hall).built_percent >= 50);
    let hp = view(&w, hall).hp;
    assert!((1600..=1700).contains(&hp), "{hp} hit points half built");
    let t = until(&mut w, 2000, |w| view(w, hall).finished());
    assert!((450..=520).contains(&t), "the second half took {t} ticks");
    assert_eq!(view(&w, hall).hp, 3000);
    assert_eq!(
        (bank(&w).supply_cap, w.structures().unwrap().tier_of(0)),
        (10, 1)
    );
    run(&mut w, 1);
    assert_eq!(w.building_target(r), None, "its work is done");
}

#[test]
fn more_builders_build_faster_up_to_four() {
    let one = build_time(100, 1);
    let two = build_time(100, 2);
    let four = build_time(100, 4);
    let six = build_time(100, 6);
    // The first builder does a tick's work a tick, each other half that,
    // after a short walk.
    assert!((1000..=1080).contains(&one), "one builder: {one}");
    assert!((667..=750).contains(&two), "two builders: {two}");
    assert!((400..=480).contains(&four), "four builders: {four}");
    assert!(six.abs_diff(four) <= 20, "six builders {six}, four {four}");
}

#[test]
fn tempo_scales_build_times() {
    let swift = build_time(75, 1);
    assert!(
        (750..=830).contains(&swift),
        "one builder at Swift: {swift}"
    );
}

#[test]
fn a_worker_mends_a_damaged_building() {
    let mut w = conquest(100);
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    let enemy = spawn(&mut w, 1, SPEARS, 22, 30);
    assert!(order(
        &mut w,
        1,
        Command::AttackStructure {
            regiment: enemy,
            structure: hall
        }
    ));
    until(&mut w, 600 * SECOND, |w| view(w, hall).hp <= 2700);
    assert!(order(
        &mut w,
        1,
        Command::RemoveRegiment { regiment: enemy }
    ));
    let before = view(&w, hall).hp;
    let r = spawn(&mut w, 0, WORKER, 30, 22);
    // Not the enemy's to mend.
    let theirs = put_up(&mut w, 1, HOUSE, 40, 40);
    assert!(!order(
        &mut w,
        0,
        Command::Construct {
            regiment: r,
            structure: theirs
        }
    ));
    assert!(order(
        &mut w,
        0,
        Command::Construct {
            regiment: r,
            structure: hall
        }
    ));
    run(&mut w, 20 * SECOND);
    // 8 hit points a second, after the walk.
    let mended = view(&w, hall).hp - before;
    assert!((8 * 16..=8 * 20).contains(&mended), "mended {mended}");
}

#[test]
fn calling_off_a_foundation_gives_back_three_quarters() {
    let mut w = conquest(100);
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    let r = spawn(&mut w, 0, WORKER, 36, 22);
    let barracks = lay(&mut w, r, BARRACKS, 30, 20).unwrap();
    assert_eq!(bank(&w).grub, 1850);
    run(&mut w, 5 * SECOND);
    // Only its owner can, and not once a building is up.
    assert!(!order(
        &mut w,
        1,
        Command::CallOff {
            structure: barracks
        }
    ));
    assert!(!order(&mut w, 0, Command::CallOff { structure: hall }));
    assert!(order(
        &mut w,
        0,
        Command::CallOff {
            structure: barracks
        }
    ));
    assert_eq!(bank(&w).grub, 1850 + 112);
    assert_eq!(w.structures().unwrap().len(), 1);
    run(&mut w, 1);
    assert_eq!(w.building_target(r), None);
    // Its ground is free again.
    assert_eq!(w.can_lay(0, BARRACKS, (30, 20)), Ok(()));
}

#[test]
fn what_may_be_laid_follows_tier_and_price() {
    let mut w = conquest(100);
    // A main building needs no tier; the rest need a finished one.
    assert_eq!(w.can_lay(0, HALL, (20, 20)), Ok(()));
    assert_eq!(w.can_lay(0, BARRACKS, (30, 20)), Err(Refusal::Tier));
    // Upgrades go up on the building they upgrade.
    assert_eq!(w.can_lay(0, KEEP, (20, 20)), Err(Refusal::Unknown));
    assert_eq!(w.can_lay(1, HALL, (20, 40)), Err(Refusal::Price));
    let theirs = spawn(&mut w, 1, WORKER, 30, 42);
    assert!(!order(
        &mut w,
        1,
        Command::Build {
            regiment: theirs,
            building: HALL,
            tile: (20, 40)
        }
    ));
    assert_eq!(w.bank(1).unwrap().grub, 100);
    // Nor someone else's worker.
    assert!(lay(&mut w, theirs, HALL, 20, 40).is_none());

    // A hall going up gives no tier.
    let r = spawn(&mut w, 0, WORKER, 30, 22);
    lay(&mut w, r, HALL, 20, 20).unwrap();
    assert_eq!(w.can_lay(0, BARRACKS, (30, 30)), Err(Refusal::Tier));
    put_up(&mut w, 0, HALL, 40, 20);
    assert_eq!(w.can_lay(0, BARRACKS, (30, 30)), Ok(()));
    assert_eq!(w.can_lay(0, TOWER, (30, 30)), Err(Refusal::Tier));
    // Placement still comes first.
    assert_eq!(w.can_lay(0, BARRACKS, (41, 21)), Err(Refusal::Built));
}

#[test]
fn a_hall_trains_a_worker_that_marches_to_its_rally_point() {
    let mut w = conquest(100);
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    let rally = FxVec2::from_ints(40, 40);
    assert!(order(
        &mut w,
        0,
        Command::Rally {
            structure: hall,
            at: rally
        }
    ));
    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: hall,
            unit: WORKER
        }
    ));
    let p = w.production().unwrap();
    assert_eq!(p.queue_of(hall), &[WORKER]);
    assert_eq!(p.rally_of(hall), Some(rally));
    assert_eq!(p.front_ticks_left(hall), Some(239));
    assert_eq!(w.training_percent(hall), Some(1));
    // Paid for now; its supply taken once it starts.
    let b = bank(&w);
    assert_eq!((b.grub, b.supply_used), (1950, 1));
    let t = until(&mut w, 1000, |w| count(w, WORKER) == 1);
    assert_eq!(t, 239);
    assert!(w.production().unwrap().queue_of(hall).is_empty());
    let new = w.regiments.iter().find(|r| r.owner == 0).unwrap();
    // Out of its south side.
    let door = FxVec2::new(gc_fixed::Fx::from_ratio(45, 2), gc_fixed::Fx::from_int(27));
    assert!((new.position - door).length() < gc_fixed::Fx::from_int(1));
    assert_eq!(bank(&w).supply_used, 1);
    run(&mut w, 20 * SECOND);
    let there = w.regiments.iter().find(|r| r.owner == 0).unwrap().position;
    assert!((there - rally).length() < gc_fixed::Fx::from_int(2));
}

#[test]
fn training_waits_for_supply() {
    let mut w = conquest(100);
    put_up(&mut w, 0, HALL, 20, 20);
    let barracks = put_up(&mut w, 0, BARRACKS, 30, 20);
    for i in 0..9 {
        spawn(&mut w, 0, WORKER, 40, 10 + 2 * i);
    }
    assert_eq!(bank(&w).supply_used, 9);
    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: barracks,
            unit: SPEARS
        }
    ));
    // 9 and 2 more is over the hall's 10: it waits.
    run(&mut w, 600);
    assert_eq!(count(&w, SPEARS), 0);
    assert_eq!(w.production().unwrap().front_ticks_left(barracks), None);
    assert_eq!(w.training_percent(barracks), Some(0));
    assert_eq!(bank(&w).supply_used, 9);
    put_up(&mut w, 0, HOUSE, 30, 30);
    assert_eq!(bank(&w).supply_used, 11);
    let t = until(&mut w, 1000, |w| count(w, SPEARS) == 1);
    assert_eq!(t, 399);
}

#[test]
fn a_queue_holds_five_and_the_last_comes_off_for_all_it_cost() {
    let mut w = conquest(100);
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    let barracks = put_up(&mut w, 0, BARRACKS, 30, 20);
    let train = |w: &mut World, at, unit| {
        order(
            w,
            0,
            Command::Train {
                structure: at,
                unit,
            },
        )
    };
    // Not what it trains, nor before its owner's tier allows.
    assert!(!train(&mut w, hall, SPEARS));
    assert!(!train(&mut w, barracks, KNIGHTS));
    assert!(!train(&mut w, barracks, 9));
    for _ in 0..5 {
        assert!(train(&mut w, barracks, SPEARS));
    }
    assert!(!train(&mut w, barracks, SPEARS), "the queue is full");
    let b = bank(&w);
    assert_eq!((b.grub, b.marrow), (1500, 400));
    assert!(order(
        &mut w,
        0,
        Command::Untrain {
            structure: barracks
        }
    ));
    let b = bank(&w);
    assert_eq!((b.grub, b.marrow), (1600, 420));
    assert_eq!(w.production().unwrap().queue_of(barracks).len(), 4);
    // Not someone else's.
    assert!(!order(
        &mut w,
        1,
        Command::Untrain {
            structure: barracks
        }
    ));
    for _ in 0..4 {
        assert!(order(
            &mut w,
            0,
            Command::Untrain {
                structure: barracks
            }
        ));
    }
    assert!(!order(
        &mut w,
        0,
        Command::Untrain {
            structure: barracks
        }
    ));
    let b = bank(&w);
    assert_eq!((b.grub, b.marrow, b.supply_used), (2000, 500, 0));
}

#[test]
fn a_building_that_falls_takes_its_queue_with_it() {
    let mut w = conquest(100);
    put_up(&mut w, 0, HALL, 20, 20);
    let barracks = put_up(&mut w, 0, BARRACKS, 30, 20);
    for _ in 0..2 {
        assert!(order(
            &mut w,
            0,
            Command::Train {
                structure: barracks,
                unit: SPEARS
            }
        ));
    }
    run(&mut w, 100);
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure {
            structure: barracks
        }
    ));
    // A new building in the same slot starts with an empty queue.
    let again = put_up(&mut w, 0, BARRACKS, 30, 20);
    assert_eq!(again.index, barracks.index);
    assert!(w.production().unwrap().queue_of(again).is_empty());
    run(&mut w, 1000);
    assert_eq!(count(&w, SPEARS), 0);
    let b = bank(&w);
    assert_eq!((b.grub, b.marrow, b.supply_used), (1800, 460, 0));
}

#[test]
fn a_hall_upgrades_to_a_keep_and_its_owner_to_tier_two() {
    let mut w = conquest(100);
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    let barracks = put_up(&mut w, 0, BARRACKS, 30, 20);
    // Only main buildings upgrade.
    assert!(!order(
        &mut w,
        0,
        Command::Upgrade {
            structure: barracks
        }
    ));
    assert!(order(&mut w, 0, Command::Upgrade { structure: hall }));
    assert!(
        !order(&mut w, 0, Command::Upgrade { structure: hall }),
        "already"
    );
    let b = bank(&w);
    assert_eq!((b.grub, b.marrow), (1700, 400));
    assert_eq!(view(&w, hall).upgrading, Some((KEEP, 0)));
    // Still a hall, and still training, while it upgrades.
    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: hall,
            unit: WORKER
        }
    ));
    run(&mut w, 450);
    let (_, done) = view(&w, hall).upgrading.unwrap();
    assert!((49..=51).contains(&done), "{done}% done");
    assert_eq!(w.structures().unwrap().tier_of(0), 1);
    let t = until(&mut w, 1000, |w| view(w, hall).building == KEEP);
    assert_eq!(t, 447);
    let v = view(&w, hall);
    assert_eq!((v.hp, v.max_hp, v.upgrading), (4000, 4000, None));
    assert_eq!(w.structures().unwrap().tier_of(0), 2);
    assert_eq!(bank(&w).supply_cap, 15);
    // What tier 2 opens, and a keep trains what a hall did.
    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: barracks,
            unit: KNIGHTS
        }
    ));
    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: hall,
            unit: WORKER
        }
    ));
    assert_eq!(w.can_lay(0, TOWER, (40, 40)), Ok(()));
    assert!(
        !order(&mut w, 0, Command::Upgrade { structure: hall }),
        "no tier 3"
    );
}

#[test]
fn calling_off_an_upgrade_gives_back_three_quarters() {
    let mut w = conquest(100);
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    assert!(order(&mut w, 0, Command::Upgrade { structure: hall }));
    run(&mut w, 100);
    assert!(order(&mut w, 0, Command::CallOff { structure: hall }));
    let b = bank(&w);
    assert_eq!((b.grub, b.marrow), (1700 + 225, 400 + 75));
    let v = view(&w, hall);
    assert_eq!((v.building, v.upgrading, v.finished()), (HALL, None, true));
    // The hall itself stays.
    assert!(!order(&mut w, 0, Command::CallOff { structure: hall }));
}

#[test]
fn tempo_scales_training_and_upgrades() {
    let mut w = conquest(75);
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: hall,
            unit: WORKER
        }
    ));
    assert!(order(&mut w, 0, Command::Upgrade { structure: hall }));
    // 240 × 75% and 900 × 75%, less the ticks they were ordered in.
    assert_eq!(until(&mut w, 1000, |w| count(w, WORKER) == 1), 178);
    assert_eq!(
        until(&mut w, 1000, |w| view(w, hall).building == KEEP),
        674 - 178
    );
}

#[test]
fn training_is_in_the_state_hash() {
    let production = gc_sim::SYSTEMS
        .iter()
        .position(|&s| s == "production")
        .unwrap();
    let mut a = conquest(100);
    let mut b = conquest(100);
    let hall = put_up(&mut a, 0, HALL, 20, 20);
    put_up(&mut b, 0, HALL, 20, 20);
    assert_eq!(a.state_hash(), b.state_hash());
    // A rally point alone moves nothing else.
    let rally = |w: &mut World, x| {
        order(
            w,
            0,
            Command::Rally {
                structure: hall,
                at: FxVec2::from_ints(x, 40),
            },
        )
    };
    assert!(rally(&mut a, 40));
    assert!(rally(&mut b, 41));
    let (ha, hb) = (a.system_hashes(), b.system_hashes());
    assert_ne!(ha[production], hb[production]);
    let others = |h: [u64; gc_sim::SYSTEMS.len()]| {
        let mut h = h;
        h[production] = 0;
        h
    };
    assert_eq!(others(ha), others(hb));
}

#[test]
fn one_finishing_makes_no_room_for_another_in_the_same_tick() {
    let mut w = conquest(100);
    put_up(&mut w, 0, HALL, 20, 20);
    let first = put_up(&mut w, 0, BARRACKS, 30, 20);
    let second = put_up(&mut w, 0, BARRACKS, 30, 30);
    for i in 0..8 {
        spawn(&mut w, 0, WORKER, 45, 10 + 2 * i);
    }
    let train = |w: &mut World, at| {
        order(
            w,
            0,
            Command::Train {
                structure: at,
                unit: SPEARS,
            },
        )
    };
    // 8 and 2 fill the hall's 10: the first starts, and the second waits
    // until the first's spearmen are out and counted.
    assert!(train(&mut w, first));
    assert!(train(&mut w, second));
    assert!(train(&mut w, second));
    for _ in 0..2000 {
        w.tick(&[]);
        let b = bank(&w);
        assert!(
            b.supply_used <= b.supply_cap,
            "{} of {}",
            b.supply_used,
            b.supply_cap
        );
    }
    assert_eq!(count(&w, SPEARS), 1);
}

#[test]
fn an_upgrade_of_another_footprint_is_no_upgrade() {
    let mut table = buildings();
    table[usize::from(KEEP)].width = 4;
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
    assert!(w.conquest(&ConquestSetup {
        buildings: &table,
        units: &conquest_units(),
        structures: 32,
        start,
        ..ConquestSetup::default()
    }));
    let hall = put_up(&mut w, 0, HALL, 20, 20);
    assert!(!order(&mut w, 0, Command::Upgrade { structure: hall }));
}

#[test]
fn a_building_walled_in_on_the_south_trains_out_of_another_side() {
    let mut w = conquest(100);
    put_up(&mut w, 0, HALL, 20, 20);
    let barracks = put_up(&mut w, 0, BARRACKS, 30, 20);
    put_up(&mut w, 0, HOUSE, 31, 24);
    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: barracks,
            unit: SPEARS
        }
    ));
    until(&mut w, 1000, |w| count(w, SPEARS) == 1);
    let out = w.regiments.iter().find(|r| r.unit == SPEARS).unwrap();
    let east = FxVec2::new(gc_fixed::Fx::from_int(35), gc_fixed::Fx::from_ratio(43, 2));
    assert!((out.position - east).length() < gc_fixed::Fx::from_int(1));
}
