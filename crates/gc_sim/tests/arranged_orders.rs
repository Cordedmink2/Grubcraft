//! A scrambled mixed army given an arranged group order marches into the
//! shape it was asked for: every regiment reaches its place and stands in
//! order, not tangled up in its friends.

use gc_fixed::{Fx, FxVec2};
use gc_sim::arrange::{Arrangement, Preset, Role, arranged_move};
use gc_sim::formation::{GroupMember, GroupOrder};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Mass, MatchSetup, PlayerCommand, TICKS_PER_SECOND, UnitStats, World,
};

/// The drill types plus a squadron of horse (type 4).
fn units() -> Vec<UnitStats> {
    let mut u = DRILL_UNITS.to_vec();
    u.push(UnitStats {
        speed: Fx::from_int(6),
        models: 8,
        files: 4,
        file_spacing: Fx::from_ratio(7, 5),
        rank_spacing: Fx::from_int(2),
        mass: Mass::Heavy,
        ..DRILL_UNITS[1]
    });
    u
}

/// The army as it stands before the order: archers (type 2) in front,
/// horse in the middle, foot (types 0, 1, 3) at the back, facing north
/// around y = 150.
const ARMY: [(u16, i32, i32); 9] = [
    (2, 100, 140),
    (2, 125, 140),
    (4, 112, 150),
    (4, 140, 150),
    (0, 95, 162),
    (1, 110, 162),
    (3, 125, 162),
    (0, 140, 162),
    (0, 155, 162),
];

fn run(preset: Preset) {
    let units = units();
    let mut w = World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities::default(),
        units: &units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let spawns: Vec<PlayerCommand> = ARMY
        .iter()
        .map(|&(unit, x, y)| PlayerCommand {
            player: 0,
            command: Command::SpawnRegiment {
                unit,
                position: FxVec2::from_ints(x, y),
                facing: NORTH,
            },
        })
        .collect();
    w.tick(&spawns);
    let mut members: Vec<GroupMember> = w
        .regiments
        .iter()
        .map(|r| GroupMember::of(&r, &units[r.unit as usize]))
        .collect();
    assert_eq!(
        members.iter().filter(|m| m.role == Role::Flanker).count(),
        2
    );
    let mut moves = Vec::new();
    arranged_move(
        &mut members,
        GroupOrder {
            target: FxVec2::from_ints(128, 90),
            facing: Some(NORTH),
            width: None,
        },
        Arrangement::Preset(preset),
        &mut moves,
    );
    let orders: Vec<PlayerCommand> = moves
        .iter()
        .map(|&command| PlayerCommand { player: 0, command })
        .collect();
    w.tick(&orders);

    let limit = 120 * TICKS_PER_SECOND;
    while w.regiments.iter().any(|r| r.moving || r.disorder > 0) {
        assert!(
            w.tick_count() < limit,
            "{preset:?}: still moving or in disorder"
        );
        w.tick(&[]);
    }
    for c in &moves {
        let Command::MoveTo {
            regiment,
            target,
            facing,
            ..
        } = *c
        else {
            unreachable!()
        };
        let r = w.regiments.get(regiment).unwrap();
        let off = (r.position - target).length();
        assert!(
            off <= Fx::from_int(2),
            "{preset:?}: regiment {} stopped {off:?} tiles from its place",
            regiment.index
        );
        let turn = r.facing.delta_to(facing.unwrap());
        assert!(turn.unsigned_abs() < 0x400, "{preset:?}: {r:?}");
    }
    if preset == Preset::Line {
        // North is smaller y: the foot stand in front of the archers.
        let y = |unit: u16| -> Vec<Fx> {
            w.regiments
                .iter()
                .filter(|r| r.unit == unit)
                .map(|r| r.position.y)
                .collect()
        };
        let foot = y(0);
        assert!(y(2).iter().all(|a| foot.iter().all(|f| f < a)));
    }
}

#[test]
fn a_scrambled_army_forms_a_line() {
    run(Preset::Line);
}

#[test]
fn a_scrambled_army_forms_each_preset() {
    for preset in [Preset::Deep, Preset::Hammer, Preset::Square] {
        run(preset);
    }
}
