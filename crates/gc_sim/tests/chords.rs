//! Chords, harmony and Resonance (docs/03-factions.md, the Choir's economy):
//! the Resonators that link, the triangles three of one owner's make, the
//! seeps a chord sings over, and the meter those triangles charge.

use gc_fixed::FxVec2;
use gc_sim::chords::{MAX_HARMONY, MAX_RESONANCE, MIN_HARMONY, RESONANCE_TICKS};
use gc_sim::economy::{NodeSetup, Resource, SEEP_GRUB, VEIN_MARROW};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::structures::{Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, NEEDS_POWER, POWERS_FIELD, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MAX_PARTIES, MatchSetup,
    PlayerCommand, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

/// A Resonator: links with its owner's other Resonators 14 tiles off, and
/// tunes the Grub seeps within 4 tiles of its edge.
const RESONATOR: u16 = 2;
/// A Siphon: tunes the Marrow veins within 3 tiles of its edge, and does
/// not link.
const SIPHON: u16 = 3;
/// A building that needs power, and trains a unit.
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

/// The building table: the Hall, a Spire, a Resonator, a Siphon and a
/// Choir Hall.
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
            links: 14,
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

fn grub(w: &World) -> u32 {
    w.bank(0).unwrap().grub
}

fn marrow(w: &World) -> u32 {
    w.bank(0).unwrap().marrow
}

/// Player `p`'s chord count, as of the last tick.
fn chords_of(w: &World, p: u8) -> u32 {
    w.bank(p).unwrap().chords
}

/// Player `p`'s Resonance in points, as of the last tick.
fn resonance_of(w: &World, p: u8) -> u32 {
    w.bank(p).unwrap().resonance
}

/// The building tuning node `node`, as of the last tick.
fn tuner_of_node(w: &World, node: u16) -> Option<StructureHandle> {
    w.economy()
        .unwrap()
        .nodes()
        .find(|n| n.index == node)
        .and_then(|n| n.tuner)
}

/// How many percent more node `node` yields under its tuner's chord, as of
/// the last tick.
fn harmony_of_node(w: &World, node: u16) -> u32 {
    w.economy()
        .unwrap()
        .nodes()
        .find(|n| n.index == node)
        .map_or(0, |n| u32::from(n.harmony))
}

/// The harmony of the chord with a corner centred on tile `(x, y)`, if
/// there is one this tick.
fn chord_harmony_at(w: &World, x: i32, y: i32) -> Option<u8> {
    let centre = FxVec2::from_ints(x, y);
    w.chords()
        .iter()
        .find(|c| c.at.contains(&centre))
        .map(|c| c.harmony)
}

/// Points of Resonance player 0 gains over `ticks` more ticks.
fn gained(w: &mut World, ticks: u32) -> u32 {
    let before = resonance_of(w, 0);
    run(w, ticks);
    resonance_of(w, 0) - before
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Whether a harmony is within the bounds a chord can give.
fn in_bounds(harmony: u32) -> bool {
    (u32::from(MIN_HARMONY)..=u32::from(MAX_HARMONY)).contains(&harmony)
}

/// Three of one player's Resonators within 14 tiles of each other make one
/// chord, and moving one of them 15 tiles off takes the chord away.
#[test]
fn three_resonators_within_fourteen_tiles_make_a_chord_and_fifteen_do_not() {
    let mut w = conquest(&[]);
    // Centres (31,23), (45,23) and (38,35): 14, 13.9 and 13.4 tiles apart.
    let a = put_up(&mut w, 0, RESONATOR, 30, 22);
    let b = put_up(&mut w, 0, RESONATOR, 44, 22);
    let c = put_up(&mut w, 0, RESONATOR, 37, 34);
    run(&mut w, 1);
    assert_eq!(w.chords().len(), 1, "three that link make one chord");
    let chord = w.chords()[0];
    assert_eq!(chord.owner, 0);
    for h in [a, b, c] {
        assert!(chord.corners.contains(&h), "{h:?} is a corner");
    }
    assert_eq!(chords_of(&w, 0), 1);

    // The third moved to centre (31,38): 15 tiles from the first.
    assert!(order(&mut w, 0, Command::RemoveStructure { structure: c }));
    put_up(&mut w, 0, RESONATOR, 30, 37);
    run(&mut w, 1);
    assert!(w.chords().is_empty(), "15 tiles off, it links with neither");
    assert_eq!(chords_of(&w, 0), 0);
}

/// Two players' Resonators never link into a chord together: only three of
/// one player's own do.
#[test]
fn two_players_buildings_never_form_a_chord_together() {
    let mut w = conquest(&[]);
    // Player 0 at centres (31,23) and (45,23); player 1 at (38,35) and
    // (38,29). Every pair is within 14 tiles.
    put_up(&mut w, 0, RESONATOR, 30, 22);
    put_up(&mut w, 0, RESONATOR, 44, 22);
    put_up(&mut w, 1, RESONATOR, 37, 34);
    put_up(&mut w, 1, RESONATOR, 37, 28);
    run(&mut w, 1);
    assert!(w.chords().is_empty(), "two each: no triangle of one owner");
    assert_eq!(chords_of(&w, 0), 0);
    assert_eq!(chords_of(&w, 1), 0);

    // Player 0's third, at centre (38,11), makes a chord of their own.
    put_up(&mut w, 0, RESONATOR, 37, 10);
    run(&mut w, 1);
    assert_eq!(w.chords().len(), 1);
    assert_eq!(w.chords()[0].owner, 0);
    assert_eq!(chords_of(&w, 0), 1);
    assert_eq!(chords_of(&w, 1), 0, "player 1 still has only two");
}

/// Three of one player's Resonators in a straight line, each pair within 14
/// tiles, make no chord and charge no Resonance.
#[test]
fn three_resonators_in_a_straight_line_make_no_chord() {
    let mut w = conquest(&[]);
    // Centres (31,23), (38,23) and (45,23): the outer two are 14 tiles apart.
    put_up(&mut w, 0, RESONATOR, 30, 22);
    put_up(&mut w, 0, RESONATOR, 37, 22);
    put_up(&mut w, 0, RESONATOR, 44, 22);
    run(&mut w, 1);
    assert!(w.chords().is_empty(), "collinear corners make no chord");
    assert_eq!(chords_of(&w, 0), 0);
    assert_eq!(gained(&mut w, MINUTE), 0, "and no Resonance is charged");
}

/// A seep inside a chord, tuned by one of its corners, yields about 1.6
/// times the Grub of the same seep tuned by a lone Resonator, and a Marrow
/// vein inside a chord gets no harmony at all.
#[test]
fn a_seep_inside_a_chord_yields_more_and_a_vein_inside_one_does_not() {
    // The seep at (34,25) lies inside the triangle with centres (31,23),
    // (45,23) and (38,35), and 4 tiles from the first corner's edge.
    let mut chord_world = conquest(&[seep(34, 25)]);
    let corner = put_up(&mut chord_world, 0, RESONATOR, 30, 22);
    put_up(&mut chord_world, 0, RESONATOR, 44, 22);
    put_up(&mut chord_world, 0, RESONATOR, 37, 34);
    run(&mut chord_world, 1);
    assert_eq!(tuner_of_node(&chord_world, 0), Some(corner));
    let harmony = chord_world.chords()[0].harmony;
    assert!(in_bounds(u32::from(harmony)), "{harmony} percent");
    assert_eq!(
        harmony_of_node(&chord_world, 0),
        u32::from(harmony),
        "the seep takes its chord's harmony"
    );

    let start = grub(&chord_world);
    run(&mut chord_world, MINUTE);
    let in_chord = grub(&chord_world) - start;

    let mut lone = conquest(&[seep(34, 25)]);
    put_up(&mut lone, 0, RESONATOR, 30, 22);
    run(&mut lone, 1);
    assert_eq!(harmony_of_node(&lone, 0), 0, "no chord covers the seep");
    let start = grub(&lone);
    run(&mut lone, MINUTE);
    let outside = grub(&lone) - start;
    let percent = in_chord * 100 / outside;
    assert!(
        (150..=170).contains(&percent),
        "{in_chord} Grub inside a chord against {outside} outside: {percent}%"
    );

    // A Siphon tunes a vein inside the same chord: no harmony, and the
    // yield is the one a vein gives outside any chord.
    let mut veins = conquest(&[vein(34, 25)]);
    put_up(&mut veins, 0, RESONATOR, 30, 22);
    put_up(&mut veins, 0, RESONATOR, 44, 22);
    put_up(&mut veins, 0, RESONATOR, 37, 34);
    let siphon = put_up(&mut veins, 0, SIPHON, 34, 27);
    run(&mut veins, 1);
    assert_eq!(veins.chords().len(), 1, "the chord is there");
    assert_eq!(tuner_of_node(&veins, 0), Some(siphon));
    assert_eq!(harmony_of_node(&veins, 0), 0, "veins get no harmony");
    let start = marrow(&veins);
    run(&mut veins, MINUTE);
    let gained = marrow(&veins) - start;
    assert!(
        (45..=55).contains(&gained),
        "{gained} Marrow a minute, as with no chord"
    );
}

/// A near-equilateral chord of side 13 has more harmony than a small one,
/// and both stay within the bounds a chord can give.
#[test]
fn a_bigger_chord_has_more_harmony_and_both_stay_within_the_bounds() {
    let mut w = conquest(&[]);
    // A small chord in the north-west, with centres (5,5), (7,5) and (6,7):
    // twice its area is 4 square tiles.
    put_up(&mut w, 0, RESONATOR, 4, 4);
    put_up(&mut w, 0, RESONATOR, 6, 4);
    put_up(&mut w, 0, RESONATOR, 5, 6);
    // A big one, with centres (31,23), (44,23) and (38,35): sides 13, 13.4
    // and 13.9 tiles, twice its area 156 square tiles.
    put_up(&mut w, 0, RESONATOR, 30, 22);
    put_up(&mut w, 0, RESONATOR, 43, 22);
    put_up(&mut w, 0, RESONATOR, 37, 34);
    run(&mut w, 1);
    assert_eq!(w.chords().len(), 2);

    let small = chord_harmony_at(&w, 5, 5).expect("the small chord");
    let big = chord_harmony_at(&w, 31, 23).expect("the big chord");
    assert!(in_bounds(u32::from(small)), "small: {small} percent");
    assert!(in_bounds(u32::from(big)), "big: {big} percent");
    assert!(big > small, "big {big} percent, small {small} percent");
    assert!(big >= 50, "a near-equilateral chord gives {big} percent");
}

/// Each chord charges Resonance a point per RESONANCE_TICKS chord-ticks, but
/// a player's chords past three, or past two fewer than their linking
/// buildings, charge no faster.
#[test]
fn resonance_charges_with_each_chord_but_no_faster_past_three() {
    // Ten points at one chord's rate: 30 seconds.
    let window = 10 * RESONANCE_TICKS;
    let mut w = conquest(&[]);
    put_up(&mut w, 0, RESONATOR, 30, 22);
    put_up(&mut w, 0, RESONATOR, 44, 22);
    run(&mut w, 1);
    assert_eq!(gained(&mut w, window), 0, "two buildings charge nothing");

    put_up(&mut w, 0, RESONATOR, 37, 34);
    assert_eq!(chords_of(&w, 0), 1);
    assert_eq!(gained(&mut w, window), 10, "one chord: 10 points");

    // On the other side of the first two: a second chord, not linked to the
    // third.
    put_up(&mut w, 0, RESONATOR, 37, 10);
    assert_eq!(chords_of(&w, 0), 2);
    assert_eq!(gained(&mut w, window), 20, "two chords: 20 points");

    // A fifth building inside the triangle makes three more chords (five in
    // all), but only three of them count towards the meter.
    put_up(&mut w, 0, RESONATOR, 37, 28);
    assert_eq!(chords_of(&w, 0), 5);
    assert_eq!(gained(&mut w, window), 30, "five chords charge as three");
}

/// Four buildings in a square make four overlapping chords, which charge as
/// the two that don't overlap.
#[test]
fn overlapping_chords_charge_as_the_triangles_that_do_not_overlap() {
    let window = 10 * RESONANCE_TICKS;
    let mut w = conquest(&[]);
    for (x, y) in [(30, 14), (39, 14), (30, 23), (39, 23)] {
        put_up(&mut w, 0, RESONATOR, x, y);
    }
    run(&mut w, 1);
    assert_eq!(chords_of(&w, 0), 4);
    assert_eq!(
        gained(&mut w, window),
        20,
        "four buildings charge as two chords"
    );
}

/// Resonance rises 20 points a minute from one chord, and stops at its
/// maximum.
#[test]
fn resonance_rises_twenty_a_minute_from_one_chord_and_stops_at_its_maximum() {
    let mut w = conquest(&[]);
    put_up(&mut w, 0, RESONATOR, 30, 22);
    put_up(&mut w, 0, RESONATOR, 44, 22);
    put_up(&mut w, 0, RESONATOR, 37, 34);
    run(&mut w, 1);
    assert_eq!(gained(&mut w, MINUTE), 20, "one chord: 20 points a minute");

    run(&mut w, 3 * MINUTE);
    assert_eq!(resonance_of(&w, 0), 80, "four minutes in");
    run(&mut w, 3 * MINUTE);
    assert_eq!(resonance_of(&w, 0), MAX_RESONANCE, "capped at the maximum");
    assert_eq!(w.bank(0).unwrap().resonance, MAX_RESONANCE);
    assert_eq!(gained(&mut w, MINUTE), 0, "and it stays there");
}

/// Destroying one corner of a chord breaks it on the next tick and Resonance
/// stops rising; rebuilding the corner makes the chord again.
#[test]
fn destroying_a_corner_breaks_the_chord_and_stops_resonance() {
    let window = 10 * RESONANCE_TICKS;
    let mut w = conquest(&[]);
    put_up(&mut w, 0, RESONATOR, 30, 22);
    put_up(&mut w, 0, RESONATOR, 44, 22);
    let corner = put_up(&mut w, 0, RESONATOR, 37, 34);
    run(&mut w, 1);
    assert_eq!(chords_of(&w, 0), 1);
    assert_eq!(gained(&mut w, window), 10, "the chord charges");

    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: corner }
    ));
    assert!(w.chords().is_empty(), "the chord is gone at once");
    assert_eq!(chords_of(&w, 0), 0);
    assert_eq!(gained(&mut w, MINUTE), 0, "and Resonance stops rising");

    put_up(&mut w, 0, RESONATOR, 37, 34);
    run(&mut w, 1);
    assert_eq!(w.chords().len(), 1, "a new corner makes the chord again");
    assert_eq!(gained(&mut w, window), 10, "and it charges again");
}
