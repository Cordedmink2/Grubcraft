//! Drawn walls (docs/26-fortifications.md, "Drawn walls"): a worker draws a
//! wall at any angle, a building a tile, paid and built tile by tile, and
//! a diagonal wall stops regiments as well as a straight one.

use gc_fixed::{Fx, FxVec2};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{
    Armour, GROWN, LINE, NEEDS_POWER, PASS, POWERS_FIELD, Refusal, Role, VEINS,
};
use gc_sim::walls::{MAX_WALL_TILES, WITHER_PER_SECOND, wall_tiles};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Cover, Ground, Growth,
    MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const WORKER: u16 = 0;
const PALISADE: u16 = 0;
/// The main building; it spreads root-veins [`HALL_VEINS`] tiles.
const HALL: u16 = 1;
/// A Choir wall: its side walks through, and it needs power.
const CURTAIN: u16 = 2;
/// A Brood wall: grown on veins with no worker.
const RIDGE: u16 = 3;
/// Powers a field [`PYLON_FIELD`] tiles round it.
const PYLON: u16 = 4;
const HALL_VEINS: u8 = 12;
const PYLON_FIELD: u8 = 20;
const RIDGE_HP: u16 = 200;
const MAP: u32 = 48;
const START_GRUB: u32 = 5000;
const TILE_COST: u32 = 10;

/// Open ground, but for a patch of woods on 40..44 × 30..34.
fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let n = MAP as usize;
    let mut cover = vec![Cover::Open as u8; n * n];
    let mut growth = vec![Growth::Open as u8; n * n];
    for y in 30..34 {
        for x in 40..44 {
            cover[y * n + x] = Cover::Wood as u8;
            growth[y * n + x] = Growth::Woods as u8;
        }
    }
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 4,
        capacities: Capacities { regiments: 16 },
        units: &[worker],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &growth,
        },
        champions: false,
    });
    let palisade = BuildingStats {
        role: Role::Wall,
        tier: 1,
        upgrades_from: None,
        cost_grub: TILE_COST as u16,
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
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (START_GRUB, 0);
    start[1] = (START_GRUB, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            palisade,
            BuildingStats {
                role: Role::Main,
                width: 3,
                depth: 3,
                flags: VEINS,
                field: HALL_VEINS,
                ..palisade
            },
            BuildingStats {
                flags: LINE | PASS | NEEDS_POWER,
                ..palisade
            },
            BuildingStats {
                flags: LINE | GROWN,
                armour: Armour::Living,
                hp: RIDGE_HP,
                ..palisade
            },
            BuildingStats {
                role: Role::Economy,
                width: 2,
                depth: 2,
                flags: POWERS_FIELD,
                field: PYLON_FIELD,
                ..palisade
            },
        ],
        units: &[ConquestUnit {
            worker: true,
            supply: 1,
            tier: 1,
            ..ConquestUnit::default()
        }],
        structures: 1024,
        start,
        ..ConquestSetup::default()
    }));
    // Halls, for the tier walls need.
    for (player, tile) in [(0, (2, 2)), (1, (44, 44))] {
        assert!(order(
            &mut w,
            player,
            Command::SpawnStructure {
                building: HALL,
                tile,
                turn: 0,
            }
        ));
    }
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit: WORKER,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn draw(w: &mut World, player: u8, r: RegimentHandle, from: (i16, i16), to: (i16, i16)) -> bool {
    draw_as(w, player, Some(r), PALISADE, from, to)
}

fn draw_as(
    w: &mut World,
    player: u8,
    regiment: Option<RegimentHandle>,
    building: u16,
    from: (i16, i16),
    to: (i16, i16),
) -> bool {
    order(
        w,
        player,
        Command::BuildWall {
            regiment,
            building,
            from,
            to,
        },
    )
}

/// Tick until every tile of `player`'s walls is up.
fn build_all(w: &mut World, player: u8, seconds: u32) {
    let mut t = 0;
    while walls(w, player).1 < walls(w, player).0.len() {
        w.tick(&[]);
        t += 1;
        assert!(t < seconds * TICKS_PER_SECOND, "the wall never went up");
    }
}

/// Send `r` to `goal` and say whether it gets across the line x + y = 48
/// within a minute.
fn crosses(w: &mut World, player: u8, r: RegimentHandle, goal: (i32, i32)) -> bool {
    assert!(order(
        w,
        player,
        Command::MoveTo {
            regiment: r,
            target: FxVec2::from_ints(goal.0, goal.1),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        let p = w.regiments.position(r).unwrap();
        if p.x + p.y > Fx::from_int(49) {
            return true;
        }
    }
    false
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

fn grub(w: &World, player: u8) -> u32 {
    w.bank(player).unwrap().grub
}

/// The tiles of `player`'s walls, and how many of them are finished.
fn walls(w: &World, player: u8) -> (Vec<(i32, i32)>, usize) {
    let s = w.structures().unwrap();
    let tiles: Vec<_> = s
        .iter()
        .filter(|v| v.owner == player && v.line.is_some())
        .collect();
    let done = tiles.iter().filter(|v| v.finished()).count();
    let mut at: Vec<_> = tiles.iter().map(|v| v.tile).collect();
    at.sort_unstable();
    (at, done)
}

#[test]
fn a_line_runs_tile_to_tile_at_any_angle() {
    assert_eq!(
        wall_tiles((0, 0), (3, 0)).unwrap(),
        [(0, 0), (1, 0), (2, 0), (3, 0)]
    );
    assert_eq!(
        wall_tiles((5, 5), (2, 2)).unwrap(),
        [(5, 5), (4, 4), (3, 3), (2, 2)]
    );
    // A shallow slope steps once, halfway.
    assert_eq!(
        wall_tiles((0, 0), (4, 1)).unwrap(),
        [(0, 0), (1, 0), (2, 1), (3, 1), (4, 1)]
    );
    assert_eq!(wall_tiles((7, 7), (7, 7)).unwrap(), [(7, 7)]);
    let most = MAX_WALL_TILES as i32 - 1;
    assert_eq!(wall_tiles((0, 0), (most, 3)).unwrap().len(), MAX_WALL_TILES);
    assert!(wall_tiles((0, 0), (most + 1, 0)).is_none());
}

#[test]
fn a_worker_draws_a_wall_pays_by_the_tile_and_builds_it_all() {
    let mut w = world();
    let worker = spawn(&mut w, 0, 10, 20);
    assert!(draw(&mut w, 0, worker, (12, 10), (17, 15)));
    let (tiles, done) = walls(&w, 0);
    assert_eq!(tiles.len(), 6);
    assert_eq!(done, 0);
    assert_eq!(grub(&w, 0), START_GRUB - 6 * TILE_COST);
    // The worker goes from tile to tile until the whole wall is up.
    let mut t = 0;
    while walls(&w, 0).1 < 6 {
        w.tick(&[]);
        t += 1;
        assert!(t < 120 * TICKS_PER_SECOND, "built {} of 6", walls(&w, 0).1);
    }
}

#[test]
fn a_diagonal_wall_stops_a_regiment_as_a_straight_one_does() {
    let mut w = world();
    let builder = spawn(&mut w, 0, 4, 4);
    // Corner to corner across the map in two lines: below-left of it is cut
    // off from above-right.
    assert!(draw(&mut w, 0, builder, (0, 47), (23, 24)));
    assert!(draw(&mut w, 0, builder, (24, 23), (47, 0)));
    let mut t = 0;
    while walls(&w, 0).1 < walls(&w, 0).0.len() {
        w.tick(&[]);
        t += 1;
        assert!(t < 900 * TICKS_PER_SECOND, "the wall never went up");
    }
    // From below-left of the wall, up and right of it is out of reach.
    let walker = spawn(&mut w, 0, 10, 30);
    let goal = FxVec2::from_ints(36, 16);
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: walker,
            target: goal,
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        let p = w.regiments.position(walker).unwrap();
        assert!(p.x + p.y < Fx::from_int(48), "it got across at {p:?}");
    }
    // A breach is one tile: knock out two and it walks through.
    for at in [(17, 30), (18, 29)] {
        let tile = w
            .structures()
            .unwrap()
            .iter()
            .find(|v| v.tile == at)
            .unwrap();
        assert!(order(
            &mut w,
            0,
            Command::RemoveStructure {
                structure: tile.handle
            }
        ));
    }
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: walker,
            target: goal,
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    let mut across = false;
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        let p = w.regiments.position(walker).unwrap();
        if p.x + p.y > Fx::from_int(49) {
            across = true;
            break;
        }
    }
    assert!(across, "it never came through the breach");
}

#[test]
fn lines_join_and_cross_the_owners_walls_but_not_others() {
    let mut w = world();
    let a = spawn(&mut w, 0, 10, 20);
    assert!(draw(&mut w, 0, a, (10, 10), (14, 10)));
    // Going on from the last tile leaves it out: a joint, not a second
    // building on it.
    let before = grub(&w, 0);
    assert!(draw(&mut w, 0, a, (14, 10), (14, 14)));
    assert_eq!(walls(&w, 0).0.len(), 5 + 4);
    assert_eq!(grub(&w, 0), before - 4 * TILE_COST);
    // Across the first line, again leaving out the tile they share.
    assert!(draw(&mut w, 0, a, (12, 8), (12, 12)));
    assert_eq!(walls(&w, 0).0.len(), 9 + 4);
    // Another side's line can't run through them.
    let b = spawn(&mut w, 1, 30, 30);
    assert_eq!(
        w.wall_plan(1, PALISADE, (11, 9), (11, 11)).unwrap()[1].fits,
        Err(Refusal::Built)
    );
    assert!(!draw(&mut w, 1, b, (11, 9), (11, 11)));
}

#[test]
fn a_line_with_any_tile_it_cant_have_is_refused_whole() {
    let mut w = world();
    let worker = spawn(&mut w, 0, 30, 20);
    // Into the woods: clear them first.
    let plan = w.wall_plan(0, PALISADE, (36, 32), (41, 32)).unwrap();
    assert_eq!(plan[5].fits, Err(Refusal::Overgrown));
    assert!(plan[..4].iter().all(|t| t.fits.is_ok()));
    assert!(!draw(&mut w, 0, worker, (36, 32), (41, 32)));
    // Over the worker itself.
    assert!(!draw(&mut w, 0, worker, (26, 20), (34, 20)));
    // Too long, and the hall is no wall.
    assert!(w.wall_plan(0, PALISADE, (0, 0), (40, 0)).is_err());
    assert_eq!(w.wall_plan(0, HALL, (0, 0), (4, 0)), Err(Refusal::Unknown));
    assert_eq!(grub(&w, 0), START_GRUB);
    assert!(walls(&w, 0).0.is_empty());
}

#[test]
fn a_wall_costs_what_its_owner_can_pay_for_all_of() {
    let mut w = world();
    let worker = spawn(&mut w, 0, 10, 46);
    // Spend in lines of 30 tiles while they can be paid for.
    let mut y = 8;
    while grub(&w, 0) >= 30 * TILE_COST {
        assert!(draw(&mut w, 0, worker, (2, y), (31, y)));
        y += 2;
    }
    let left = grub(&w, 0);
    let fits = (left / TILE_COST) as i16;
    assert!(fits > 0 && fits < 30);
    // One tile more than it can pay for is refused; as many as it can is not.
    assert_eq!(
        w.wall_plan(
            0,
            PALISADE,
            (2, i32::from(y)),
            (2 + i32::from(fits), i32::from(y))
        ),
        Err(Refusal::Price)
    );
    assert!(draw(&mut w, 0, worker, (2, y), (1 + fits, y)));
    assert_eq!(grub(&w, 0), left - u32::from(fits as u16) * TILE_COST);
}

#[test]
fn a_tile_called_off_leaves_the_builder_going_on_with_the_rest() {
    let mut w = world();
    let worker = spawn(&mut w, 0, 10, 20);
    assert!(draw(&mut w, 0, worker, (12, 18), (12, 13)));
    // The tile it is walking to is called off before it gets there.
    let s = w.structures().unwrap();
    let first = s
        .iter()
        .filter(|v| v.line.is_some())
        .min_by_key(|v| (v.tile.1 - 20).abs())
        .unwrap();
    assert_eq!(first.tile, (12, 18));
    assert!(order(
        &mut w,
        0,
        Command::CallOff {
            structure: first.handle
        }
    ));
    let mut t = 0;
    while walls(&w, 0).1 < 5 {
        w.tick(&[]);
        t += 1;
        assert!(t < 120 * TICKS_PER_SECOND, "built {} of 5", walls(&w, 0).1);
    }
    assert_eq!(walls(&w, 0).0.len(), 5);
}

#[test]
fn a_wall_is_drawn_not_laid_as_one_building() {
    let mut w = world();
    let worker = spawn(&mut w, 0, 10, 20);
    assert!(!order(
        &mut w,
        0,
        Command::Build {
            regiment: worker,
            building: PALISADE,
            tile: (12, 12),
            turn: 0,
        }
    ));
}

#[test]
fn the_choir_walks_through_its_curtain_until_the_power_is_cut() {
    let mut w = world();
    // Two pylons power the whole diagonal, one each side of the middle.
    spawn_structure(&mut w, 0, PYLON, (5, 30));
    spawn_structure(&mut w, 0, PYLON, (30, 5));
    // Out of the power, a curtain is refused.
    assert_eq!(
        w.wall_plan(0, CURTAIN, (40, 46), (46, 46)).unwrap()[0].fits,
        Err(Refusal::Unpowered)
    );
    let builder = spawn(&mut w, 0, 4, 4);
    assert!(draw_as(
        &mut w,
        0,
        Some(builder),
        CURTAIN,
        (0, 47),
        (23, 24)
    ));
    assert!(draw_as(
        &mut w,
        0,
        Some(builder),
        CURTAIN,
        (24, 23),
        (47, 0)
    ));
    build_all(&mut w, 0, 900);
    // Its own side walks straight through; an enemy doesn't.
    let own = spawn(&mut w, 0, 12, 28);
    assert!(
        crosses(&mut w, 0, own, (36, 16)),
        "its own side got through"
    );
    let foe = spawn(&mut w, 1, 10, 30);
    assert!(!crosses(&mut w, 1, foe, (36, 16)), "an enemy got through");
    // Cut the power and it drops, open to everyone.
    let pylons: Vec<_> = (w.structures().unwrap().iter())
        .filter(|v| v.building == PYLON)
        .map(|v| v.handle)
        .collect();
    for structure in pylons {
        assert!(order(&mut w, 0, Command::RemoveStructure { structure }));
    }
    assert!(
        crosses(&mut w, 1, foe, (36, 16)),
        "it dropped without power"
    );
}

#[test]
fn the_brood_grows_its_ridge_along_its_veins_with_no_worker() {
    let mut w = world();
    let worker = spawn(&mut w, 0, 10, 20);
    // No worker for a grown wall, and one for any other.
    assert!(!draw_as(&mut w, 0, Some(worker), RIDGE, (6, 10), (12, 10)));
    assert!(!draw_as(&mut w, 0, None, PALISADE, (6, 10), (12, 10)));
    // Only where the hall's root-veins reach.
    let plan = w.wall_plan(0, RIDGE, (6, 10), (24, 10)).unwrap();
    assert!(plan[..8].iter().all(|t| t.fits.is_ok()));
    assert_eq!(plan.last().unwrap().fits, Err(Refusal::NoVeins));
    assert!(!draw_as(&mut w, 0, None, RIDGE, (6, 10), (24, 10)));
    // Every tile grows at once, on its own.
    assert!(draw_as(&mut w, 0, None, RIDGE, (6, 10), (12, 10)));
    assert_eq!(grub(&w, 0), START_GRUB - 7 * TILE_COST);
    let mut t = 0;
    while walls(&w, 0).1 < 7 {
        w.tick(&[]);
        t += 1;
        // One build time, give or take, not seven.
        assert!(t < 3 * 40, "grew {} of 7", walls(&w, 0).1);
    }
    // Cut off from the veins (the hall comes down), it withers away.
    let hall = (w.structures().unwrap().iter())
        .find(|v| v.owner == 0 && v.building == HALL)
        .unwrap()
        .handle;
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: hall }
    ));
    for _ in 0..5 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let s = w.structures().unwrap();
    let ridge: Vec<_> = s.iter().filter(|v| v.building == RIDGE).collect();
    assert_eq!(ridge.len(), 7, "still standing, withering");
    assert!(
        ridge.iter().all(|v| v.hp < u32::from(RIDGE_HP)),
        "{ridge:?}"
    );
    let life = u32::from(RIDGE_HP) / WITHER_PER_SECOND;
    for _ in 0..life * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert!(walls(&w, 0).0.is_empty(), "the ridge withered");
}
