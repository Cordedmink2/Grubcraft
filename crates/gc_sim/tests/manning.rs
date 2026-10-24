//! Manning the walls (docs/26-fortifications.md, "Manning the walls"): a
//! regiment climbs onto its side's wall of level 2 or 3, holds the walkway
//! out of reach of melee from the ground, shoots further from it and is
//! covered by it, and comes down when ordered off or when the wall falls
//! under it.

use gc_fixed::Angle;
use gc_fixed::{Fx, FxVec2};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, LINE, Refusal, Role, TowerAttack};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Cover, Ground, Growth,
    MatchSetup, PlayerCommand, RegimentHandle, RegimentView, StructureHandle, TICKS_PER_SECOND,
    UnitStats, World,
};

const WORKER: u16 = 0;
const FOOT: u16 = 1;
const ARCHERS: u16 = 2;
const PALISADE: u16 = 0;
const HALL: u16 = 1;
const STONE: u16 = 2;
const TOWER: u16 = 3;
const MAP: u32 = 64;
/// The wall's column: [`STONE`] tiles from y = 10 to 39, facing east.
const WALL_X: i32 = 20;

/// Open ground; each side has a hall, and side 0 has a stone wall down
/// x = [`WALL_X`] from y = 10 to 39 and a palisade tile at (10, 50).
fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let n = MAP as usize;
    let cover = vec![Cover::Open as u8; n * n];
    let growth = vec![Growth::Open as u8; n * n];
    let worker = UnitStats {
        models: 1,
        files: 1,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 16 },
        units: &[worker, DRILL_UNITS[0], DRILL_UNITS[2]],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &growth,
            footing: &[],
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    });
    let palisade = BuildingStats {
        role: Role::Wall,
        tier: 1,
        upgrades_from: None,
        cost_grub: 10,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 250,
        armour: Armour::Fortified,
        width: 1,
        depth: 1,
        supply: 0,
        flags: LINE,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: gc_sim::structures::WallTwist::None,
    };
    let soldier = ConquestUnit {
        mans_walls: true,
        ..ConquestUnit::default()
    };
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            palisade,
            BuildingStats {
                role: Role::Main,
                width: 3,
                depth: 3,
                flags: 0,
                ..palisade
            },
            BuildingStats {
                upgrades_from: Some(PALISADE),
                hp: 625,
                ..palisade
            },
            BuildingStats {
                role: Role::Defence,
                width: 2,
                depth: 2,
                flags: 0,
                attack: Some(TowerAttack {
                    range: Fx::from_int(1),
                    damage: 1,
                    reload_ticks: 20,
                    shots: 1,
                }),
                ..palisade
            },
        ],
        units: &[
            ConquestUnit {
                worker: true,
                ..ConquestUnit::default()
            },
            soldier,
            soldier,
        ],
        structures: 1024,
        ..ConquestSetup::default()
    }));
    for (player, tile) in [(0, (2, 2)), (1, (58, 58))] {
        spawn_structure(&mut w, player, HALL, tile);
    }
    for y in 10..40 {
        spawn_structure(&mut w, 0, STONE, (WALL_X as i16, y));
    }
    spawn_structure(&mut w, 0, PALISADE, (10, 50));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn_structure(w: &mut World, player: u8, building: u16, tile: (i16, i16)) {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile,
            turn: 0,
        }
    ));
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: if player == 0 { NORTH } else { Angle(0x4000) },
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn tile(w: &World, at: (i32, i32)) -> StructureHandle {
    let s = w.structures().unwrap();
    s.iter().find(|v| v.tile == at).unwrap().handle
}

/// Hit points left in regiment `h`, every soldier together.
fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().map(|&v| u32::from(v)).sum())
}

fn view(w: &World, h: RegimentHandle) -> Option<RegimentView> {
    w.regiments.iter().find(|r| r.handle == h)
}

fn man(w: &mut World, player: u8, r: RegimentHandle, at: (i32, i32)) -> bool {
    let structure = tile(w, at);
    order(
        w,
        player,
        Command::ManWall {
            regiment: r,
            structure,
        },
    )
}

/// Tick until `r` is on the wall, failing after a minute.
fn climb(w: &mut World, r: RegimentHandle) {
    for _ in 0..60 * TICKS_PER_SECOND {
        if view(w, r).unwrap().on_wall {
            return;
        }
        w.tick(&[]);
    }
    panic!("it never got onto the wall");
}

fn move_to(w: &mut World, player: u8, r: RegimentHandle, x: i32, y: i32) -> bool {
    order(
        w,
        player,
        Command::MoveTo {
            regiment: r,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    )
}

#[test]
fn only_foot_on_a_walkway_of_their_own_may_man_it() {
    let mut w = world();
    let foot = spawn(&mut w, 0, FOOT, 14, 20);
    let worker = spawn(&mut w, 0, WORKER, 14, 24);
    let theirs = spawn(&mut w, 1, FOOT, 30, 20);
    let stone = tile(&w, (WALL_X, 20));
    let palisade = tile(&w, (10, 50));
    assert_eq!(w.man_check(foot, palisade), Err(Refusal::NoWalkway));
    assert_eq!(w.man_check(worker, stone), Err(Refusal::Unknown));
    assert_eq!(w.man_check(theirs, stone), Err(Refusal::Unknown));
    assert_eq!(w.man_check(foot, stone), Ok(()));
    assert!(!man(&mut w, 0, worker, (WALL_X, 20)));
    assert!(!man(&mut w, 1, theirs, (WALL_X, 20)));
    assert!(man(&mut w, 0, foot, (WALL_X, 20)));
}

#[test]
fn a_regiment_climbs_and_holds_a_stretch_of_walkway_one_man_a_tile() {
    let mut w = world();
    let a = spawn(&mut w, 0, FOOT, 14, 24);
    let b = spawn(&mut w, 0, FOOT, 14, 34);
    assert!(man(&mut w, 0, a, (WALL_X, 24)));
    climb(&mut w, a);
    let held: Vec<_> = w.wall_tiles(a).to_vec();
    // Twenty-four soldiers, a tile each, starting where it was sent.
    assert_eq!(held.len(), 24);
    assert_eq!(held[0], tile(&w, (WALL_X, 24)));
    let p = view(&w, a).unwrap().position;
    assert!((p.x - Fx::from_ratio(41, 2)).abs() < Fx::ONE, "{p:?}");
    // The second takes the six tiles left, none of the first's.
    assert!(man(&mut w, 0, b, (WALL_X, 34)));
    climb(&mut w, b);
    let more = w.wall_tiles(b).to_vec();
    assert_eq!(more.len(), 6);
    assert!(more.iter().all(|t| !held.contains(t)));
    // Up there the wall holds them: neither drifts.
    for _ in 0..5 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert_eq!(view(&w, a).unwrap().position, p);
}

#[test]
fn melee_from_the_ground_cant_reach_a_regiment_on_the_wall() {
    let mut w = world();
    let ours = spawn(&mut w, 0, FOOT, 14, 24);
    assert!(man(&mut w, 0, ours, (WALL_X, 24)));
    climb(&mut w, ours);
    let models = view(&w, ours).unwrap().models;
    let theirs = spawn(&mut w, 1, FOOT, 24, 24);
    assert!(order(
        &mut w,
        1,
        Command::Attack {
            regiment: theirs,
            target: ours,
        }
    ));
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
        assert!(!view(&w, ours).unwrap().engaged);
        assert!(!view(&w, theirs).unwrap().engaged);
    }
    assert_eq!(view(&w, ours).unwrap().models, models);
}

/// How many of an enemy regiment of foot standing `gap` tiles east of
/// side 0's archers lose in `seconds`, the archers on the wall or not.
fn shot_at_from(on_wall: bool, gap: i32, seconds: u32) -> u16 {
    let mut w = world();
    let archers = spawn(&mut w, 0, ARCHERS, 16, 24);
    if on_wall {
        assert!(man(&mut w, 0, archers, (WALL_X, 24)));
        climb(&mut w, archers);
    } else {
        assert!(move_to(&mut w, 0, archers, WALL_X - 2, 24));
        for _ in 0..20 * TICKS_PER_SECOND {
            w.tick(&[]);
        }
    }
    let x = view(&w, archers).unwrap().position.x;
    let x = (x + Fx::from_int(gap)).floor_int();
    let theirs = spawn(&mut w, 1, FOOT, x, 24);
    let before = view(&w, theirs).unwrap().models;
    for _ in 0..seconds * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    before - view(&w, theirs).map_or(0, |v| v.models)
}

#[test]
fn archers_on_a_wall_shoot_further() {
    // Range 25 on the ground; 30 up on the wall.
    assert_eq!(shot_at_from(false, 28, 20), 0);
    assert!(shot_at_from(true, 28, 20) > 0);
}

/// How many hit points side 0's foot, on the wall or by it, lose in
/// `seconds` to enemy archers 15 tiles east.
fn shot_from(on_wall: bool, seconds: u32) -> u32 {
    let mut w = world();
    let ours = spawn(&mut w, 0, FOOT, 16, 24);
    if on_wall {
        assert!(man(&mut w, 0, ours, (WALL_X, 24)));
        climb(&mut w, ours);
    }
    let theirs = spawn(&mut w, 1, ARCHERS, 36, 24);
    let before = hp(&w, ours);
    for _ in 0..seconds * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert!(view(&w, theirs).is_some());
    before - hp(&w, ours)
}

#[test]
fn the_wall_covers_those_on_it_from_arrows() {
    let (up, down) = (shot_from(true, 30), shot_from(false, 30));
    assert!(down > 0);
    assert!(up * 4 < down * 3, "on the wall {up}, on the ground {down}");
}

#[test]
fn any_other_order_brings_it_down_at_its_foot() {
    let mut w = world();
    let ours = spawn(&mut w, 0, FOOT, 14, 24);
    assert!(man(&mut w, 0, ours, (WALL_X, 24)));
    climb(&mut w, ours);
    assert!(move_to(&mut w, 0, ours, 10, 24));
    let v = view(&w, ours).unwrap();
    assert!(!v.on_wall);
    assert!(w.wall_tiles(ours).is_empty());
    // Down on the side it climbed from, and walking away.
    assert!(v.position.x < Fx::from_int(WALL_X), "{:?}", v.position);
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let p = view(&w, ours).unwrap().position;
    assert!(
        (p - FxVec2::from_ints(10, 24)).length() < Fx::from_int(2),
        "{p:?}"
    );
}

#[test]
fn when_a_tile_it_holds_falls_it_comes_down_into_the_breach() {
    let mut w = world();
    let ours = spawn(&mut w, 0, FOOT, 14, 24);
    assert!(man(&mut w, 0, ours, (WALL_X, 24)));
    climb(&mut w, ours);
    let before = hp(&w, ours);
    let models = view(&w, ours).unwrap().models;
    let gone = tile(&w, (WALL_X, 26));
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: gone }
    ));
    let v = view(&w, ours).unwrap();
    assert!(!v.on_wall);
    assert!(v.disorder > 0);
    assert!(hp(&w, ours) < before);
    // Hurt by the fall, not killed.
    assert_eq!(v.models, models);
    // Down in the gap.
    assert!(
        (v.position.y - Fx::from_int(26)).abs() < Fx::from_int(3),
        "{:?}",
        v.position
    );
}

/// Ticks side 0's foot, standing at (14, 23), takes to get onto the wall
/// sent to tile (WALL_X, 30), with or without a tower joined to the wall
/// beside it.
fn time_to_climb(tower: bool) -> u32 {
    let mut w = world();
    if tower {
        spawn_structure(&mut w, 0, TOWER, ((WALL_X - 2) as i16, 22));
    }
    let ours = spawn(&mut w, 0, FOOT, 14, 23);
    assert!(man(&mut w, 0, ours, (WALL_X, 30)));
    for t in 0..60 * TICKS_PER_SECOND {
        if view(&w, ours).unwrap().on_wall {
            return t;
        }
        w.tick(&[]);
    }
    panic!("it never got onto the wall");
}

#[test]
fn a_tower_joined_to_the_wall_is_its_stair() {
    // Without the tower it walks to an end of the wall (13 tiles off);
    // with it, it climbs at the tower beside it.
    let (by_tower, by_end) = (time_to_climb(true), time_to_climb(false));
    assert!(
        by_tower + TICKS_PER_SECOND < by_end,
        "by the tower {by_tower} ticks, at the end {by_end}"
    );
}

#[test]
fn up_there_it_faces_out_away_from_where_it_was_sent_from() {
    let mut w = world();
    // Climbing at the tower on the wall's own line still faces it east.
    spawn_structure(&mut w, 0, TOWER, ((WALL_X - 2) as i16, 22));
    let ours = spawn(&mut w, 0, FOOT, 14, 23);
    assert!(man(&mut w, 0, ours, (WALL_X, 30)));
    climb(&mut w, ours);
    let east = Angle::of_vec(FxVec2::from_ints(1, 0));
    assert_eq!(view(&w, ours).unwrap().facing, east);
}

#[test]
fn it_comes_down_in_the_shape_it_climbed_in() {
    let mut w = world();
    let ours = spawn(&mut w, 0, FOOT, 14, 24);
    let frontage = view(&w, ours).unwrap().frontage;
    assert!(man(&mut w, 0, ours, (WALL_X, 24)));
    climb(&mut w, ours);
    assert_ne!(view(&w, ours).unwrap().frontage, frontage);
    assert!(move_to(&mut w, 0, ours, 10, 24));
    assert_eq!(view(&w, ours).unwrap().frontage, frontage);
}

/// Whether side 0's regiment of `unit` on the wall is still up there
/// after being sent to attack an enemy regiment on the ground east of it.
fn stays_up_to_attack(unit: u16) -> bool {
    let mut w = world();
    let ours = spawn(&mut w, 0, unit, 14, 24);
    assert!(man(&mut w, 0, ours, (WALL_X, 24)));
    climb(&mut w, ours);
    let theirs = spawn(&mut w, 1, FOOT, 30, 24);
    assert!(order(
        &mut w,
        0,
        Command::Attack {
            regiment: ours,
            target: theirs,
        }
    ));
    w.tick(&[]);
    view(&w, ours).unwrap().on_wall
}

#[test]
fn shooters_attack_from_the_wall_and_melee_comes_down_to() {
    assert!(stays_up_to_attack(ARCHERS));
    assert!(!stays_up_to_attack(FOOT));
}

fn climb_order(w: &mut World, player: u8, r: RegimentHandle, at: (i32, i32)) -> bool {
    let structure = tile(w, at);
    order(
        w,
        player,
        Command::Climb {
            regiment: r,
            structure,
        },
    )
}

#[test]
fn only_enemy_foot_may_climb_a_walkway_on_ladders() {
    let mut w = world();
    let ours = spawn(&mut w, 0, FOOT, 14, 20);
    let theirs = spawn(&mut w, 1, FOOT, 30, 20);
    let worker = spawn(&mut w, 1, WORKER, 30, 26);
    let stone = tile(&w, (WALL_X, 20));
    let palisade = tile(&w, (10, 50));
    assert_eq!(w.climb_check(ours, stone), Err(Refusal::Unknown));
    assert_eq!(w.climb_check(worker, stone), Err(Refusal::Unknown));
    assert_eq!(w.climb_check(theirs, palisade), Err(Refusal::NoWalkway));
    assert_eq!(w.climb_check(theirs, stone), Ok(()));
    assert!(!climb_order(&mut w, 0, ours, (WALL_X, 20)));
    assert!(!climb_order(&mut w, 0, theirs, (WALL_X, 20)));
    assert!(climb_order(&mut w, 1, theirs, (WALL_X, 20)));
}

#[test]
fn climbers_take_an_empty_wall_and_come_down_inside() {
    let mut w = world();
    let theirs = spawn(&mut w, 1, FOOT, 26, 24);
    assert!(climb_order(&mut w, 1, theirs, (WALL_X, 24)));
    let started = w.tick_count();
    climb(&mut w, theirs);
    // Ladders are slow: three times the stair.
    assert!(w.tick_count() - started >= 9 * TICKS_PER_SECOND);
    let v = view(&w, theirs).unwrap();
    assert!(!v.climber);
    assert!(!w.wall_tiles(theirs).is_empty());
    // Facing in, over the wall from where they came.
    assert_eq!(v.facing, Angle::of_vec(FxVec2::from_ints(-1, 0)));
    assert!(move_to(&mut w, 1, theirs, 10, 24));
    let v = view(&w, theirs).unwrap();
    assert!(!v.on_wall);
    assert!(v.position.x < Fx::from_int(WALL_X), "{:?}", v.position);
}

/// Hit points lost by side 0's foot manning the wall and by side 1's foot
/// climbing to it, in `seconds` after the climbers reach the top.
fn ladder_fight(seconds: u32) -> (u32, u32) {
    let mut w = world();
    let ours = spawn(&mut w, 0, FOOT, 14, 24);
    assert!(man(&mut w, 0, ours, (WALL_X, 24)));
    climb(&mut w, ours);
    let theirs = spawn(&mut w, 1, FOOT, 26, 30);
    assert!(climb_order(&mut w, 1, theirs, (WALL_X, 30)));
    climb(&mut w, theirs);
    let v = view(&w, theirs).unwrap();
    assert!(v.climber);
    assert_eq!(v.frontage, 3);
    let (a, b) = (hp(&w, ours), hp(&w, theirs));
    let mut fought = false;
    for _ in 0..seconds * TICKS_PER_SECOND {
        w.tick(&[]);
        fought |= view(&w, theirs).is_some_and(|v| v.engaged);
    }
    assert!(fought, "they fought at the ladder head");
    (a - hp(&w, ours), b - hp(&w, theirs))
}

#[test]
fn climbers_fight_the_defenders_at_a_disadvantage() {
    let (defenders, climbers) = ladder_fight(20);
    assert!(climbers > 0);
    assert!(
        climbers * 4 > defenders * 5,
        "defenders lost {defenders}, climbers {climbers}"
    );
}

#[test]
fn climbers_who_beat_the_defenders_take_the_wall() {
    let mut w = world();
    // Two lone defenders, a regiment of climbers.
    let ours = spawn(&mut w, 0, FOOT, 14, 24);
    assert!(man(&mut w, 0, ours, (WALL_X, 24)));
    climb(&mut w, ours);
    let theirs = spawn(&mut w, 1, FOOT, 26, 30);
    assert!(climb_order(&mut w, 1, theirs, (WALL_X, 30)));
    climb(&mut w, theirs);
    assert!(view(&w, theirs).unwrap().climber);
    // The defenders are ordered off: the ladder head is free.
    assert!(move_to(&mut w, 0, ours, 10, 24));
    w.tick(&[]);
    w.tick(&[]);
    let v = view(&w, theirs).unwrap();
    assert!(v.on_wall && !v.climber);
    assert!(w.wall_tiles(theirs).len() > 1);
}

#[test]
fn friends_climbing_the_same_tile_both_take_the_wall() {
    let mut w = world();
    let a = spawn(&mut w, 1, FOOT, 26, 22);
    let b = spawn(&mut w, 1, FOOT, 26, 28);
    assert!(climb_order(&mut w, 1, a, (WALL_X, 25)));
    assert!(climb_order(&mut w, 1, b, (WALL_X, 25)));
    climb(&mut w, a);
    climb(&mut w, b);
    for _ in 0..TICKS_PER_SECOND {
        w.tick(&[]);
    }
    for r in [a, b] {
        let v = view(&w, r).unwrap();
        assert!(v.on_wall && !v.climber, "{r:?} stuck at the ladder head");
    }
}

#[test]
fn climbers_reach_a_defender_that_holds_their_tile_from_afar() {
    let mut w = world();
    // The defender spreads out from y = 10: its block's middle is far
    // from the last tile it holds.
    let ours = spawn(&mut w, 0, FOOT, 14, 12);
    assert!(man(&mut w, 0, ours, (WALL_X, 10)));
    climb(&mut w, ours);
    let last = *w.wall_tiles(ours).last().unwrap();
    let s = w.structures().unwrap();
    let at = s.iter().find(|v| v.handle == last).unwrap().tile;
    let theirs = spawn(&mut w, 1, FOOT, 26, at.1);
    assert!(climb_order(&mut w, 1, theirs, at));
    climb(&mut w, theirs);
    assert!(view(&w, theirs).unwrap().climber);
    let mut fought = false;
    for _ in 0..5 * TICKS_PER_SECOND {
        w.tick(&[]);
        fought |= view(&w, theirs).is_some_and(|v| v.engaged);
    }
    assert!(fought, "the climbers met the defender");
}
