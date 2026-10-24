//! Charges that plough in (docs/05-combat.md): a charge heavy enough for
//! the ranks it hits drives them back in disorder; one that isn't stops on
//! them.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Mass, MatchSetup, PlayerCommand, RegimentHandle, RegimentView, UnitStats,
    World,
};

const SOUTH: Angle = Angle::QUARTER;
const WEST: Angle = Angle::HALF;
/// Drill unit 0: 24 men in 8 files (three ranks), medium.
const FOOT: u16 = 0;
/// Drill unit 1: 24 fast chargers, medium...
const CHARGERS: u16 = 1;
/// ...and the same as heavy horse.
const HEAVY: u16 = 3;

/// The drill's units with morale off, unit 3 heavy chargers.
fn units() -> [UnitStats; 4] {
    let mut units = DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u });
    units[HEAVY as usize] = UnitStats {
        mass: Mass::Heavy,
        ..units[CHARGERS as usize]
    };
    units
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn view(w: &World, h: RegimentHandle) -> RegimentView {
    w.regiments.get(h).unwrap()
}

fn world(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities::default(),
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        // Generals off: their drill units never rout, so any of them could
        // take a general and change what these rules measure.
        generals: false,
    })
}

/// Foot stand facing north at (100, 100); `unit` charges them from
/// `from`, facing `facing`, from far enough off for a run-up. Returns the
/// world and both handles on the tick they meet.
fn charge(unit: u16, from: ((i32, i32), Angle)) -> (World, RegimentHandle, RegimentHandle) {
    let units = units();
    let mut w = world(&units);
    let foot = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    let chargers = charge_at(&mut w, foot, unit, from);
    (w, foot, chargers)
}

/// `unit`, for player 1, charges `target` from `from`; ticks until it hits
/// home and returns it.
fn charge_at(
    w: &mut World,
    target: RegimentHandle,
    unit: u16,
    ((x, y), facing): ((i32, i32), Angle),
) -> RegimentHandle {
    let chargers = spawn(w, 1, unit, x, y, facing);
    assert!(order(
        w,
        1,
        Command::Attack {
            regiment: chargers,
            target,
        },
    ));
    for _ in 0..20 * 20 {
        w.tick(&[]);
        if view(w, chargers).engaged {
            assert!(view(w, chargers).charge > 0, "the charge hits home");
            return chargers;
        }
    }
    panic!("the regiments never met");
}

const AHEAD: ((i32, i32), Angle) = ((100, 60), SOUTH);
const RIGHT: ((i32, i32), Angle) = ((140, 100), WEST);

fn tiles(d: Fx) -> f32 {
    d.raw() as f32 / Fx::ONE.raw() as f32
}

#[test]
fn heavy_horse_ploughs_into_the_ranks_and_drives_them_back() {
    let (mut w, foot, horse) = charge(HEAVY, AHEAD);
    let v = view(&w, foot);
    assert!(
        v.ploughed > 0,
        "heavy horse ploughs into three ranks of foot"
    );
    assert!(v.disorder > 0, "their ranks are thrown");
    assert!(v.giving_ground);
    let (start, horse_start) = (v.position, view(&w, horse).position);
    for _ in 0..v.ploughed {
        w.tick(&[]);
    }
    let (v, h) = (view(&w, foot), view(&w, horse));
    assert_eq!(v.ploughed, 0, "and then they hold again");
    // Driven back south, away from the horse, which follows them in.
    let driven = tiles(v.position.y - start.y);
    assert!(driven > 0.1, "driven back {driven} tiles");
    assert!(
        tiles(h.position.y - horse_start.y) > 0.1,
        "the horse followed"
    );
    assert!(h.engaged && v.engaged);
}

#[test]
fn a_charge_of_equal_weight_stops_on_the_front_ranks() {
    let (w, foot, _) = charge(CHARGERS, AHEAD);
    let v = view(&w, foot);
    assert_eq!(v.ploughed, 0, "three ranks of foot hold medium chargers");
    assert_eq!(v.disorder, 0);
}

#[test]
fn a_flank_is_bowled_over_by_a_charge_of_equal_weight() {
    let (w, foot, _) = charge(CHARGERS, RIGHT);
    let v = view(&w, foot);
    assert!(v.ploughed > 0, "nobody braces a flank");
    // Its flank already takes the extra damage of a flank attack.
    assert_eq!(v.disorder, 0);
}

#[test]
fn a_flank_charge_drives_a_fighting_line_away_from_the_charger() {
    let units = units();
    let mut w = world(&units);
    let foot = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    // Locked with foot of their own weight in front...
    let front = spawn(&mut w, 1, FOOT, 100, 92, SOUTH);
    assert!(order(
        &mut w,
        1,
        Command::Attack {
            regiment: front,
            target: foot,
        },
    ));
    for _ in 0..10 * 20 {
        w.tick(&[]);
    }
    assert!(view(&w, foot).engaged);
    // ...heavy horse hits their right flank.
    let horse = charge_at(&mut w, foot, HEAVY, RIGHT);
    let v = view(&w, foot);
    assert_eq!(v.ploughed_by, Some(horse), "the flank is bowled over");
    let (start, front_at) = (v.position, view(&w, front).position);
    for _ in 0..v.ploughed {
        w.tick(&[]);
    }
    let moved = view(&w, foot).position - start;
    let slid = view(&w, front).position - front_at;
    assert!(tiles(slid.x).abs() < 0.05, "the foe in front slid {slid:?}");
    // Driven west, away from the horse, not back from the foot in front.
    assert!(tiles(moved.x) < -0.3, "moved {moved:?}");
    assert!(tiles(moved.y).abs() < 0.1, "moved {moved:?}");
}
