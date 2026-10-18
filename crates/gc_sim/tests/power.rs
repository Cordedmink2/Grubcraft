//! The Choir's power and tuning (docs/03-factions.md, docs/04-economy.md):
//! a Spire's field powers the buildings within its reach, a building that
//! needs power trains nothing while it has none, and a Resonator or Siphon
//! works the seeps and veins within its reach with no workers at all.

use gc_fixed::FxVec2;
use gc_sim::economy::{NodeSetup, Resource, SEEP_GRUB, VEIN_MARROW};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{
    Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, NEEDS_POWER, POWERS_FIELD, Refusal, Role,
};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MAX_PARTIES, MatchSetup,
    PlayerCommand, RegimentHandle, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

/// A plain worker, that walks its loads to a drop-off.
const WORKER: u16 = 0;
/// A worker that builds and mends but does not gather (a Choir Tuner).
const TUNER: u16 = 1;
/// A regiment a Choir Hall trains, so it can hold a queue.
const CHOIR: u16 = 2;

/// The Hall: the main building that gives a player their tier.
const HALL: u16 = 0;
/// A Spire: powers a field 8 tiles from its edge.
const SPIRE: u16 = 1;
/// A Resonator: tunes the Grub seeps within 4 tiles of its edge.
const RESONATOR: u16 = 2;
/// A Siphon: tunes the Marrow veins within 3 tiles of its edge.
const SIPHON: u16 = 3;
/// A building that needs power, and trains CHOIR.
const CHOIR_HALL: u16 = 4;

const SECOND: u32 = TICKS_PER_SECOND;
const MINUTE: u32 = 60 * SECOND;

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
        worker,
        ConquestUnit {
            builds_only: true,
            ..worker
        },
        ConquestUnit {
            trained_at: Some(CHOIR_HALL),
            tier: 1,
            train_ticks: 30 * SECOND as u16,
            supply: 1,
            ..ConquestUnit::default()
        },
    ]
}

fn building(role: Role, armour: Armour, side: u8, hp: u16, flags: u16) -> BuildingStats {
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
        links: 0,
    }
}

fn buildings() -> [BuildingStats; 5] {
    [
        BuildingStats {
            supply: 40,
            cost_grub: 400,
            build_ticks: 1000,
            ..building(
                Role::Main,
                Armour::Living,
                5,
                3000,
                DROP_OFF_GRUB | DROP_OFF_MARROW,
            )
        },
        BuildingStats {
            field: 8,
            ..building(Role::Housing, Armour::Structure, 2, 1500, POWERS_FIELD)
        },
        BuildingStats {
            role: Role::Economy,
            tunes: Some((Resource::Grub, 4)),
            ..building(Role::Economy, Armour::Structure, 2, 1500, 0)
        },
        BuildingStats {
            role: Role::Economy,
            tunes: Some((Resource::Marrow, 3)),
            ..building(Role::Economy, Armour::Structure, 2, 1500, 0)
        },
        building(Role::Production, Armour::Structure, 3, 1500, NEEDS_POWER),
    ]
}

fn seep(x: i32, y: i32) -> NodeSetup {
    NodeSetup {
        resource: Resource::Grub,
        at: FxVec2::from_ints(x, y),
        amount: SEEP_GRUB,
    }
}

fn vein(x: i32, y: i32) -> NodeSetup {
    NodeSetup {
        resource: Resource::Marrow,
        at: FxVec2::from_ints(x, y),
        amount: VEIN_MARROW,
    }
}

/// An open map 64 tiles across, in Conquest at Standard tempo, with the
/// given nodes. Both players start with 100 Grub.
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

fn grub(w: &World) -> u32 {
    w.bank(0).unwrap().grub
}

fn marrow(w: &World) -> u32 {
    w.bank(0).unwrap().marrow
}

/// The building tuning node `node`, as of the last tick.
fn tuner_of_node(w: &World, node: u16) -> Option<StructureHandle> {
    w.economy()
        .unwrap()
        .nodes()
        .find(|n| n.index == node)
        .and_then(|n| n.tuner)
}

/// Workers filling a load at node `node`, as of the last tick.
fn node_workers(w: &World, node: u16) -> u8 {
    w.economy()
        .unwrap()
        .nodes()
        .find(|n| n.index == node)
        .map_or(0, |n| n.workers)
}

/// Ticks left on the regiment at the front of building `h`'s queue.
fn front_ticks_left(w: &World, h: StructureHandle) -> Option<u32> {
    w.production()?.ticks_left(h, 0)
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// A Spire's field powers a spot 8 tiles from its edge and not 9, and a
/// field of another player's Spire does not power it for the owner.
#[test]
fn a_powered_building_can_be_laid_only_where_one_of_its_owners_fields_reaches() {
    let mut w = conquest(&[]);
    put_up(&mut w, 0, HALL, 2, 2);
    put_up(&mut w, 1, HALL, 50, 50);
    let spot = (30, 30);
    assert_eq!(
        w.can_lay(0, CHOIR_HALL, spot),
        Err(Refusal::Unpowered),
        "no field stands at all"
    );

    // 4 tiles below the spot, but it is player 1's field.
    put_up(&mut w, 1, SPIRE, 30, 37);
    run(&mut w, 2);
    assert_eq!(
        w.can_lay(0, CHOIR_HALL, spot),
        Err(Refusal::Unpowered),
        "an enemy's field does not power it for player 0"
    );
    assert_eq!(
        w.can_lay(1, CHOIR_HALL, spot),
        Ok(()),
        "player 1's own field does"
    );

    // Our Spire 9 tiles east of the spot's edge: one tile out of reach.
    let far = put_up(&mut w, 0, SPIRE, 42, 30);
    run(&mut w, 2);
    assert_eq!(
        w.can_lay(0, CHOIR_HALL, spot),
        Err(Refusal::Unpowered),
        "9 tiles from the Spire's edge is out of its field"
    );
    assert!(!w.power_reaches(0, CHOIR_HALL, spot));

    // 8 tiles off is in reach.
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: far }
    ));
    put_up(&mut w, 0, SPIRE, 41, 30);
    run(&mut w, 2);
    assert_eq!(
        w.can_lay(0, CHOIR_HALL, spot),
        Ok(()),
        "8 tiles from the Spire's edge is in its field"
    );
    assert!(w.power_reaches(0, CHOIR_HALL, spot));
}

/// A building that needs power and has a regiment queued makes no progress
/// while unpowered: its queue percent and the ticks left on its front
/// regiment stay put. A Spire's field reaching it lets training go on from
/// where it stopped, and cutting the field stops it again. `powered` is
/// Some for a building that needs power and None for one that doesn't.
#[test]
fn an_unpowered_building_trains_nothing_until_a_field_reaches_it() {
    let mut w = conquest(&[]);
    let hall = put_up(&mut w, 0, HALL, 2, 2);
    let choir = put_up(&mut w, 0, CHOIR_HALL, 30, 30);
    assert_eq!(w.powered(choir), Some(false), "no field stands");
    assert_eq!(w.powered(hall), None, "the Hall needs no power");

    assert!(order(
        &mut w,
        0,
        Command::Train {
            structure: choir,
            unit: CHOIR
        }
    ));
    run(&mut w, 3 * SECOND);
    assert_eq!(w.queue_percent(choir, 0), Some(0), "unpowered: no progress");
    assert_eq!(
        front_ticks_left(&w, choir),
        None,
        "and nothing ticking down"
    );

    // A Spire 8 tiles off the building's edge powers it.
    let spire = put_up(&mut w, 0, SPIRE, 41, 30);
    run(&mut w, 2);
    assert_eq!(w.powered(choir), Some(true));
    assert_eq!(w.powered(spire), None, "a Spire needs no power itself");
    run(&mut w, 3 * SECOND);
    let progressed = w.queue_percent(choir, 0).unwrap();
    let left = front_ticks_left(&w, choir).expect("training has started");
    assert!(progressed > 0, "powered: training goes on");

    // Its field is taken away: the queue stops where it is.
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: spire }
    ));
    run(&mut w, 1);
    assert_eq!(w.powered(choir), Some(false));
    run(&mut w, 5 * SECOND);
    assert_eq!(
        w.queue_percent(choir, 0),
        Some(progressed),
        "unpowered again: the percent stays put"
    );
    assert_eq!(
        front_ticks_left(&w, choir),
        Some(left),
        "and so do the ticks left"
    );
}

/// A Resonator tunes the Grub seeps within 4 tiles of its edge: with no
/// workers at all, the seep brings its owner a load of 10 Grub every 3
/// seconds, about 200 Grub a minute. A seep 6 tiles from its edge is past
/// its reach, is not tuned, and brings in nothing.
#[test]
fn a_resonator_tunes_a_seep_within_four_tiles_and_not_one_six_away() {
    let mut w = conquest(&[seep(36, 22), seep(38, 22)]);
    let resonator = put_up(&mut w, 0, RESONATOR, 30, 22);
    run(&mut w, 1);
    assert_eq!(
        tuner_of_node(&w, 0),
        Some(resonator),
        "the seep 4 tiles off names the Resonator"
    );
    assert_eq!(
        tuner_of_node(&w, 1),
        None,
        "the seep 6 tiles off is untuned"
    );
    assert_eq!(node_workers(&w, 0), 0, "and no workers are there");

    let start = grub(&w);
    run(&mut w, MINUTE);
    let gained = grub(&w) - start;
    assert!(
        (180..=210).contains(&gained),
        "{gained} Grub in a minute, expected about 200"
    );
}

/// A Siphon tunes the Marrow veins within 3 tiles of its edge: a load of 5
/// Marrow every 6 seconds, about 50 Marrow a minute, with no workers.
#[test]
fn a_siphon_tunes_a_vein_and_brings_about_fifty_marrow_a_minute() {
    let mut w = conquest(&[vein(34, 40)]);
    let siphon = put_up(&mut w, 0, SIPHON, 30, 40);
    run(&mut w, 1);
    assert_eq!(
        tuner_of_node(&w, 0),
        Some(siphon),
        "the vein 2 tiles off names the Siphon"
    );

    let start = marrow(&w);
    run(&mut w, MINUTE);
    let gained = marrow(&w) - start;
    assert!(
        (45..=55).contains(&gained),
        "{gained} Marrow in a minute, expected about 50"
    );
}

/// Two tuners in reach of one seep: only the one whose centre is nearer
/// tunes it, so the seep brings in what one tuner's seep does and no more.
#[test]
fn of_two_tuners_in_reach_of_a_seep_only_the_nearer_one_tunes_it() {
    let mut w = conquest(&[seep(40, 22)]);
    // Near: its edge is 3 tiles off the seep, its centre 4.1 tiles.
    let near = put_up(&mut w, 0, RESONATOR, 35, 22);
    // Far: its edge is 4 tiles off the seep (in reach), its centre 5.1.
    let far = put_up(&mut w, 0, RESONATOR, 40, 26);
    run(&mut w, 1);
    assert_eq!(
        tuner_of_node(&w, 0),
        Some(near),
        "the nearer Resonator takes the seep"
    );
    assert_ne!(tuner_of_node(&w, 0), Some(far));

    let start = grub(&w);
    run(&mut w, MINUTE);
    let gained = grub(&w) - start;
    assert!(
        (180..=210).contains(&gained),
        "{gained} Grub in a minute: one tuner's worth, about 200"
    );
}

/// A seep yields only while its tuner stands: taken away, the tuner stops
/// the seep the very tick it goes, and nothing comes in after that.
#[test]
fn a_seep_stops_yielding_when_its_tuner_is_taken_away() {
    let mut w = conquest(&[seep(36, 22)]);
    let resonator = put_up(&mut w, 0, RESONATOR, 30, 22);
    let before = grub(&w);
    run(&mut w, 30 * SECOND);
    assert!(grub(&w) > before, "it yields while the Resonator stands");

    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure {
            structure: resonator
        }
    ));
    assert_eq!(tuner_of_node(&w, 0), None, "the seep is untuned at once");
    let after = grub(&w);
    run(&mut w, MINUTE);
    assert_eq!(grub(&w), after, "and gives nothing more");
}

/// A worker that builds only (a Tuner) is refused a Gather order; a plain
/// worker sent to the same seep takes it.
#[test]
fn a_builds_only_worker_is_refused_a_gather_order_and_a_plain_one_is_not() {
    let mut w = conquest(&[seep(40, 22)]);
    let tuner = spawn(&mut w, 0, TUNER, 36, 26);
    let plain = spawn(&mut w, 0, WORKER, 36, 30);
    assert!(w.builds_only(tuner));
    assert!(!w.builds_only(plain));

    assert!(!gather(&mut w, tuner, 0), "a Tuner can't be sent to gather");
    assert!(gather(&mut w, plain, 0), "a plain worker can");
    run(&mut w, 1);
    assert_eq!(w.gathers_at(tuner), None, "the Tuner gathers nothing");
    assert_eq!(w.gathers_at(plain), Some(0), "the plain worker is sent");
}
