//! Faction powers (docs/17-unit-rosters.md, the Choir's Resonance; the table
//! is data/rosters/choir.ron): Overclock hastes one of the caster's buildings
//! or regiments, Phase Shift steps regiments to a Resonator, and both are
//! paid from Resonance, held to the caster's tier and cooled down after.

use gc_fixed::{Fx, FxVec2};
use gc_sim::economy::{NodeSetup, Resource, SEEP_GRUB};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{
    Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, NEEDS_POWER, POWERS_FIELD, Role, SILENCES, TowerAttack,
};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MAX_PARTIES, MatchSetup,
    Meter, PlayerCommand, PowerEffect, PowerStats, PowerTarget, Refusal, RegimentHandle,
    StructureHandle, TICKS_PER_SECOND, TickStats, UnitStats, World,
};

const HALL: u16 = 0;
/// Choir: powers the buildings in its field, so the Choir Hall can work.
const HOUSING: u16 = 1;
/// A Resonator: links with its owner's other Resonators 14 tiles off, and
/// tunes the Grub seeps within 4 tiles of its edge.
const RESONATOR: u16 = 2;
/// A Siphon: tunes the Marrow veins within 3 tiles of its edge, and does not
/// link.
const SIPHON: u16 = 3;
/// A building that needs power, and trains the regiment below.
const CHOIR_HALL: u16 = 4;
/// A tier 2 main building, for Phase Shift.
const KEEP: u16 = 5;
/// A Bell Tower: it tolls when enemies come near, silencing their powers.
const BELL: u16 = 6;
/// Trained at the Choir Hall.
const CHOIR_UNIT: u16 = 2;

/// The powers table: index 0 Overclock, 1 Phase Shift, 2 a power not built.
const OVERCLOCK: u8 = 0;
const PHASE_SHIFT: u8 = 1;
const NOT_BUILT: u8 = 2;
const POWERS: [PowerStats; 3] = [
    PowerStats {
        tier: 1,
        cost: 20,
        meter: Meter::Resonance,
        cooldown_ticks: 400,
        duration_ticks: 400,
        effect: PowerEffect::Haste { percent: 50 },
    },
    PowerStats {
        tier: 2,
        cost: 60,
        meter: Meter::Resonance,
        cooldown_ticks: 1800,
        duration_ticks: 0,
        effect: PowerEffect::Shift { to: RESONATOR },
    },
    PowerStats {
        tier: 3,
        cost: 80,
        meter: Meter::Resonance,
        cooldown_ticks: 3000,
        duration_ticks: 0,
        effect: PowerEffect::NotBuilt,
    },
];

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
            train_ticks: 30 * TICKS_PER_SECOND as u16,
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

/// The building table: the Hall, a Housing that powers a field, a
/// Resonator, a Siphon, a Choir Hall that needs power, a tier 2 Keep and a
/// Bell Tower.
fn buildings() -> [BuildingStats; 7] {
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
        BuildingStats {
            tier: 2,
            ..building(Role::Main, Armour::Fortified, 5, 3000, 0)
        },
        BuildingStats {
            attack: Some(TowerAttack {
                range: Fx::from_int(30),
                damage: 1,
                reload_ticks: 40,
                shots: 1,
            }),
            ..building(Role::Defence, Armour::Fortified, 2, 900, SILENCES)
        },
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
/// given nodes and the powers table. Both players start with 100 Grub.
fn conquest(nodes: &[NodeSetup]) -> World {
    let side = 64 * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 9,
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
        powers: &POWERS,
        meters: [Meter::Resonance; MAX_PARTIES],
        ..ConquestSetup::default()
    }));
    w
}

/// Player 0 with a Hall, a Housing and a Choir Hall in its field, and three
/// Resonators making one chord: centres (31,23), (45,23) and (38,35). The
/// first Resonator is at tile (30,22).
fn chorded(nodes: &[NodeSetup]) -> World {
    let mut w = conquest(nodes);
    put_up(&mut w, 0, HALL, 4, 4);
    put_up(&mut w, 0, HOUSING, 12, 56);
    put_up(&mut w, 0, CHOIR_HALL, 18, 56);
    put_up(&mut w, 0, RESONATOR, 30, 22);
    put_up(&mut w, 0, RESONATOR, 44, 22);
    put_up(&mut w, 0, RESONATOR, 37, 34);
    run(&mut w, 1);
    assert_eq!(w.chords().len(), 1);
    w
}

/// One tick with one command, from `player`.
fn tick_one(w: &mut World, player: u8, command: Command) -> TickStats {
    w.tick(&[PlayerCommand { player, command }])
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    tick_one(w, player, command).commands_applied == 1
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

/// Player `player` spawns a regiment of type `unit` at `(x, y)`.
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

fn position(w: &World, h: RegimentHandle) -> FxVec2 {
    w.regiments.iter().find(|r| r.handle == h).unwrap().position
}

/// Player 0's regiments of type `unit`.
fn count(w: &World, unit: u16) -> usize {
    w.regiments
        .iter()
        .filter(|r| r.owner == 0 && r.unit == unit)
        .count()
}

fn grub(w: &World) -> u32 {
    w.bank(0).unwrap().grub
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

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

/// Runs until a point of Resonance comes in, which is the tick the meter
/// passes a whole multiple of [`gc_sim::chords::RESONANCE_TICKS`]: from
/// then, every 60 ticks of one chord are exactly one point. Returns the
/// points.
fn align(w: &mut World) -> u32 {
    let start = resonance_of(w, 0);
    for _ in 0..10_000 {
        w.tick(&[]);
        if resonance_of(w, 0) != start {
            return resonance_of(w, 0);
        }
    }
    panic!("no point of Resonance in 10000 ticks");
}

fn overclock(h: StructureHandle) -> Command {
    Command::UsePower {
        power: OVERCLOCK,
        target: PowerTarget::Structure(h),
    }
}

fn shift(regiment: RegimentHandle, to: StructureHandle) -> Command {
    Command::UsePower {
        power: PHASE_SHIFT,
        target: PowerTarget::Shift { regiment, to },
    }
}

/// Ticks from the first tick of `first` until a regiment of [`CHOIR_UNIT`]
/// is trained, counting that tick.
fn ticks_to_train(w: &mut World, first: &[PlayerCommand]) -> u32 {
    let mut commands = first;
    for t in 1..=1000 {
        w.tick(commands);
        commands = &[];
        if count(w, CHOIR_UNIT) > 0 {
            return t;
        }
    }
    panic!("no regiment trained in 1000 ticks");
}

/// A power is paid from Resonance: without its price it is refused and the
/// command is rejected, and nothing changes.
#[test]
fn a_power_is_refused_without_its_resonance_and_the_command_is_rejected() {
    let mut w = chorded(&[]);
    let r1 = at(&w, 30, 22).unwrap();
    assert_eq!(resonance_of(&w, 0), 0);
    assert_eq!(w.can_use_power(0, OVERCLOCK), Err(Refusal::Price));

    let stats = tick_one(&mut w, 0, overclock(r1));
    assert_eq!(stats.commands_rejected, 1);
    assert_eq!(stats.commands_applied, 0);
    assert_eq!(resonance_of(&w, 0), 0);
    assert_eq!(w.structure_haste(r1), 0, "no haste");
    assert_eq!(w.power_cooling(0, OVERCLOCK), 0, "no cooldown started");
}

/// Overclock on your own building costs 20 points, cools for 400 ticks, and
/// works again once that has passed and the points are there.
#[test]
fn overclock_on_your_own_building_costs_twenty_points_and_cools_for_400_ticks() {
    let mut w = chorded(&[]);
    let r1 = at(&w, 30, 22).unwrap();
    align(&mut w);
    run(&mut w, 40 * 60);
    let before = resonance_of(&w, 0);
    assert!(before >= 40, "{before} points");

    let used = w.tick_count();
    assert!(order(&mut w, 0, overclock(r1)));
    assert_eq!(resonance_of(&w, 0), before - 20, "20 points");
    assert_eq!(w.power_cooling(0, OVERCLOCK), 400 - 1);
    assert_eq!(w.can_use_power(0, OVERCLOCK), Err(Refusal::Cooling));
    // A second use on the very next tick is refused, and nothing is spent.
    let stats = tick_one(&mut w, 0, overclock(r1));
    assert_eq!(stats.commands_rejected, 1);
    assert_eq!(resonance_of(&w, 0), before - 20, "nothing spent again");

    let ready = used + 400;
    let left = ready - w.tick_count() - 1;
    run(&mut w, left);
    assert_eq!(w.can_use_power(0, OVERCLOCK), Err(Refusal::Cooling));
    run(&mut w, 1);
    assert_eq!(w.tick_count(), ready);
    assert_eq!(w.can_use_power(0, OVERCLOCK), Ok(()));
    assert!(order(&mut w, 0, overclock(r1)), "works again");
}

/// Overclock on an enemy's building or regiment is rejected, and costs
/// nothing.
#[test]
fn overclock_on_an_enemy_building_or_regiment_is_rejected_and_costs_nothing() {
    let mut w = chorded(&[]);
    put_up(&mut w, 1, RESONATOR, 50, 10);
    let enemy_building = at(&w, 50, 10).unwrap();
    let enemy_regiment = spawn(&mut w, 1, CHOIR_UNIT, 50, 40);
    align(&mut w);
    run(&mut w, 25 * 60);
    let before = resonance_of(&w, 0);

    let on_building = tick_one(&mut w, 0, overclock(enemy_building));
    assert_eq!(on_building.commands_rejected, 1);
    let on_regiment = tick_one(
        &mut w,
        0,
        Command::UsePower {
            power: OVERCLOCK,
            target: PowerTarget::Regiment(enemy_regiment),
        },
    );
    assert_eq!(on_regiment.commands_rejected, 1);

    assert_eq!(resonance_of(&w, 0), before, "nothing spent");
    assert_eq!(w.power_cooling(0, OVERCLOCK), 0, "no cooldown");
    assert_eq!(w.structure_haste(enemy_building), 0);
    assert_eq!(w.regiment_haste(enemy_regiment), 0);
    assert_eq!(w.can_use_power(0, OVERCLOCK), Ok(()));
}

/// A hasted building trains a regiment in two thirds of the time, a third
/// sooner: 400 ticks where 600 take.
#[test]
fn a_hasted_building_trains_a_regiment_about_a_third_sooner() {
    // Both worlds stand the same; only the first hastes the Choir Hall.
    let mut hasted = chorded(&[]);
    let mut plain = chorded(&[]);
    run(&mut hasted, 25 * 60);
    run(&mut plain, 25 * 60);
    let hall = at(&hasted, 18, 56).unwrap();
    assert_eq!(
        hasted.powered(hall),
        Some(true),
        "the Choir Hall is powered"
    );
    let train = PlayerCommand {
        player: 0,
        command: Command::Train {
            structure: hall,
            unit: CHOIR_UNIT,
        },
    };
    let use_power = PlayerCommand {
        player: 0,
        command: overclock(hall),
    };

    let both = ticks_to_train(&mut hasted, &[use_power, train]);
    let alone = ticks_to_train(&mut plain, &[train]);
    assert!((398..=402).contains(&both), "hasted took {both} ticks");
    assert!((598..=602).contains(&alone), "unhasted took {alone} ticks");
}

/// A hasted tuning Resonator yields more Grub over the 400 ticks of its
/// haste than the same Resonator unhasted: about 1.5 times as much.
#[test]
fn a_hasted_tuning_resonator_yields_more_grub_over_the_haste() {
    fn gained(hasted: bool) -> u32 {
        let mut w = chorded(&[seep(34, 25)]);
        let r1 = at(&w, 30, 22).unwrap();
        assert_eq!(
            tuner_of_node(&w, 0),
            Some(r1),
            "the first Resonator tunes it"
        );
        run(&mut w, 25 * 60);
        let start = grub(&w);
        let first = if hasted {
            vec![PlayerCommand {
                player: 0,
                command: overclock(r1),
            }]
        } else {
            vec![]
        };
        assert_eq!(w.tick(&first).commands_applied, first.len() as u32);
        run(&mut w, 399);
        grub(&w) - start
    }
    let plain = gained(false);
    let hasted = gained(true);
    assert!(plain > 0, "the seep yields without haste");
    assert!(
        hasted * 10 >= plain * 14 && hasted * 10 <= plain * 16,
        "hasted {hasted} Grub against {plain} unhasted"
    );
}

/// The haste counts down from 400 ticks and is gone when it runs out.
#[test]
fn the_haste_wears_off_after_400_ticks() {
    let mut w = chorded(&[]);
    let r1 = at(&w, 30, 22).unwrap();
    align(&mut w);
    run(&mut w, 25 * 60);
    let used = w.tick_count();
    assert!(order(&mut w, 0, overclock(r1)));
    while w.tick_count() < used + 400 {
        assert_eq!(
            w.structure_haste(r1),
            (used + 400 - w.tick_count()) as u16,
            "ticks left at tick {}",
            w.tick_count()
        );
        run(&mut w, 1);
    }
    assert_eq!(w.structure_haste(r1), 0, "the haste is over");
    run(&mut w, 10);
    assert_eq!(w.structure_haste(r1), 0, "and stays over");
}

/// A power that isn't built, or an index past the table, is unknown and
/// refused whatever the Resonance.
#[test]
fn a_power_not_built_or_out_of_range_is_unknown() {
    let mut w = chorded(&[]);
    let r1 = at(&w, 30, 22).unwrap();
    align(&mut w);
    run(&mut w, 90 * 60);
    assert!(resonance_of(&w, 0) >= 80, "enough for the price of 80");
    assert_eq!(w.powers().len(), 3);

    for power in [NOT_BUILT, 9] {
        assert_eq!(w.can_use_power(0, power), Err(Refusal::Unknown));
        let stats = tick_one(
            &mut w,
            0,
            Command::UsePower {
                power,
                target: PowerTarget::Structure(r1),
            },
        );
        assert_eq!(stats.commands_rejected, 1, "power {power}");
    }
    assert!(resonance_of(&w, 0) >= 80, "nothing spent");
}

/// Phase Shift needs a tier 2 main building: at tier 1 it is refused until
/// a tier 2 one stands.
#[test]
fn phase_shift_needs_a_tier_two_main_building() {
    let mut w = chorded(&[]);
    let r1 = at(&w, 30, 22).unwrap();
    let mine = spawn(&mut w, 0, CHOIR_UNIT, 20, 44);
    align(&mut w);
    run(&mut w, 70 * 60);
    assert!(resonance_of(&w, 0) >= 60);

    assert_eq!(w.can_use_power(0, PHASE_SHIFT), Err(Refusal::Tier));
    let stats = tick_one(&mut w, 0, shift(mine, r1));
    assert_eq!(stats.commands_rejected, 1);

    put_up(&mut w, 0, KEEP, 56, 56);
    assert_eq!(w.can_use_power(0, PHASE_SHIFT), Ok(()));
}

/// Phase Shift moves two regiments ordered in one tick next to the chosen
/// Resonator, for one payment of 60 points, and starts its 90 s cooldown.
#[test]
fn phase_shift_moves_two_regiments_in_one_tick_for_one_payment_and_starts_its_cooldown() {
    let mut w = chorded(&[]);
    put_up(&mut w, 0, KEEP, 56, 56);
    let a = spawn(&mut w, 0, CHOIR_UNIT, 20, 44);
    let b = spawn(&mut w, 0, CHOIR_UNIT, 22, 44);
    let (a0, b0) = (position(&w, a), position(&w, b));
    let target = at(&w, 37, 34).unwrap();
    align(&mut w);
    run(&mut w, 70 * 60);
    let before = resonance_of(&w, 0);

    let stats = w.tick(&[
        PlayerCommand {
            player: 0,
            command: shift(a, target),
        },
        PlayerCommand {
            player: 0,
            command: shift(b, target),
        },
    ]);
    assert_eq!(stats.commands_applied, 2, "both shifts applied");
    assert_eq!(resonance_of(&w, 0), before - 60, "60 points once, not 120");
    assert_eq!(w.power_cooling(0, PHASE_SHIFT), 1800 - 1, "cooling 90 s");
    assert_eq!(w.can_use_power(0, PHASE_SHIFT), Err(Refusal::Cooling));

    // Next to the Resonator, whose centre is (38,35).
    let centre = FxVec2::from_ints(38, 35);
    for (h, from) in [(a, a0), (b, b0)] {
        let now = position(&w, h);
        assert!(now != from, "it moved");
        assert!(
            (now - centre).length() < Fx::from_int(12),
            "{now:?} is near"
        );
    }
    assert!(position(&w, a) != position(&w, b), "they stand apart");
}

/// A group shift the network carries over two turns is still one payment,
/// and a shift to another building is not part of it.
#[test]
fn a_group_shift_split_over_ticks_rides_on_one_payment() {
    let mut w = chorded(&[]);
    put_up(&mut w, 0, KEEP, 56, 56);
    let a = spawn(&mut w, 0, CHOIR_UNIT, 20, 44);
    let b = spawn(&mut w, 0, CHOIR_UNIT, 22, 44);
    let c = spawn(&mut w, 0, CHOIR_UNIT, 24, 44);
    let (target, other) = (at(&w, 37, 34).unwrap(), at(&w, 30, 22).unwrap());
    align(&mut w);
    run(&mut w, 70 * 60);
    let before = resonance_of(&w, 0);

    assert!(order(&mut w, 0, shift(a, target)));
    assert!(order(&mut w, 0, shift(b, target)), "rides a tick later");
    assert_eq!(resonance_of(&w, 0), before - 60, "paid once");
    assert!(position(&w, a) != position(&w, b), "they stand apart");
    assert!(!order(&mut w, 0, shift(c, other)), "another building pays");
    run(&mut w, gc_sim::powers::RIDE_TICKS);
    assert!(!order(&mut w, 0, shift(c, target)), "too late to ride");
}

/// A regiment shifted out of a melee stays where it was shifted to rather
/// than marching back to its foe.
#[test]
fn a_regiment_shifted_out_of_a_fight_leaves_it() {
    let mut w = chorded(&[]);
    put_up(&mut w, 0, KEEP, 56, 56);
    let mine = spawn(&mut w, 0, CHOIR_UNIT, 20, 44);
    let target = at(&w, 37, 34).unwrap();
    align(&mut w);
    run(&mut w, 70 * 60);
    let foe = spawn(&mut w, 1, CHOIR_UNIT, 20, 48);
    assert!(order(
        &mut w,
        0,
        Command::Attack {
            regiment: mine,
            target: foe
        }
    ));
    let engaged = |w: &World| w.regiments.iter().any(|r| r.handle == mine && r.engaged);
    let mut ticks = 0;
    while !engaged(&w) {
        run(&mut w, 1);
        ticks += 1;
        assert!(ticks < 60 * 20, "they never met");
    }
    run(&mut w, 20);
    assert!(order(&mut w, 0, shift(mine, target)));
    let landed = position(&w, mine);
    run(&mut w, 10 * 20);
    assert!(!engaged(&w));
    let drift = (position(&w, mine) - landed).length();
    assert!(
        drift < Fx::from_int(3),
        "it walked {drift:?} back to the fight"
    );
}

/// Phase Shift to an enemy's Resonator, or to a building that isn't a
/// Resonator, is rejected and spends nothing; a shift to your own Resonator
/// still works after.
#[test]
fn phase_shift_refuses_an_enemy_or_non_resonator_target_and_spends_nothing() {
    let mut w = chorded(&[]);
    put_up(&mut w, 0, KEEP, 56, 56);
    put_up(&mut w, 1, RESONATOR, 50, 10);
    let enemy = at(&w, 50, 10).unwrap();
    let siphon = put_up(&mut w, 0, SIPHON, 4, 12);
    let mine = spawn(&mut w, 0, CHOIR_UNIT, 20, 44);
    let start = position(&w, mine);
    align(&mut w);
    run(&mut w, 70 * 60);
    let before = resonance_of(&w, 0);

    let on_enemy = tick_one(&mut w, 0, shift(mine, enemy));
    assert_eq!(on_enemy.commands_rejected, 1, "an enemy's Resonator");
    let on_siphon = tick_one(&mut w, 0, shift(mine, siphon));
    assert_eq!(on_siphon.commands_rejected, 1, "a Siphon");

    assert_eq!(resonance_of(&w, 0), before, "nothing spent");
    assert_eq!(w.power_cooling(0, PHASE_SHIFT), 0, "no cooldown");
    assert_eq!(position(&w, mine), start, "it did not move");

    let own = at(&w, 37, 34).unwrap();
    assert!(
        order(&mut w, 0, shift(mine, own)),
        "your own Resonator works"
    );
    assert_eq!(resonance_of(&w, 0), before - 60);
}

/// The same commands in two worlds give the same state hash on every tick,
/// and a different command gives a different one.
#[test]
fn the_same_commands_give_the_same_state_hash_every_tick() {
    fn go(w: &mut World, hashes: &mut Vec<u64>, commands: &[PlayerCommand]) {
        let stats = w.tick(commands);
        assert_eq!(
            stats.commands_rejected, 0,
            "the script's commands all apply"
        );
        hashes.push(w.state_hash());
    }
    fn build(building: u16, x: i16, y: i16) -> PlayerCommand {
        PlayerCommand {
            player: 0,
            command: Command::SpawnStructure {
                building,
                tile: (x, y),
            },
        }
    }
    fn play(overclock_on: bool) -> Vec<u64> {
        let mut w = conquest(&[seep(34, 25)]);
        let mut hashes = Vec::new();
        for (building, x, y) in [
            (HALL, 4, 4),
            (HOUSING, 12, 56),
            (CHOIR_HALL, 18, 56),
            (RESONATOR, 30, 22),
            (RESONATOR, 44, 22),
            (RESONATOR, 37, 34),
            (KEEP, 56, 56),
        ] {
            go(&mut w, &mut hashes, &[build(building, x, y)]);
        }
        for _ in 0..70 * 60 {
            go(&mut w, &mut hashes, &[]);
        }
        for x in [20, 22] {
            let command = Command::SpawnRegiment {
                unit: CHOIR_UNIT,
                position: FxVec2::from_ints(x, 44),
                facing: NORTH,
            };
            go(&mut w, &mut hashes, &[PlayerCommand { player: 0, command }]);
        }
        let regiments: Vec<RegimentHandle> = w
            .regiments
            .iter()
            .filter(|r| r.owner == 0)
            .map(|r| r.handle)
            .collect();
        let choir = at(&w, 18, 56).unwrap();
        let target = at(&w, 37, 34).unwrap();
        let r1 = at(&w, 30, 22).unwrap();
        if overclock_on {
            let command = overclock(r1);
            go(&mut w, &mut hashes, &[PlayerCommand { player: 0, command }]);
        }
        let command = Command::Train {
            structure: choir,
            unit: CHOIR_UNIT,
        };
        go(&mut w, &mut hashes, &[PlayerCommand { player: 0, command }]);
        // Enough Resonance again for the shift, after Overclock's 20.
        for _ in 0..40 * 60 {
            go(&mut w, &mut hashes, &[]);
        }
        let shifts = regiments.into_iter().map(|r| PlayerCommand {
            player: 0,
            command: shift(r, target),
        });
        let shifts: Vec<PlayerCommand> = shifts.collect();
        go(&mut w, &mut hashes, &shifts);
        for _ in 0..200 {
            go(&mut w, &mut hashes, &[]);
        }
        hashes
    }
    let a = play(true);
    let b = play(true);
    assert_eq!(a, b, "two worlds, the same commands");
    let c = play(false);
    assert_ne!(a.last(), c.last(), "without Overclock, another state");
}

/// Near an enemy Bell Tower that has tolled, a power on a regiment is
/// refused and costs nothing; once the silence is over it works.
#[test]
fn a_bell_tower_silences_powers_used_near_it() {
    let mut w = chorded(&[]);
    put_up(&mut w, 1, BELL, 50, 50);
    let near = spawn(&mut w, 0, CHOIR_UNIT, 51, 58);
    align(&mut w);
    run(&mut w, 25 * 60);
    let haste = |h| Command::UsePower {
        power: OVERCLOCK,
        target: PowerTarget::Regiment(h),
    };
    // The bell tolled as the regiment came, 20 s and more ago, and again
    // since: wait for a toll, then use the power.
    while !w.silenced(0, position(&w, near)) {
        w.tick(&[]);
    }
    let before = resonance_of(&w, 0);
    assert!(before >= 20, "{before} points");
    assert_eq!(tick_one(&mut w, 0, haste(near)).commands_rejected, 1);
    // Nothing spent (a point may have come in meanwhile).
    assert!(resonance_of(&w, 0) >= before);
    assert_eq!(w.regiment_haste(near), 0);
    while w.silenced(0, position(&w, near)) {
        w.tick(&[]);
    }
    assert!(
        order(&mut w, 0, haste(near)),
        "works once the silence is over"
    );
}
