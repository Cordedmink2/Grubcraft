//! The Brood's root-veins (docs/03-factions.md, docs/04-economy.md): a Hive
//! spreads them to the Pustules within reach, Gnawers burrow in at a seep
//! on them and send each load home through them, and living buildings on
//! them regrow.

use gc_fixed::{Fx, FxVec2};
use gc_sim::economy::{GRUB_LOAD, GRUB_SLOTS, GRUB_TICKS, NodeSetup, Resource, SEEP_GRUB};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, Role, StructureView, VEINS};
use gc_sim::veins::{REGROW_PER_SECOND, VEIN_FLOW_TICKS, VeinView};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MAX_PARTIES, MatchSetup,
    PlayerCommand, RegimentHandle, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

/// A worker that burrows in at its seep (the Gnawer).
const GNAWER: u16 = 0;
/// A worker that walks its loads to a drop-off, for contrast.
const PLAIN: u16 = 1;
/// A plain spearman, to damage buildings with.
const SPEARS: u16 = 2;

const HIVE: u16 = 0;
/// A small Living building with VEINS: spreads veins, and regrows.
const PUSTULE: u16 = 1;
/// A plain Structure-armour building, no VEINS.
const STRUCTURE: u16 = 2;
/// A plain Living building, no VEINS.
const SHRINE: u16 = 3;

/// How far the Hive's and a Pustule's root-veins reach, in tiles.
const MAIN_VEIN_TILES: i32 = 15;
const RELAY_VEIN_TILES: i32 = 8;

const SECOND: u32 = TICKS_PER_SECOND;
const MINUTE: u32 = 60 * SECOND;
/// Ticks a Gnawer's load takes from its seep to the store, at Standard
/// tempo, once it has started: the fill, the flow home, and the
/// bookkeeping of the tick it is banked and the tick it sets off again.
const LOAD_CYCLE: u32 = GRUB_TICKS + VEIN_FLOW_TICKS;

fn units() -> [UnitStats; 3] {
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    [worker, worker, DRILL_UNITS[0]]
}

fn conquest_units() -> [ConquestUnit; 3] {
    let worker = ConquestUnit {
        worker: true,
        supply: 1,
        ..ConquestUnit::default()
    };
    [
        ConquestUnit {
            burrows: true,
            ..worker
        },
        worker,
        ConquestUnit::default(),
    ]
}

fn building(role: Role, armour: Armour, side: u8, hp: u16, flags: u8) -> BuildingStats {
    BuildingStats {
        role,
        tier: 1,
        upgrades_from: None,
        cost_grub: 60,
        cost_marrow: 0,
        build_ticks: 300,
        hp,
        armour,
        width: side,
        depth: side,
        supply: 0,
        flags,
        attack: None,
        field: 0,
        tunes: None,
    }
}

fn buildings() -> [BuildingStats; 4] {
    [
        BuildingStats {
            supply: 40,
            cost_grub: 400,
            build_ticks: 1000,
            field: MAIN_VEIN_TILES as u8,
            ..building(Role::Main, Armour::Living, 5, 3000, VEINS | DROP_OFF_GRUB)
        },
        BuildingStats {
            field: RELAY_VEIN_TILES as u8,
            ..building(Role::Housing, Armour::Living, 2, 1500, VEINS)
        },
        building(Role::Housing, Armour::Structure, 3, 1500, 0),
        building(Role::Housing, Armour::Living, 2, 1500, 0),
    ]
}

fn seep(x: i32, y: i32) -> NodeSetup {
    NodeSetup {
        resource: Resource::Grub,
        at: FxVec2::from_ints(x, y),
        amount: SEEP_GRUB,
    }
}

/// An open map 64 tiles across, in Conquest at Standard tempo, with the
/// given seeps. Both players start with 100 Grub.
fn conquest(nodes: &[NodeSetup]) -> World {
    let side = 64 * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 64 },
        units: &units(),
        map_tiles: 64,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    });
    let mut start = [(0, 0); MAX_PARTIES];
    start[0] = (100, 0);
    start[1] = (100, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &conquest_units(),
        structures: 32,
        nodes,
        start,
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

/// Player `player` puts up a finished building with its north-west tile at
/// `(x, y)`.
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

fn gather(w: &mut World, r: RegimentHandle, node: u16) -> bool {
    order(w, 0, Command::Gather { regiment: r, node })
}

fn view(w: &World, h: StructureHandle) -> StructureView {
    w.structures().unwrap().get(h).unwrap()
}

/// The veins view of building `h`, if its veins are linked.
fn vein_of(w: &World, h: StructureHandle) -> Option<VeinView> {
    w.veins().find(|v| v.building == h)
}

fn grub(w: &World) -> u32 {
    w.bank(0).unwrap().grub
}

/// Workers filling a load at node `node`, as of the last tick.
fn node_workers(w: &World, node: u16) -> u8 {
    w.economy()
        .unwrap()
        .nodes()
        .find(|n| n.index == node)
        .map_or(0, |n| n.workers)
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

/// A main building's veins reach 15 tiles from its edge, a pustule's 8. A
/// Hive put up whole is linked and is its own link; a pustule 15 tiles off
/// its edge links through the Hive, one 16 tiles off does not, and a
/// pustule 8 tiles beyond a linked pustule links through that one.
#[test]
fn a_hive_roots_veins_that_reach_pustules_within_reach_and_chain_on() {
    let mut w = conquest(&[]);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    let hive_at = view(&w, hive).centre();
    let main = vein_of(&w, hive).expect("a Hive's veins are linked");
    assert_eq!(main.owner, 0);
    assert_eq!(
        (main.from, main.at),
        (hive_at, hive_at),
        "a main building links through itself"
    );
    assert_eq!(main.reach, MAIN_VEIN_TILES);

    let near = put_up(&mut w, 0, PUSTULE, 40, 20);
    let near_at = view(&w, near).centre();
    let v = vein_of(&w, near).expect("15 tiles off the Hive's edge is in reach");
    assert_eq!((v.from, v.reach), (hive_at, RELAY_VEIN_TILES));

    let far = put_up(&mut w, 0, PUSTULE, 20, 41);
    run(&mut w, 2);
    assert!(
        vein_of(&w, far).is_none(),
        "16 tiles off the Hive's edge is out of reach"
    );

    let chained = put_up(&mut w, 0, PUSTULE, 50, 20);
    let v = vein_of(&w, chained).expect("8 tiles off a linked pustule is in reach");
    assert_eq!(v.from, near_at, "it links through the pustule it is near");
    assert_eq!(v.at, view(&w, chained).centre());
    assert_eq!(
        w.veins().count(),
        3,
        "the Hive, its pustule and the chained one"
    );
}

/// Removing the pustule in the middle of a chain cuts off the one beyond it
/// in the same tick, and it stays cut off.
#[test]
fn removing_the_middle_pustule_cuts_off_the_one_beyond() {
    let mut w = conquest(&[]);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    let middle = put_up(&mut w, 0, PUSTULE, 40, 20);
    let beyond = put_up(&mut w, 0, PUSTULE, 50, 20);
    assert!(vein_of(&w, beyond).is_some());

    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: middle }
    ));
    assert!(
        vein_of(&w, beyond).is_none(),
        "unlinked from the tick the middle one goes"
    );
    assert!(vein_of(&w, middle).is_none());
    assert!(vein_of(&w, hive).is_some(), "the Hive is still linked");
    assert_eq!(w.veins().count(), 1);

    run(&mut w, 40);
    assert!(vein_of(&w, beyond).is_none(), "and stays unlinked");
}

/// `on_veins` holds for a point within reach of its owner's linked veins
/// and not beyond, reaches out along a chain, and is not true for another
/// player's veins.
#[test]
fn on_veins_is_true_within_reach_of_the_owners_veins_only() {
    let mut w = conquest(&[]);
    put_up(&mut w, 0, HIVE, 20, 20);
    put_up(&mut w, 0, PUSTULE, 40, 20);
    put_up(&mut w, 1, HIVE, 50, 50);
    let p = FxVec2::from_ints;

    // Within the Hive's 15 tiles, and beyond them.
    assert!(w.on_veins(0, p(14, 22)), "6 tiles off the Hive's edge");
    assert!(!w.on_veins(0, p(2, 22)), "18 tiles off the Hive's edge");
    // Out along the pustule's 8 tiles, and beyond them.
    assert!(w.on_veins(0, p(48, 21)), "6 tiles off the pustule's edge");
    assert!(!w.on_veins(0, p(52, 21)), "10 tiles off the pustule's edge");

    // Another player's veins do not count for player 0, or the reverse.
    assert!(w.on_veins(1, p(46, 52)), "4 tiles off player 1's Hive");
    assert!(
        !w.on_veins(0, p(46, 52)),
        "player 0's veins don't reach player 1's Hive"
    );
    assert!(
        !w.on_veins(1, p(14, 22)),
        "player 1 has no veins by player 0's Hive"
    );
}

/// Five Gnawers on a seep on the veins bring in about 200 Grub a minute
/// (their loads, 10 Grub each, at one every 300 ticks plus the flow home),
/// with supply under 60 so upkeep keeps all of it.
#[test]
fn five_gnawers_on_a_seep_on_the_veins_bring_in_about_two_hundred_grub_a_minute() {
    let mut w = conquest(&[seep(38, 22)]);
    put_up(&mut w, 0, HIVE, 20, 20);
    assert!(w.on_veins(0, FxVec2::from_ints(38, 22)));
    for y in [18, 22, 26, 30, 34] {
        let g = spawn(&mut w, 0, GNAWER, 30, y);
        assert!(gather(&mut w, g, 0));
    }
    until(&mut w, 60 * SECOND, |w| node_workers(w, 0) == 5);
    // Each has come in once by then, so measure two minutes of steady work.
    run(&mut w, 2 * LOAD_CYCLE);
    let start = grub(&w);
    run(&mut w, 2 * MINUTE);
    let gained = grub(&w) - start;
    let per_minute = gained * MINUTE / (2 * MINUTE);
    let expected = GRUB_LOAD * u32::from(GRUB_SLOTS) * MINUTE / LOAD_CYCLE;
    assert!(
        w.bank(0).unwrap().supply_used < 60,
        "supply must stay under 60 for full upkeep"
    );
    assert!(
        (expected * 9 / 10..=expected * 11 / 10).contains(&per_minute),
        "{gained} Grub in two minutes: {per_minute} a minute, expected about {expected}"
    );
}

/// A Gnawer stays at its seep, within a few tiles of it, from the moment it
/// arrives, while a plain worker sent to a seep off the veins walks its
/// loads to the Hive. (The contrast shows the walk would have been seen.)
#[test]
fn a_gnawer_stays_at_its_seep_and_never_walks_to_the_hive() {
    let mut w = conquest(&[seep(38, 22), seep(30, 44)]);
    let hive = put_up(&mut w, 0, HIVE, 20, 20);
    let hive_at = view(&w, hive).centre();
    let gnawer = spawn(&mut w, 0, GNAWER, 30, 22);
    let plain = spawn(&mut w, 0, PLAIN, 28, 44);
    assert!(gather(&mut w, gnawer, 0));
    assert!(gather(&mut w, plain, 1));
    let seep_at = FxVec2::from_ints(38, 22);
    until(&mut w, 60 * SECOND, |w| node_workers(w, 0) == 1);

    let mut plain_nearest_hive = Fx::MAX;
    for _ in 0..3 * MINUTE {
        w.tick(&[]);
        let at = w.regiments.position(gnawer).expect("the Gnawer is alive");
        assert!(
            (at - seep_at).length() <= Fx::from_int(3),
            "the Gnawer left its seep: {at:?}"
        );
        if let Some(p) = w.regiments.position(plain) {
            plain_nearest_hive = plain_nearest_hive.min((p - hive_at).length());
        }
    }
    assert!(
        plain_nearest_hive < Fx::from_int(7),
        "the plain worker never walked to the Hive: {plain_nearest_hive:?}"
    );
}

/// A Gnawer on a seep off the veins brings in nothing; once a pustule goes
/// up that reaches the seep, its loads start coming in.
#[test]
fn a_gnawer_on_a_seep_off_the_veins_brings_nothing_until_a_pustule_links_it() {
    let mut w = conquest(&[seep(50, 22)]);
    put_up(&mut w, 0, HIVE, 20, 20);
    let seep_at = FxVec2::from_ints(50, 22);
    assert!(!w.on_veins(0, seep_at), "25 tiles off the Hive's edge");

    let gnawer = spawn(&mut w, 0, GNAWER, 46, 22);
    assert!(gather(&mut w, gnawer, 0));
    until(&mut w, 60 * SECOND, |w| node_workers(w, 0) == 1);
    let before = grub(&w);
    // Long enough for its first load to fill, and several more to try.
    run(&mut w, MINUTE);
    assert_eq!(grub(&w), before, "nothing comes in off the veins");

    // A pustule 8 tiles off the seep: it links the seep through the Hive.
    put_up(&mut w, 0, PUSTULE, 40, 20);
    assert!(w.on_veins(0, seep_at));
    until(&mut w, 2 * SECOND, |w| grub(w) > before);
    // The first load lands as it links; within 900 ticks, two more do.
    let after_first = grub(&w);
    run(&mut w, 3 * LOAD_CYCLE);
    assert!(
        grub(&w) >= after_first + 2 * GRUB_LOAD,
        "its loads keep coming in once it is linked: {} then {}",
        after_first,
        grub(&w)
    );
}

/// A damaged Living building on its owner's veins regrows about two hit
/// points a second; one off the veins does not, and neither does a
/// Structure-armour building on the veins.
#[test]
fn a_damaged_living_building_on_the_veins_regrows_and_the_rest_do_not() {
    let mut w = conquest(&[]);
    put_up(&mut w, 0, HIVE, 20, 20);
    let pustule = put_up(&mut w, 0, PUSTULE, 40, 20);
    let shrine_on = put_up(&mut w, 0, SHRINE, 20, 38);
    let wall = put_up(&mut w, 0, STRUCTURE, 30, 27);
    let shrine_off = put_up(&mut w, 0, SHRINE, 5, 50);
    assert!(w.on_veins(0, view(&w, shrine_on).centre()));
    assert!(w.on_veins(0, view(&w, wall).centre()));
    assert!(!w.on_veins(0, view(&w, shrine_off).centre()));

    // An enemy regiment knocks each one down a little, from a few tiles off.
    let attack = [
        (pustule, 41, 34),
        (shrine_on, 10, 39),
        (wall, 45, 28),
        (shrine_off, 14, 52),
    ];
    let mut attackers = Vec::new();
    for &(target, x, y) in &attack {
        let a = spawn(&mut w, 1, SPEARS, x, y);
        assert!(order(
            &mut w,
            1,
            Command::AttackStructure {
                regiment: a,
                structure: target
            }
        ));
        attackers.push(a);
    }
    let hurt = |w: &World, h: StructureHandle| {
        let v = view(w, h);
        v.max_hp - v.hp >= 100
    };
    until(&mut w, 90 * SECOND, |w| {
        attack.iter().all(|&(h, _, _)| hurt(w, h))
    });
    for a in attackers {
        assert!(order(&mut w, 1, Command::RemoveRegiment { regiment: a }));
    }

    let targets = [pustule, shrine_on, wall, shrine_off];
    let before: Vec<u32> = targets.iter().map(|&h| view(&w, h).hp).collect();
    run(&mut w, 10 * SECOND);
    let gained: Vec<u32> = targets
        .iter()
        .zip(&before)
        .map(|(&h, &b)| view(&w, h).hp - b)
        .collect();

    let regrows = REGROW_PER_SECOND * 10;
    assert_eq!(gained[0], regrows, "a pustule on the veins regrows");
    assert_eq!(gained[1], regrows, "a Living shrine on the veins regrows");
    assert_eq!(gained[2], 0, "a Structure-armour wall does not regrow");
    assert_eq!(
        gained[3], 0,
        "a Living shrine off the veins does not regrow"
    );
}

/// Two worlds given the same orders stay alike tick by tick: Gnawers
/// working on seeps on the veins and off them, a plain worker, and a
/// pustule taken out while they work.
#[test]
fn two_worlds_given_the_same_orders_play_alike_with_gnawers_and_a_removal() {
    let play = || {
        let mut w = conquest(&[seep(38, 22), seep(54, 22), seep(30, 44)]);
        put_up(&mut w, 0, HIVE, 20, 20);
        let middle = put_up(&mut w, 0, PUSTULE, 40, 20);
        put_up(&mut w, 0, PUSTULE, 50, 20);
        let mut start = Vec::new();
        for y in [26, 30, 34] {
            let g = spawn(&mut w, 0, GNAWER, 30, y);
            start.push(PlayerCommand {
                player: 0,
                command: Command::Gather {
                    regiment: g,
                    node: 0,
                },
            });
        }
        for (x, y) in [(46, 30), (46, 34)] {
            let g = spawn(&mut w, 0, GNAWER, x, y);
            start.push(PlayerCommand {
                player: 0,
                command: Command::Gather {
                    regiment: g,
                    node: 1,
                },
            });
        }
        let plain = spawn(&mut w, 0, PLAIN, 28, 44);
        start.push(PlayerCommand {
            player: 0,
            command: Command::Gather {
                regiment: plain,
                node: 2,
            },
        });
        let remove = [PlayerCommand {
            player: 0,
            command: Command::RemoveStructure { structure: middle },
        }];
        let mut hashes = Vec::new();
        for t in 0..600u32 {
            let orders: &[PlayerCommand] = match t {
                0 => &start,
                300 => &remove,
                _ => &[],
            };
            w.tick(orders);
            hashes.push(w.state_hash());
        }
        assert_eq!(w.veins().count(), 1, "only the Hive is linked at the end");
        (hashes, grub(&w))
    };
    let (first, grub_first) = play();
    let (second, grub_second) = play();
    assert!(grub_first > 0, "the workers brought something in");
    assert_eq!(grub_first, grub_second);
    assert_eq!(first, second);
}

/// A Gnawer holding a load from a seep off the veins, sent to a seep on
/// them, takes the load there before it comes in: it can't bank it from
/// where it stands.
#[test]
fn a_stranded_load_comes_in_only_once_its_gnawer_reaches_a_linked_seep() {
    let mut w = conquest(&[seep(50, 22), seep(30, 12)]);
    put_up(&mut w, 0, HIVE, 20, 20);
    assert!(!w.on_veins(0, FxVec2::from_ints(50, 22)));
    assert!(w.on_veins(0, FxVec2::from_ints(30, 12)));
    let gnawer = spawn(&mut w, 0, GNAWER, 46, 22);
    assert!(gather(&mut w, gnawer, 0));
    // Its first load fills and waits at the cut-off seep.
    run(&mut w, 2 * LOAD_CYCLE);
    let before = grub(&w);
    assert!(gather(&mut w, gnawer, 1));
    assert_eq!(grub(&w), before, "not banked from where it stands");
    until(&mut w, MINUTE, |w| grub(w) > before);
    let here = w.regiments.get(gnawer).unwrap().position;
    assert!(
        (here - FxVec2::from_ints(30, 12)).length() <= gc_fixed::Fx::from_int(3),
        "banked once at the linked seep"
    );
}
