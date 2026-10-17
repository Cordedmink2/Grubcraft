//! Fog of war in Conquest (docs/20-ambushes.md, "Fog of war and line of
//! sight"): regiments and buildings see so far and no further, buildings
//! stay known once seen, and coming into sight is no ambush.

use gc_fixed::{Fx, FxVec2};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, Role};
use gc_sim::vision::{SIGHT, SIGHT_EVERY};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, MatchSetup, PlayerCommand, RegimentHandle,
    StructureHandle, TICKS_PER_SECOND, UnitStats, Victories, World,
};

const HALL: u16 = 0;
const HOUSE: u16 = 1;
const TILES: i32 = 128;

fn buildings() -> [BuildingStats; 2] {
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 400,
        cost_marrow: 0,
        build_ticks: 100,
        hp: 3000,
        armour: Armour::Fortified,
        width: 3,
        depth: 3,
        supply: 10,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
    };
    let house = BuildingStats {
        role: Role::Housing,
        ..hall
    };
    [hall, house]
}

/// An open map 128 tiles across in Conquest, played for `victories`.
fn conquest(victories: Victories) -> World {
    with_units(victories, &[DRILL_UNITS[0]])
}

/// The same with `units` to field.
fn with_units(victories: Victories, units: &[UnitStats]) -> World {
    let side = TILES as usize * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 4,
        capacities: Capacities { regiments: 16 },
        units,
        map_tiles: TILES as u32,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    });
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        structures: 16,
        victories,
        ..ConquestSetup::default()
    }));
    w
}

/// Played for Annihilation, so in fog, with no Heart to hold.
const ANNIHILATION: Victories = Victories {
    annihilation: true,
    heart: false,
};

fn fogged() -> World {
    conquest(ANNIHILATION)
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32) -> RegimentHandle {
    spawn_as(w, player, 0, x, y)
}

fn spawn_as(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        },
    ));
    (w.regiments.iter())
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile: (x, y),
        },
    ));
    (w.structures().unwrap().iter())
        .find(|v| v.tile == (i32::from(x), i32::from(y)))
        .unwrap()
        .handle
}

fn march(w: &mut World, player: u8, regiment: RegimentHandle, x: i32, y: i32) {
    assert!(order(
        w,
        player,
        Command::MoveTo {
            regiment,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    ));
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

fn seen_by(w: &World, h: StructureHandle, party: u8) -> bool {
    w.structures().unwrap().get(h).unwrap().seen_by(party)
}

#[test]
fn a_regiment_sees_so_far_and_no_further() {
    let mut w = fogged();
    assert!(w.fog());
    let ours = spawn(&mut w, 0, 20, 64);
    let theirs = spawn(&mut w, 1, 110, 64);
    run(&mut w, 2);
    assert!(!w.sees(0, theirs) && !w.sees(1, ours));
    assert!(w.sees(0, ours), "a side sees its own");
    march(&mut w, 1, theirs, 45, 64);
    until(&mut w, 60 * TICKS_PER_SECOND, |w| w.sees(0, theirs));
    // Each regiment is looked round on its own tick, a few apart.
    run(&mut w, SIGHT_EVERY);
    assert!(w.sees(1, ours), "sight works both ways");
    // Out of sight again: unseen once the sighting lingers out.
    march(&mut w, 1, theirs, 120, 64);
    until(&mut w, 60 * TICKS_PER_SECOND, |w| !w.sees(0, theirs));
}

#[test]
fn a_regiment_placed_in_sight_is_seen_at_once() {
    // Each regiment is looked round on its own tick, but a new one on
    // the tick it is placed (placing takes a tick here), so it is never
    // unseen by a side with it in sight.
    let mut w = fogged();
    let ours = spawn(&mut w, 0, 20, 64);
    let theirs = spawn(&mut w, 1, 30, 64);
    assert!(w.sees(0, theirs));
    run(&mut w, SIGHT_EVERY);
    assert!(w.sees(1, ours));
}

#[test]
fn the_sandbox_sees_everything() {
    let mut w = conquest(Victories::default());
    assert!(!w.fog());
    spawn(&mut w, 0, 20, 64);
    let theirs = spawn(&mut w, 1, 110, 64);
    run(&mut w, 2);
    assert!(w.sees(0, theirs));
    let hall = put_up(&mut w, 1, HALL, 100, 100);
    assert!(seen_by(&w, hall, 0));
}

#[test]
fn a_building_keeps_watch_round_itself() {
    let mut w = fogged();
    put_up(&mut w, 0, HOUSE, 20, 20);
    let near = spawn(&mut w, 1, 21, 36);
    let far = spawn(&mut w, 1, 21, 80);
    run(&mut w, 2);
    assert!(w.sees(0, near));
    assert!(!w.sees(0, far));
}

#[test]
fn an_enemy_building_is_known_once_seen() {
    let mut w = fogged();
    let scout = spawn(&mut w, 0, 20, 20);
    let hall = put_up(&mut w, 1, HALL, 100, 100);
    run(&mut w, 2);
    assert!(!seen_by(&w, hall, 0));
    assert!(seen_by(&w, hall, 1));
    // An attack order needs a building its side has seen.
    let attack = Command::AttackStructure {
        regiment: scout,
        structure: hall,
    };
    assert!(!order(&mut w, 0, attack));
    march(&mut w, 0, scout, 80, 100);
    until(&mut w, 60 * TICKS_PER_SECOND, |w| seen_by(w, hall, 0));
    march(&mut w, 0, scout, 10, 10);
    run(&mut w, 30 * TICKS_PER_SECOND);
    assert!(seen_by(&w, hall, 0), "it stays known");
    assert!(order(&mut w, 0, attack));
}

#[test]
fn coming_into_sight_is_no_ambush() {
    let mut w = fogged();
    let ours = spawn(&mut w, 0, 20, 64);
    let theirs = spawn(&mut w, 1, 110, 64);
    march(&mut w, 1, theirs, 40, 64);
    until(&mut w, 60 * TICKS_PER_SECOND, |w| w.sees(1, ours));
    assert!(order(
        &mut w,
        1,
        Command::Attack {
            regiment: theirs,
            target: ours,
        },
    ));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        assert!(w.ambushes().is_empty());
        if w.regiments.get(theirs).is_some_and(|r| r.engaged) {
            return;
        }
    }
    panic!("never met");
}

#[test]
fn sight_is_drawn_as_circles_round_a_side() {
    let mut w = fogged();
    spawn(&mut w, 0, 20, 64);
    put_up(&mut w, 0, HOUSE, 30, 30);
    spawn(&mut w, 1, 110, 64);
    let mut circles = Vec::new();
    w.sight(0, &mut circles);
    assert_eq!(circles.len(), 2);
    assert!(circles.iter().all(|&(_, r)| r > Role::Housing.sight()));
    assert!(circles[0].1 > SIGHT);
}

#[test]
fn archers_shoot_as_far_as_their_side_sees() {
    // Archers that carry further than sight, shooting at what a friend
    // sees: out of their target's sight is not hidden, so they loose.
    let long = UnitStats {
        range: Fx::from_int(60),
        ..DRILL_UNITS[2]
    };
    let mut w = with_units(ANNIHILATION, &[long, DRILL_UNITS[0]]);
    let archers = spawn(&mut w, 0, 20, 64);
    spawn_as(&mut w, 0, 1, 75, 40);
    let theirs = spawn_as(&mut w, 1, 1, 75, 64);
    run(&mut w, 2);
    assert!(w.sees(0, theirs), "a friend sees them");
    assert!(!w.sees(1, archers), "they can't see the archers");
    let before = w.regiments.get(theirs).unwrap().models;
    until(&mut w, 30 * TICKS_PER_SECOND, |w| {
        w.regiments.get(theirs).is_none_or(|r| r.models < before)
    });
}
