//! Routes around blocked ground.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, MoraleState, PlayerCommand, RegimentHandle, UnitStats, World,
};

const SOUTH: Angle = Angle(0x4000);
const SPEARS: u16 = 0;
const SKITTERS: u16 = 1;

/// A map `tiles` across, open but for the cells `blocked` says.
fn map(tiles: i32, blocked: impl Fn(i32, i32) -> bool) -> Vec<u8> {
    let side = tiles * CELLS_PER_TILE;
    let mut cells = Vec::new();
    for y in 0..side {
        for x in 0..side {
            // Cell (x, y) covers tiles x / 2 .. (x + 1) / 2.
            cells.push(if blocked(x, y) { BLOCKED } else { 1 });
        }
    }
    cells
}

fn world_on(tiles: u32, nav: &[u8], units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities { regiments: 256 },
        units,
        map_tiles: tiles,
        nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
    })
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn move_to(w: &mut World, player: u8, regiment: RegimentHandle, x: i32, y: i32) {
    let stats = w.tick(&[PlayerCommand {
        player,
        command: Command::MoveTo {
            regiment,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
}

/// Whether the cell under `p` is blocked on `nav` (a map `tiles` across).
fn on_blocked(nav: &[u8], tiles: i32, p: FxVec2) -> bool {
    let side = tiles * CELLS_PER_TILE;
    let cell = |v: Fx| {
        ((i64::from(v.raw()) * i64::from(CELLS_PER_TILE)) >> 16).clamp(0, i64::from(side) - 1)
            as i32
    };
    nav[(cell(p.y) * side + cell(p.x)) as usize] == BLOCKED
}

/// Tick until the regiment stops moving; asserts it never steps onto
/// blocked ground. Returns the ticks taken.
fn walk(w: &mut World, nav: &[u8], tiles: i32, h: RegimentHandle, max_ticks: u32) -> u32 {
    for t in 0..max_ticks {
        let r = w.regiments.get(h).unwrap();
        assert!(
            !on_blocked(nav, tiles, r.position),
            "on blocked ground at {:?}",
            r.position
        );
        if !r.moving && t > 0 {
            return t;
        }
        w.tick(&[]);
    }
    panic!(
        "still walking after {max_ticks} ticks: {:?}",
        w.regiments.get(h)
    );
}

fn distance(a: FxVec2, b: FxVec2) -> Fx {
    (a - b).length()
}

/// A wall across the middle of a 64-tile map (tiles 30 to 33 thick), with
/// one gap near the east edge, tiles 52 to 58.
fn wall_with_gap(x: i32, y: i32) -> bool {
    (60..68).contains(&y) && !(104..116).contains(&x)
}

#[test]
fn regiments_walk_around_walls() {
    let nav = map(64, wall_with_gap);
    let mut w = world_on(64, &nav, &DRILL_UNITS);
    let r = spawn(&mut w, 0, SPEARS, 16, 50, NORTH);
    move_to(&mut w, 0, r, 16, 14);
    // The route goes out through the gap, not straight through the wall.
    let route = w.route(r).to_vec();
    assert!(route.len() >= 2, "{route:?}");
    assert!(route.iter().any(|p| p.x > Fx::from_int(45)), "{route:?}");
    walk(&mut w, &nav, 64, r, 20 * 120);
    let end = w.regiments.get(r).unwrap().position;
    assert!(
        distance(end, FxVec2::from_ints(16, 14)) < Fx::ONE,
        "{end:?}"
    );
}

#[test]
fn routes_stay_on_open_ground_between_waypoints() {
    let nav = gc_sim::scenario::ribs_map(128);
    let mut w = world_on(128, &nav, &DRILL_UNITS);
    let mut checked = 0;
    for (k, &(from, to)) in [
        ((10, 10), (118, 118)),
        ((118, 10), (10, 118)),
        ((20, 100), (100, 20)),
        ((64, 10), (64, 118)),
        ((10, 64), (118, 64)),
    ]
    .iter()
    .enumerate()
    {
        let r = spawn(&mut w, (k % 4) as u8, SPEARS, from.0, from.1, NORTH);
        move_to(&mut w, (k % 4) as u8, r, to.0, to.1);
        while w.route(r).is_empty() {
            w.tick(&[]);
        }
        let mut prev = w.regiments.get(r).unwrap().position;
        for &p in w.route(r) {
            // Sample the leg every quarter tile.
            let d = p - prev;
            let steps = (d.length().raw() >> 14).max(1);
            for s in 0..=steps {
                let q = prev + FxVec2::new(d.x * s / steps, d.y * s / steps);
                assert!(
                    !on_blocked(&nav, 128, q),
                    "leg {prev:?} -> {p:?} crosses blocked ground at {q:?}"
                );
            }
            prev = p;
            checked += 1;
        }
    }
    assert!(checked >= 5);
}

#[test]
fn blocked_goals_take_the_nearest_open_ground() {
    // A walled-in square in the middle: nobody can get inside.
    let nav = map(64, |x, y| {
        let ring = |v: i32| (50..78).contains(&v);
        ring(x) && ring(y) && !((54..74).contains(&x) && (54..74).contains(&y))
    });
    let mut w = world_on(64, &nav, &DRILL_UNITS);
    let r = spawn(&mut w, 0, SPEARS, 10, 32, NORTH);
    move_to(&mut w, 0, r, 32, 32);
    walk(&mut w, &nav, 64, r, 20 * 60);
    let end = w.regiments.get(r).unwrap().position;
    // It stops just outside the wall (7 tiles from the centre), as near the
    // centre as it can get.
    let d = distance(end, FxVec2::from_ints(32, 32));
    assert!(d > Fx::from_int(7) && d < Fx::from_int(10), "{end:?}");
}

#[test]
fn long_winding_routes_arrive() {
    // A serpentine: walls across the map with gaps at alternating ends.
    let nav = map(64, |x, y| {
        let band = y / 16;
        (12..16).contains(&(y % 16)) && if band % 2 == 0 { x < 112 } else { x >= 16 }
    });
    let mut w = world_on(64, &nav, &DRILL_UNITS);
    let r = spawn(&mut w, 0, SKITTERS, 8, 2, SOUTH);
    move_to(&mut w, 0, r, 8, 57);
    let ticks = walk(&mut w, &nav, 64, r, 20 * 400);
    let end = w.regiments.get(r).unwrap().position;
    assert!(
        distance(end, FxVec2::from_ints(8, 57)) < Fx::ONE,
        "{end:?} after {ticks} ticks"
    );
    // Seven walls to wind through: more waypoints than one route holds.
    assert!(ticks > 20 * 100, "{ticks}");
}

#[test]
fn regiments_on_blocked_ground_walk_off_it() {
    let nav = map(64, wall_with_gap);
    let mut w = world_on(64, &nav, &DRILL_UNITS);
    let r = spawn(&mut w, 0, SPEARS, 20, 32, NORTH); // inside the wall
    assert!(!w.nav().walkable(FxVec2::from_ints(20, 32)));
    move_to(&mut w, 0, r, 20, 10);
    for _ in 0..20 * 120 {
        w.tick(&[]);
    }
    let end = w.regiments.get(r).unwrap().position;
    assert!(
        distance(end, FxVec2::from_ints(20, 10)) < Fx::ONE,
        "{end:?}"
    );
}

#[test]
fn many_orders_at_once_are_spread_over_ticks() {
    let nav = gc_sim::scenario::ribs_map(128);
    let mut w = world_on(128, &nav, &DRILL_UNITS);
    let mut regs = Vec::new();
    for k in 0..40 {
        regs.push(spawn(
            &mut w,
            0,
            SPEARS,
            8 + (k % 10) * 6,
            8 + (k / 10) * 5,
            SOUTH,
        ));
    }
    let orders: Vec<_> = regs
        .iter()
        .enumerate()
        .map(|(k, &regiment)| PlayerCommand {
            player: 0,
            command: Command::MoveTo {
                regiment,
                target: FxVec2::from_ints(70 + (k as i32 % 10) * 5, 110 + (k as i32 / 10) * 4),
                facing: None,
                files: 0,
                pace: Fx::ZERO,
            },
        })
        .collect();
    w.tick(&orders);
    let waiting = w.nav().queued();
    assert!(
        waiting > 0 && waiting < 40,
        "{waiting} still waiting after one tick"
    );
    let mut ticks = 1;
    while w.nav().queued() > 0 {
        w.tick(&[]);
        ticks += 1;
        assert!(ticks < 200);
    }
    for &r in &regs {
        assert!(!w.route(r).is_empty() || !w.regiments.get(r).unwrap().moving);
    }
}

#[test]
fn attackers_find_their_way_to_the_enemy() {
    let nav = map(64, wall_with_gap);
    let mut w = world_on(64, &nav, &DRILL_UNITS);
    let a = spawn(&mut w, 0, SPEARS, 20, 50, NORTH);
    let b = spawn(&mut w, 1, SPEARS, 20, 14, SOUTH);
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Attack {
            regiment: a,
            target: b,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    for _ in 0..20 * 120 {
        let r = w.regiments.get(a).unwrap();
        assert!(!on_blocked(&nav, 64, r.position));
        if r.engaged {
            return;
        }
        w.tick(&[]);
    }
    panic!("never reached the enemy: {:?}", w.regiments.get(a));
}

#[test]
fn routers_keep_off_blocked_ground() {
    // A wall just behind the skitterlings' line.
    let nav = map(64, |_, y| (52..56).contains(&y));
    let mut w = world_on(64, &nav, &DRILL_UNITS);
    let skitters = spawn(&mut w, 1, SKITTERS, 32, 22, NORTH);
    spawn(&mut w, 0, SPEARS, 32, 19, SOUTH);
    let mut routed = false;
    for _ in 0..20 * 200 {
        w.tick(&[]);
        let Some(r) = w.regiments.get(skitters) else {
            break;
        };
        routed |= r.morale == MoraleState::Routing;
        assert!(!on_blocked(&nav, 64, r.position), "{:?}", r.position);
    }
    assert!(routed);
}

#[test]
fn same_map_same_hashes() {
    let run = || {
        let nav = gc_sim::scenario::ribs_map(128);
        let mut w = world_on(128, &nav, &DRILL_UNITS);
        let r = spawn(&mut w, 0, SPEARS, 10, 10, NORTH);
        move_to(&mut w, 0, r, 118, 118);
        for _ in 0..200 {
            w.tick(&[]);
        }
        w.state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn routes_across_open_country_keep_their_line_through_portals() {
    // A rock just short of the goal hides it, so the route goes through the
    // portal graph; on the open ground before it, the block should keep to
    // its own line rather than funnel to wherever the portals sit (their
    // middles are 8 tiles apart, so a neighbour could be marched into).
    // Once marching south (across edges between sectors above and below)
    // and once east (across edges between sectors side by side).
    for east in [false, true] {
        let flip = |x: i32, y: i32| if east { (y, x) } else { (x, y) };
        let rock = |x: i32, y: i32| {
            let (x, y) = flip(x, y);
            (106..114).contains(&x) && (196..202).contains(&y)
        };
        let nav = map(128, rock);
        let mut w = world_on(128, &nav, &DRILL_UNITS);
        let (sx, sy) = flip(56, 20);
        let r = spawn(&mut w, 0, SPEARS, sx, sy, SOUTH);
        let (gx, gy) = flip(56, 106);
        move_to(&mut w, 0, r, gx, gy);
        let route = w.route(r).to_vec();
        // (How far along the march, how far across it.)
        let along = |p: &FxVec2| if east { (p.x, p.y) } else { (p.y, p.x) };
        let open_country: Vec<_> = route
            .iter()
            .filter(|p| along(p).0 < Fx::from_int(94))
            .collect();
        assert!(open_country.len() >= 2, "east {east}: {route:?}");
        for p in open_country {
            assert!(
                (along(p).1 - Fx::from_int(56)).abs() <= Fx::from_ratio(3, 2),
                "east {east}: waypoint off the line: {route:?}"
            );
        }
        walk(&mut w, &nav, 128, r, 20 * 200);
    }
}
