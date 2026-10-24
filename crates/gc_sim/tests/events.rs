//! The simulation tells what happened each tick (`gc_sim::events`), and what
//! it tells agrees with the state it leaves.

use std::collections::HashMap;

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::{Event, VolleyId};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Ground, MatchSetup, MoraleState, PlayerCommand, RegimentHandle, World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const CAVALRY: u16 = 1;
const ARCHERS: u16 = 2;

/// Archers and spears in the north, spears and horse in the south, every
/// regiment ordered at the nearest enemy.
fn battle() -> World {
    let mut world = World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 64,
        nav: &[],
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let spawn = |player, unit, x, y, facing| PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    };
    let mut spawns = Vec::new();
    for k in 0..3 {
        spawns.push(spawn(0, ARCHERS, 18 + 12 * k, 14, SOUTH));
        spawns.push(spawn(0, SPEARS, 18 + 12 * k, 22, SOUTH));
        spawns.push(spawn(1, SPEARS, 18 + 12 * k, 42, NORTH));
    }
    spawns.push(spawn(1, CAVALRY, 12, 48, NORTH));
    spawns.push(spawn(1, CAVALRY, 52, 48, NORTH));
    world.tick(&spawns);
    let regs: Vec<_> = world
        .regiments
        .iter()
        .map(|r| (r.handle, r.owner, r.position))
        .collect();
    let orders: Vec<_> = regs
        .iter()
        .filter_map(|&(regiment, owner, at)| {
            let (target, ..) = regs
                .iter()
                .filter(|r| r.1 != owner)
                .min_by_key(|r| (r.2 - at).length_sq_raw())?;
            Some(PlayerCommand {
                player: owner,
                command: Command::Attack {
                    regiment,
                    target: *target,
                },
            })
        })
        .collect();
    world.tick(&orders);
    world
}

#[test]
fn every_volley_is_told_loosed_then_landed_and_losses_add_up() {
    let mut world = battle();
    let mut in_flight: HashMap<VolleyId, gc_sim::Shooter> = HashMap::new();
    let (mut loosed, mut landed, mut contacts, mut wavered) = (0, 0, 0, 0);
    let mut arrows_killed = 0;
    for _ in 0..2400 {
        let before: HashMap<RegimentHandle, u16> = world
            .regiments
            .iter()
            .map(|r| (r.handle, r.models))
            .collect();
        world.tick(&[]);
        let mut lost: HashMap<RegimentHandle, u16> = HashMap::new();
        let mut shot: HashMap<RegimentHandle, u16> = HashMap::new();
        for e in world.events() {
            match *e {
                Event::Loosed {
                    volley, shooter, ..
                } => {
                    loosed += 1;
                    assert!(in_flight.insert(volley, shooter).is_none());
                    // The volley in the air says who loosed it.
                    let seen = world.volleys().find(|v| v.id == volley).unwrap();
                    assert_eq!(seen.shooter, shooter);
                }
                Event::Landed {
                    volley,
                    target,
                    lost: n,
                    ..
                } => {
                    landed += 1;
                    *shot.entry(target).or_default() += n;
                    assert!(in_flight.remove(&volley).is_some(), "landed unloosed");
                }
                Event::Losses { regiment, lost: n } => {
                    assert!(lost.insert(regiment, n).is_none(), "told twice");
                }
                Event::Morale { from, to, .. } => {
                    assert_ne!(from, to);
                    if from == MoraleState::Steady {
                        wavered += 1;
                    }
                }
                Event::Contact { regiment, foe } => {
                    contacts += 1;
                    assert!(world.regiments.get(regiment).unwrap().engaged);
                    assert!(world.regiments.owner(foe).is_some());
                }
                Event::Ambush(_) => {}
                Event::Fallen { .. }
                | Event::Struck { .. }
                | Event::Consumed { .. }
                | Event::Tolled { .. }
                | Event::Cleared { .. } => {
                    unreachable!("no buildings or workers in a battle")
                }
            }
        }
        // Soldiers shot down are among the losses told.
        for (h, n) in shot {
            assert!(n <= lost.get(&h).copied().unwrap_or(0), "{h:?}");
            arrows_killed += u32::from(n);
        }
        // Losses told are exactly the soldiers gone.
        for (h, was) in before {
            let now = world.regiments.get(h).map_or(0, |r| r.models);
            assert_eq!(lost.get(&h).copied().unwrap_or(0), was - now, "{h:?}");
        }
    }
    assert!(
        loosed > 10 && landed > 10 && arrows_killed > 0,
        "{loosed} loosed, {landed} landed, {arrows_killed} shot down"
    );
    assert!(
        contacts > 0 && wavered > 0,
        "{contacts} contacts, {wavered} wavered"
    );
    // Whatever is still told in flight is in the air.
    for id in in_flight.keys() {
        assert!(world.volleys().any(|v| v.id == *id));
    }
}

#[test]
fn a_rehearsal_tells_nothing() {
    let mut world = battle();
    let mut told = 0;
    for _ in 0..600 {
        world.tick(&[]);
        told += world.events().len();
    }
    assert!(told > 0);
    let mut copy = world.fork_region(FxVec2::from_ints(32, 32), Fx::from_int(100));
    assert_eq!(copy.regiments.len(), world.regiments.len());
    for _ in 0..300 {
        copy.tick(&[]);
        assert!(copy.events().is_empty());
    }
}
