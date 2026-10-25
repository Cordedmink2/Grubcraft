//! Drawn walls (docs/26-fortifications.md, "Drawn walls"): a worker draws a
//! wall at any angle, a building a tile, paid and built tile by tile, and
//! a diagonal wall stops regiments as well as a straight one.

use gc_fixed::{Fx, FxVec2};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{
    Armour, GATE, GROWN, LINE, NEEDS_POWER, PASS, POWERS_FIELD, Refusal, Role, VEINS,
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
/// A gate one tile wide and three deep.
const GATE_3: u16 = 5;
const GATE_COST: u32 = 60;
const STONE: u16 = 6;
const STONE_COST: u32 = 15;
const CHITIN: u16 = 7;
const HALL_VEINS: u8 = 12;
const PYLON_FIELD: u8 = 20;
const RIDGE_HP: u16 = 200;
const MAP: u32 = 48;
const START_GRUB: u32 = 5000;
const TILE_COST: u32 = 10;

/// Open ground, but for a patch of woods on 40..44 × 30..34.
fn world() -> World {
    world_with(START_GRUB)
}

/// [`world`], with each side starting with `grub`.
fn world_with(grub: u32) -> World {
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
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
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
        twist: gc_sim::structures::WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (grub, 0);
    start[1] = (grub, 0);
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
            BuildingStats {
                cost_grub: GATE_COST as u16,
                hp: 1000,
                depth: 3,
                flags: GATE,
                ..palisade
            },
            BuildingStats {
                upgrades_from: Some(PALISADE),
                cost_grub: STONE_COST as u16,
                hp: 625,
                build_ticks: 50,
                ..palisade
            },
            BuildingStats {
                upgrades_from: Some(RIDGE),
                flags: LINE | GROWN,
                armour: Armour::Living,
                hp: 500,
                ..palisade
            },
            BuildingStats {
                upgrades_from: Some(STONE),
                tier: 2,
                hp: 1000,
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

#[test]
fn a_gate_on_its_owners_wall_replaces_that_stretch() {
    let mut w = world();
    let builder = spawn(&mut w, 0, 20, 20);
    // A wall down the map from top to bottom, in two lines.
    assert!(draw(&mut w, 0, builder, (24, 0), (24, 23)));
    assert!(draw(&mut w, 0, builder, (24, 24), (24, 47)));
    build_all(&mut w, 0, 900);
    let gate = |w: &mut World, player: u8, r: RegimentHandle, tile: (i16, i16)| {
        order(
            w,
            player,
            Command::Build {
                regiment: r,
                building: GATE_3,
                tile,
                turn: 0,
            },
        )
    };
    // Not on someone else's wall, and a building put up finished doesn't
    // replace any.
    let enemy = spawn(&mut w, 1, 30, 30);
    assert!(!gate(&mut w, 1, enemy, (24, 30)));
    assert!(!order(
        &mut w,
        0,
        Command::SpawnStructure {
            building: GATE_3,
            tile: (24, 30),
            turn: 0,
        }
    ));
    // Across the join of the two lines: the three tiles under it go.
    let before = grub(&w, 0);
    assert!(gate(&mut w, 0, builder, (24, 22)));
    assert_eq!(grub(&w, 0), before - GATE_COST);
    let (tiles, _) = walls(&w, 0);
    assert_eq!(tiles.len(), 45);
    assert!(!tiles.iter().any(|&(x, y)| x == 24 && (22..25).contains(&y)));
    // Once it is up, the owner's side walks through it.
    let mut t = 0;
    while !w
        .structures()
        .unwrap()
        .iter()
        .any(|v| v.gate.is_some() && v.finished())
    {
        w.tick(&[]);
        t += 1;
        assert!(t < 120 * TICKS_PER_SECOND, "the gate never went up");
    }
    let walker = spawn(&mut w, 0, 16, 23);
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: walker,
            target: FxVec2::from_ints(34, 23),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    let mut through = false;
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        if w.regiments.position(walker).unwrap().x > Fx::from_int(26) {
            through = true;
            break;
        }
    }
    assert!(through, "it never got through its gate");
}

#[test]
fn a_gate_on_a_wall_still_going_up_leaves_its_builder_on_the_rest() {
    let mut w = world();
    let builder = spawn(&mut w, 0, 20, 8);
    assert!(draw(&mut w, 0, builder, (24, 10), (24, 20)));
    // A second worker lays a gate across the middle before any is up.
    let mason = spawn(&mut w, 0, 20, 16);
    assert!(order(
        &mut w,
        0,
        Command::Build {
            regiment: mason,
            building: GATE_3,
            tile: (24, 14),
            turn: 0,
        }
    ));
    assert_eq!(walls(&w, 0).0.len(), 8);
    // The first builder goes on along the wall on both sides of the gate.
    build_all(&mut w, 0, 300);
}

#[test]
fn a_gate_on_a_wall_must_wait_for_the_rest_of_its_ground_to_be_cleared() {
    let mut w = world();
    let builder = spawn(&mut w, 0, 36, 26);
    // Down to the edge of the woods at 40..44 × 30..34.
    assert!(draw(&mut w, 0, builder, (40, 25), (40, 29)));
    build_all(&mut w, 0, 300);
    // A gate over the last two tiles would reach into the woods.
    assert_eq!(w.can_lay(0, GATE_3, (40, 28), 0), Err(Refusal::Overgrown));
    // Wholly on the wall, it goes.
    assert_eq!(w.can_lay(0, GATE_3, (40, 26), 0), Ok(()));
}

fn plan(w: &mut World, player: u8, from: (i16, i16), to: (i16, i16)) -> bool {
    order(
        w,
        player,
        Command::PlanWall {
            building: PALISADE,
            from,
            to,
        },
    )
}

fn work_plans(w: &mut World, player: u8, r: RegimentHandle) -> bool {
    order(w, player, Command::WorkPlan { regiment: r })
}

#[test]
fn a_blueprint_costs_nothing_until_a_worker_lays_it_tile_by_tile() {
    let mut w = world();
    assert!(plan(&mut w, 0, (20, 10), (20, 15)));
    assert_eq!(w.plans().len(), 6);
    assert_eq!(grub(&w, 0), START_GRUB);
    assert!(walls(&w, 0).0.is_empty());
    // Planning the same tiles again plans nothing new.
    assert!(!plan(&mut w, 0, (20, 12), (20, 13)));
    // A worker set to the plans pays for and builds each tile in turn.
    let worker = spawn(&mut w, 0, 16, 12);
    assert!(work_plans(&mut w, 0, worker));
    assert!(w.works_plans(worker));
    let mut t = 0;
    while walls(&w, 0).1 < 6 {
        w.tick(&[]);
        t += 1;
        assert!(t < 300 * TICKS_PER_SECOND, "built {} of 6", walls(&w, 0).1);
        // Never paid ahead of the tiles laid.
        let laid = walls(&w, 0).0.len() as u32;
        assert_eq!(grub(&w, 0), START_GRUB - laid * TILE_COST);
        assert_eq!(w.plans().len() as u32, 6 - laid);
    }
    // Nothing left planned: it leaves the plans.
    w.tick(&[]);
    assert!(!w.works_plans(worker));
}

#[test]
fn a_worker_on_the_plans_waits_while_its_side_cannot_pay() {
    // Enough for one tile.
    let mut w = world_with(TILE_COST);
    assert!(plan(&mut w, 0, (20, 10), (20, 13)));
    let worker = spawn(&mut w, 0, 16, 12);
    assert!(work_plans(&mut w, 0, worker));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    // One tile laid; the rest wait for Grub, and the worker stays on them.
    assert_eq!(w.plans().len(), 3);
    assert!(w.works_plans(worker));
    // Another order takes it off the plans.
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: worker,
            target: FxVec2::from_ints(10, 10),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    assert!(!w.works_plans(worker));
}

#[test]
fn plans_are_dropped_by_order_or_when_their_ground_is_taken() {
    let mut w = world();
    let drop = |w: &mut World, player, from, to| order(w, player, Command::DropPlan { from, to });
    assert!(plan(&mut w, 0, (20, 10), (20, 15)));
    // One tile at a time...
    assert!(drop(&mut w, 0, (20, 10), (20, 10)));
    assert_eq!(w.plans().len(), 5);
    // ...or a whole line as it was planned, either way round, not a stretch
    // of one.
    assert!(!drop(&mut w, 0, (20, 11), (20, 12)));
    // A line planned on from a corner keeps the corner when it is dropped.
    assert!(plan(&mut w, 0, (20, 15), (23, 15)));
    assert_eq!(w.plans().len(), 8);
    assert!(drop(&mut w, 0, (23, 15), (20, 15)));
    assert_eq!(w.plans().len(), 5);
    // Only the planner's own.
    assert!(!drop(&mut w, 1, (20, 10), (20, 15)));
    // Someone else builds on a planned tile: it is dropped when reached.
    spawn_structure(&mut w, 1, PALISADE, (20, 12));
    let worker = spawn(&mut w, 0, 16, 12);
    assert!(work_plans(&mut w, 0, worker));
    build_all(&mut w, 0, 300);
    let mut t = 0;
    while !w.plans().is_empty() {
        w.tick(&[]);
        t += 1;
        assert!(t < 300 * TICKS_PER_SECOND, "plans left: {:?}", w.plans());
    }
    build_all(&mut w, 0, 300);
    assert_eq!(walls(&w, 0).0, [(20, 11), (20, 13), (20, 14), (20, 15)]);
    // A wall drawn over planned tiles takes them off the plan.
    assert!(plan(&mut w, 0, (25, 10), (25, 14)));
    let builder = spawn(&mut w, 0, 24, 8);
    assert!(draw(&mut w, 0, builder, (25, 10), (25, 11)));
    assert_eq!(w.plans().len(), 3);
}

#[test]
fn a_planned_tile_someone_stands_by_waits_for_them_to_go() {
    let mut w = world();
    assert!(plan(&mut w, 0, (30, 10), (30, 12)));
    // A regiment of the side's own idles by the middle tile.
    spawn(&mut w, 0, 30, 11);
    let worker = spawn(&mut w, 0, 26, 9);
    assert!(work_plans(&mut w, 0, worker));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    build_all(&mut w, 0, 300);
    // The other tiles went up; the crowded one is still planned.
    assert!(w.plans().iter().any(|p| p.tile == (30, 11)));
    assert!(!walls(&w, 0).0.contains(&(30, 11)));
    assert!(!walls(&w, 0).0.is_empty(), "{:?}", w.plans());
}

fn raise(w: &mut World, player: u8, tile: (i32, i32), r: Option<RegimentHandle>) -> bool {
    let s = w.structures().unwrap();
    let structure = s
        .iter()
        .find(|v| v.owner == player && v.tile == tile)
        .unwrap()
        .handle;
    order(
        w,
        player,
        Command::RaiseWall {
            structure,
            regiment: r,
        },
    )
}

fn kinds(w: &World, player: u8) -> Vec<u16> {
    let s = w.structures().unwrap();
    s.iter()
        .filter(|v| v.owner == player && v.line.is_some())
        .map(|v| v.building)
        .collect()
}

#[test]
fn a_wall_goes_up_a_level_a_stretch_at_a_time_as_a_worker_raises_it() {
    let mut w = world();
    let worker = spawn(&mut w, 0, 16, 12);
    assert!(draw(&mut w, 0, worker, (20, 10), (20, 14)));
    // Not while it is still going up.
    assert!(!raise(&mut w, 0, (20, 12), Some(worker)));
    build_all(&mut w, 0, 300);
    // A second, separate wall is another stretch.
    assert!(draw(&mut w, 0, worker, (30, 10), (30, 11)));
    build_all(&mut w, 0, 300);
    // A single tile can't be upgraded on its own; the next level needs
    // the tier, and the stretch is paid for now.
    let s = w.structures().unwrap();
    let tile = s.iter().find(|v| v.tile == (20, 12)).unwrap().handle;
    assert!(!order(&mut w, 0, Command::Upgrade { structure: tile }));
    let before = grub(&w, 0);
    assert!(raise(&mut w, 0, (20, 12), Some(worker)));
    assert_eq!(grub(&w, 0), before - 5 * STONE_COST);
    // Raised by hand: nothing happens without a builder.
    assert!(!raise(&mut w, 0, (20, 10), None));
    let mut t = 0;
    while kinds(&w, 0).iter().filter(|&&k| k == STONE).count() < 5 {
        w.tick(&[]);
        t += 1;
        assert!(t < 120 * TICKS_PER_SECOND, "{:?}", kinds(&w, 0));
    }
    // It stood the whole time, and the other wall stayed as it was.
    assert_eq!(walls(&w, 0).0.len(), 7);
    assert_eq!(kinds(&w, 0).iter().filter(|&&k| k == PALISADE).count(), 2);
    // Level 3 (the Castle) needs tier 2.
    let s = w.structures().unwrap();
    let tile = s.iter().find(|v| v.tile == (20, 12)).unwrap().handle;
    assert!(matches!(
        w.wall_level(0, tile),
        Err(gc_sim::structures::Refusal::Tier)
    ));
    assert!(!raise(&mut w, 0, (20, 12), Some(worker)));
    // Without a worker a drawn wall's stretch is paid and waits.
    let idle = spawn(&mut w, 0, 26, 12);
    assert!(raise(&mut w, 0, (30, 10), None));
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert_eq!(kinds(&w, 0).iter().filter(|&&k| k == PALISADE).count(), 2);
    // A worker sent to build it raises it.
    let s = w.structures().unwrap();
    let at = s.iter().find(|v| v.tile == (30, 10)).unwrap().handle;
    assert!(order(
        &mut w,
        0,
        Command::Construct {
            regiment: idle,
            structure: at
        }
    ));
    let mut t = 0;
    while kinds(&w, 0).contains(&PALISADE) {
        w.tick(&[]);
        t += 1;
        assert!(t < 120 * TICKS_PER_SECOND, "{:?}", kinds(&w, 0));
    }
}

#[test]
fn a_grown_wall_grows_up_a_level_on_its_own() {
    let mut w = world();
    assert!(draw_as(&mut w, 0, None, RIDGE, (6, 10), (9, 10)));
    build_all(&mut w, 0, 300);
    let worker = spawn(&mut w, 0, 16, 12);
    // No worker raises a grown wall.
    assert!(!raise(&mut w, 0, (7, 10), Some(worker)));
    assert!(raise(&mut w, 0, (7, 10), None));
    let mut t = 0;
    while kinds(&w, 0).contains(&RIDGE) {
        w.tick(&[]);
        t += 1;
        assert!(t < 120 * TICKS_PER_SECOND, "{:?}", kinds(&w, 0));
    }
    assert!(kinds(&w, 0).iter().all(|&k| k == CHITIN));
}
