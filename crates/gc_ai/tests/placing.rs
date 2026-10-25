//! Where a Conquest mind puts its buildings (`gc_ai::conquest::placing`):
//! every spot it picks is one the simulation accepts, keeps a tile between
//! buildings, stays off the lanes workers walk between a hall and its
//! seeps, and never walls the hall in.
//!
//! The world is a small Conquest map built by hand, as `gc_sim`'s own tests
//! do: player 0 has a 5x5 hall (a main building that takes Grub and Marrow),
//! a 3x3 house type to place, a worker, and a couple of Grub seeps nearby.

use gc_ai::conquest::placing::{Foot, GAP, LANE, REACH, WIDE_GAP, overlaps, spot_for};
use gc_ai::perception::v2;
use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::economy::Resource;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, DROP_OFF_MARROW, Refusal, Role};
use gc_sim::{
    BuildingId, BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MAX_PARTIES,
    MatchSetup, NodeSetup, PlayerCommand, PlayerId, RegimentHandle, TICKS_PER_SECOND, UnitStats,
    World,
};
use glam::Vec2;

const TILES: i32 = 64;
const ME: PlayerId = 0;
const WORKER: u16 = 0;
const HALL: BuildingId = 0;
const HOUSE: BuildingId = 1;
/// Where the hall stands (north-west tile); its middle is (30.5, 30.5).
const HALL_AT: (i16, i16) = (28, 28);
const HOUSE_COST: u16 = 40;

fn hall_stats() -> BuildingStats {
    BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 400,
        cost_marrow: 0,
        build_ticks: 500,
        hp: 3000,
        armour: Armour::Fortified,
        width: 5,
        depth: 5,
        supply: 15,
        flags: DROP_OFF_GRUB | DROP_OFF_MARROW,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: gc_sim::structures::WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    }
}

fn house_stats() -> BuildingStats {
    BuildingStats {
        role: Role::Housing,
        cost_grub: HOUSE_COST,
        width: 3,
        depth: 3,
        supply: 10,
        flags: 0,
        ..hall_stats()
    }
}

/// A hall and a house type, by [`HALL`] and [`HOUSE`].
fn buildings() -> [BuildingStats; 2] {
    [hall_stats(), house_stats()]
}

fn grub_seep(x: i32, y: i32) -> NodeSetup {
    NodeSetup {
        resource: Resource::Grub,
        at: FxVec2::from_ints(x, y),
        amount: 5000,
    }
}

/// Two seeps a good walk from the hall: one east, one south.
fn two_seeps() -> Vec<NodeSetup> {
    vec![grub_seep(40, 31), grub_seep(31, 41)]
}

/// A conquest world `TILES` across where the tiles `rough` says are blocked
/// ground, with these seeps and `grub` in player 0's store. No buildings
/// yet.
fn empty_world(rough: impl Fn(i32, i32) -> bool, nodes: &[NodeSetup], grub: u32) -> World {
    let side = (TILES * CELLS_PER_TILE) as usize;
    let nav: Vec<u8> = (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as i32, (i / side) as i32);
            if rough(x / CELLS_PER_TILE, y / CELLS_PER_TILE) {
                BLOCKED
            } else {
                1
            }
        })
        .collect();
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 11,
        capacities: Capacities { regiments: 64 },
        units: &[worker],
        map_tiles: TILES as u32,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let mut start = [(0, 0); MAX_PARTIES];
    start[usize::from(ME)] = (grub, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings(),
        units: &[ConquestUnit {
            worker: true,
            supply: 1,
            ..ConquestUnit::default()
        }],
        nodes,
        start,
        structures: 64,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, command: Command) -> bool {
    w.tick(&[PlayerCommand {
        player: ME,
        command,
    }])
    .commands_applied
        == 1
}

/// Put up a finished building for player 0, at once.
fn put_up(w: &mut World, building: BuildingId, tile: (i32, i32)) {
    let command = Command::SpawnStructure {
        building,
        tile: (tile.0 as i16, tile.1 as i16),
        turn: 0,
    };
    assert!(order(w, command), "building {building} at {tile:?} refused");
}

/// A worker for player 0 standing at `(x, y)`.
fn spawn_worker(w: &mut World, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let facing: Angle = NORTH;
    assert!(order(
        w,
        Command::SpawnRegiment {
            unit: WORKER,
            position: FxVec2::from_ints(x, y),
            facing,
        }
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .expect("the worker did not appear")
}

/// The usual world: open map, the hall up, two seeps, Grub to spare, and a
/// worker far away in the north-west corner.
fn base_world() -> (World, RegimentHandle) {
    let mut w = empty_world(|_, _| false, &two_seeps(), 2000);
    put_up(&mut w, HALL, (i32::from(HALL_AT.0), i32::from(HALL_AT.1)));
    let worker = spawn_worker(&mut w, 6, 6);
    (w, worker)
}

fn hall_centre() -> Vec2 {
    Vec2::new(f32::from(HALL_AT.0) + 2.5, f32::from(HALL_AT.1) + 2.5)
}

fn node_centres(w: &World) -> Vec<Vec2> {
    w.economy().unwrap().nodes().map(|n| v2(n.at)).collect()
}

/// Lanes from the hall's middle to each seep, as the Steward builds them.
fn lanes_to_nodes(w: &World) -> Vec<(Vec2, Vec2)> {
    node_centres(w)
        .into_iter()
        .map(|n| (hall_centre(), n))
        .collect()
}

/// Footprints of player 0's buildings.
fn my_feet(w: &World) -> Vec<Foot> {
    w.structures()
        .unwrap()
        .iter()
        .filter(|v| v.owner == ME)
        .map(|v| [v.tile.0, v.tile.1, i32::from(v.width), i32::from(v.depth)])
        .collect()
}

fn foot_at(tile: (i32, i32), stats: &BuildingStats) -> Foot {
    [
        tile.0,
        tile.1,
        i32::from(stats.width),
        i32::from(stats.depth),
    ]
}

/// Straight-line distance from the footprint's rectangle to the lane,
/// measured every hundredth of a tile along it.
fn distance_to_lane(f: Foot, a: Vec2, b: Vec2) -> f32 {
    let min = Vec2::new(f[0] as f32, f[1] as f32);
    let max = min + Vec2::new(f[2] as f32, f[3] as f32);
    let steps = (a.distance(b) * 100.0).ceil().max(1.0) as i32;
    (0..=steps)
        .map(|i| {
            let p = a.lerp(b, i as f32 / steps as f32);
            p.clamp(min, max).distance(p)
        })
        .fold(f32::MAX, f32::min)
}

// Reachability, from the simulation's own walkable ground. ----------------

/// The middle of half-tile cell `(cx, cy)`, in tiles.
fn cell_centre(cx: i32, cy: i32) -> FxVec2 {
    FxVec2::new(
        Fx::from_ratio(2 * cx + 1, 2 * CELLS_PER_TILE),
        Fx::from_ratio(2 * cy + 1, 2 * CELLS_PER_TILE),
    )
}

fn cell_walkable(w: &World, cx: i32, cy: i32) -> bool {
    let side = TILES * CELLS_PER_TILE;
    cx >= 0 && cy >= 0 && cx < side && cy < side && w.nav().walkable(0, cell_centre(cx, cy))
}

/// The walkable cell nearest `p` within 4 tiles of it.
fn nearest_walkable(w: &World, p: Vec2) -> Option<(i32, i32)> {
    let (px, py) = (
        (p.x * CELLS_PER_TILE as f32) as i32,
        (p.y * CELLS_PER_TILE as f32) as i32,
    );
    let reach = 4 * CELLS_PER_TILE;
    (-reach..=reach)
        .flat_map(|dy| (-reach..=reach).map(move |dx| (px + dx, py + dy)))
        .filter(|&(x, y)| cell_walkable(w, x, y))
        .min_by_key(|&(x, y)| (x - px) * (x - px) + (y - py) * (y - py))
}

/// Every walkable cell a regiment's centre could reach from `start`
/// without crossing a cell it may not stand on (4-way, so it never counts
/// a squeeze between two diagonal corners).
fn flood(w: &World, start: (i32, i32)) -> std::collections::HashSet<(i32, i32)> {
    let mut seen = std::collections::HashSet::from([start]);
    let mut todo = vec![start];
    while let Some((x, y)) = todo.pop() {
        for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if cell_walkable(w, n.0, n.1) && seen.insert(n) {
                todo.push(n);
            }
        }
    }
    seen
}

/// Distance in tiles from point `p` to the rectangle `f`.
fn distance_to_foot(f: Foot, p: Vec2) -> f32 {
    let min = Vec2::new(f[0] as f32, f[1] as f32);
    let max = min + Vec2::new(f[2] as f32, f[3] as f32);
    p.clamp(min, max).distance(p)
}

/// Whether a worker could still walk from `node` to within `slack` tiles of
/// the hall: the cell nearest the seep is in the same open area as a cell
/// that close to the hall's footprint.
fn node_reaches_hall(w: &World, node: Vec2, hall: Foot, slack: f32) -> bool {
    let Some(start) = nearest_walkable(w, node) else {
        return false;
    };
    flood(w, start).iter().any(|&(x, y)| {
        let p = Vec2::new(
            (x as f32 + 0.5) / CELLS_PER_TILE as f32,
            (y as f32 + 0.5) / CELLS_PER_TILE as f32,
        );
        distance_to_foot(hall, p) <= slack
    })
}

fn hall_foot() -> Foot {
    [i32::from(HALL_AT.0), i32::from(HALL_AT.1), 5, 5]
}

// Tests. ------------------------------------------------------------------

/// The spot is one the simulation accepts for player 0 (placement, tier,
/// price), and a worker's Build order lands there.
#[test]
fn a_spot_is_one_the_simulation_accepts() {
    let (mut w, worker) = base_world();
    let stats = house_stats();
    let keep = lanes_to_nodes(&w);
    let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep).expect("room for a house");
    assert_eq!(w.can_lay(ME, HOUSE, tile, 0), Ok(()));

    let before = w.structures().unwrap().len();
    let grub_before = w.bank(ME).unwrap().grub;
    assert!(
        order(
            &mut w,
            Command::Build {
                regiment: worker,
                building: HOUSE,
                tile: (tile.0 as i16, tile.1 as i16),
                turn: 0,
            }
        ),
        "the worker's Build at {tile:?} was refused"
    );
    assert_eq!(w.structures().unwrap().len(), before + 1);
    assert!(w.structures().unwrap().iter().any(|v| v.tile == tile));
    assert!(
        w.bank(ME).unwrap().grub < grub_before,
        "it was not paid for"
    );
}

/// The same spot also takes a finished building by SpawnStructure.
#[test]
fn the_spot_takes_a_spawned_building_too() {
    let (mut w, _) = base_world();
    let tile = spot_for(
        &w,
        ME,
        HOUSE,
        &house_stats(),
        hall_centre(),
        &lanes_to_nodes(&w),
    )
    .unwrap();
    put_up(&mut w, HOUSE, tile);
}

/// The footprint keeps at least GAP tiles from every building the player
/// has, house after house.
#[test]
fn spots_keep_a_gap_from_every_building() {
    let (mut w, _) = base_world();
    let stats = house_stats();
    for n in 0..8 {
        let keep = lanes_to_nodes(&w);
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep)
            .unwrap_or_else(|| panic!("no room for house {n}"));
        let foot = foot_at(tile, &stats);
        for other in my_feet(&w) {
            assert!(
                !overlaps(foot, other, GAP),
                "house {n} at {foot:?} is within {GAP} of {other:?}"
            );
        }
        put_up(&mut w, HOUSE, tile);
    }
}

/// Asked for a spot right beside an enemy building, the answer never
/// overlaps it (the simulation's own `can_lay` sees to that).
#[test]
fn spots_never_overlap_an_enemy_building() {
    let (mut w, _) = base_world();
    let enemy = PlayerCommand {
        player: 1,
        command: Command::SpawnStructure {
            building: HOUSE,
            tile: (33, 24),
            turn: 0,
        },
    };
    assert_eq!(w.tick(&[enemy]).commands_applied, 1);
    let stats = house_stats();
    let tile = spot_for(&w, ME, HOUSE, &stats, Vec2::new(34.5, 25.5), &[]).unwrap();
    assert_eq!(w.can_lay(ME, HOUSE, tile, 0), Ok(()));
    assert!(!overlaps(foot_at(tile, &stats), [33, 24, 3, 3], 0));
}

/// With lanes from the hall's middle to each seep, no footprint comes
/// within LANE tiles of a lane, however many houses are placed.
#[test]
fn spots_stay_off_the_lanes() {
    let (mut w, _) = base_world();
    let stats = house_stats();
    let keep = lanes_to_nodes(&w);
    for n in 0..8 {
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep)
            .unwrap_or_else(|| panic!("no room for house {n}"));
        let foot = foot_at(tile, &stats);
        for &(a, b) in &keep {
            let d = distance_to_lane(foot, a, b);
            assert!(
                d >= LANE,
                "house {n} at {foot:?} comes within {d} of the lane {a:?}-{b:?} (LANE {LANE})"
            );
        }
        put_up(&mut w, HOUSE, tile);
    }
}

/// Even when the best spot (nearest the seep) lies right on a lane, the
/// answer moves aside.
#[test]
fn asking_for_a_spot_on_a_lane_gives_one_beside_it() {
    let (w, _) = base_world();
    let stats = house_stats();
    let keep = lanes_to_nodes(&w);
    let on_lane = Vec2::new(35.0, 31.0);
    let tile = spot_for(&w, ME, HOUSE, &stats, on_lane, &keep).expect("room beside the lane");
    let foot = foot_at(tile, &stats);
    for &(a, b) in &keep {
        assert!(
            distance_to_lane(foot, a, b) >= LANE,
            "{foot:?} is on a lane"
        );
    }
}

/// The search is deterministic: the same world gives the same spot.
#[test]
fn the_same_question_gets_the_same_answer() {
    let (w, _) = base_world();
    let keep = lanes_to_nodes(&w);
    let a = spot_for(&w, ME, HOUSE, &house_stats(), hall_centre(), &keep);
    let b = spot_for(&w, ME, HOUSE, &house_stats(), hall_centre(), &keep);
    assert_eq!(a, b);
}

/// Eight houses one after another (each laid before the next is asked for)
/// all find a spot, and the houses are never on top of each other.
#[test]
fn eight_houses_all_find_room() {
    let (mut w, _) = base_world();
    let stats = house_stats();
    let mut spots = Vec::new();
    for n in 0..8 {
        let keep = lanes_to_nodes(&w);
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep)
            .unwrap_or_else(|| panic!("no room for house {n}"));
        assert!(!spots.contains(&tile), "house {n} asked for {tile:?} twice");
        spots.push(tile);
        put_up(&mut w, HOUSE, tile);
    }
    assert_eq!(my_feet(&w).len(), 9);
}

/// Houses are laid by a worker's Build order, not only spawned: the
/// foundation is a building at once, so the next spot keeps clear of it.
#[test]
fn houses_laid_by_a_worker_are_kept_clear_of_too() {
    let (mut w, worker) = base_world();
    let stats = house_stats();
    let mut laid: Vec<Foot> = Vec::new();
    for n in 0..4 {
        let keep = lanes_to_nodes(&w);
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep)
            .unwrap_or_else(|| panic!("no room for house {n}"));
        let foot = foot_at(tile, &stats);
        assert!(
            laid.iter().all(|&f| !overlaps(foot, f, GAP)),
            "house {n} at {foot:?} is too near {laid:?}"
        );
        assert!(
            order(
                &mut w,
                Command::Build {
                    regiment: worker,
                    building: HOUSE,
                    tile: (tile.0 as i16, tile.1 as i16),
                    turn: 0,
                }
            ),
            "Build of house {n} at {tile:?} refused"
        );
        laid.push(foot);
    }
}

/// After eight houses, a worker could still get from each seep to the hall:
/// the hall is not walled in. (4-way flood fill over the cells a
/// regiment's centre may stand on, to within two tiles of the hall.)
#[test]
fn eight_houses_never_box_the_hall_in() {
    let (mut w, _) = base_world();
    let stats = house_stats();
    for n in 0..8 {
        let keep = lanes_to_nodes(&w);
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep)
            .unwrap_or_else(|| panic!("no room for house {n}"));
        put_up(&mut w, HOUSE, tile);
        for node in node_centres(&w) {
            assert!(
                node_reaches_hall(&w, node, hall_foot(), 2.0),
                "after house {n} at {tile:?}, the seep at {node:?} cannot reach the hall"
            );
        }
    }
}

/// The ground facing each seep stays open: a ring of walkable cells next
/// to the hall's side that faces the seep (not only a path somewhere).
#[test]
fn the_side_of_the_hall_facing_each_seep_stays_open() {
    let (mut w, _) = base_world();
    let stats = house_stats();
    for n in 0..8 {
        let keep = lanes_to_nodes(&w);
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep)
            .unwrap_or_else(|| panic!("no room for house {n}"));
        put_up(&mut w, HOUSE, tile);
    }
    let houses: Vec<Foot> = my_feet(&w).into_iter().skip(1).collect();
    for node in node_centres(&w) {
        let dir = (node - hall_centre()).normalize();
        // The point on the hall's edge, and one tile past it, on the way to the seep.
        let edge = hall_centre() + dir * 3.5;
        for f in &houses {
            assert!(
                distance_to_foot(*f, edge) >= 1.0,
                "house {f:?} sits on the path from the hall to the seep at {node:?}"
            );
        }
    }
}

/// Near the map's edge, or a corner, the answer is a spot that can be laid
/// or nothing, and never a panic.
#[test]
fn near_the_map_edge_is_none_or_valid() {
    let (w, _) = base_world();
    let stats = house_stats();
    let edge = TILES as f32;
    for near in [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.5, 31.0),
        Vec2::new(edge, edge),
        Vec2::new(edge - 0.5, 10.0),
        Vec2::new(31.0, edge + 0.5),
        Vec2::new(-30.0, -30.0),
        Vec2::new(500.0, 500.0),
        Vec2::new(-1.0e6, 1.0e6),
    ] {
        if let Some(tile) = spot_for(&w, ME, HOUSE, &stats, near, &[]) {
            assert_eq!(
                w.can_lay(ME, HOUSE, tile, 0),
                Ok(()),
                "near {near:?}: {tile:?}"
            );
            let f = foot_at(tile, &stats);
            assert!(f[0] >= 0 && f[1] >= 0 && f[0] + f[2] <= TILES && f[1] + f[3] <= TILES);
        }
    }
}

/// A map edge a spot is asked for on the very edge: the spot is pulled
/// inside, onto the map.
#[test]
fn a_corner_spot_is_pulled_onto_the_map() {
    let (w, _) = base_world();
    let tile = spot_for(&w, ME, HOUSE, &house_stats(), Vec2::new(0.0, 0.0), &[])
        .expect("the corner is open ground");
    assert!(tile.0 >= 0 && tile.1 >= 0);
    assert_eq!(w.can_lay(ME, HOUSE, tile, 0), Ok(()));
}

/// With a lake across the place wanted, the spot is on dry ground beside
/// it.
#[test]
fn blocked_ground_is_never_built_on() {
    let lake = |x: i32, y: i32| (14..30).contains(&x) && (14..30).contains(&y);
    let mut w = empty_world(lake, &[], 2000);
    put_up(&mut w, HALL, (40, 40));
    let stats = house_stats();
    let tile = spot_for(&w, ME, HOUSE, &stats, Vec2::new(22.0, 22.0), &[])
        .expect("dry ground within reach");
    assert_eq!(w.can_lay(ME, HOUSE, tile, 0), Ok(()));
    let f = foot_at(tile, &stats);
    let wet = (f[0]..f[0] + f[2]).any(|x| (f[1]..f[1] + f[3]).any(|y| lake(x, y)));
    assert!(!wet, "{f:?} is in the lake");
}

/// When all ground within REACH is blocked, nothing is found: `None`, not
/// a panic and not a spot out at the edge of the map.
#[test]
fn nothing_within_reach_is_none() {
    // Only the hall's own corner is open ground.
    let open = |x: i32, y: i32| x < 14 && y < 14;
    let mut w = empty_world(|x, y| !open(x, y), &[], 2000);
    put_up(&mut w, HALL, (1, 1));
    let near = Vec2::new(50.0, 50.0);
    assert!(near.distance(Vec2::new(14.0, 14.0)) > REACH as f32 * 1.5);
    assert_eq!(spot_for(&w, ME, HOUSE, &house_stats(), near, &[]), None);
}

/// A cramped map has one place for a house: it is found, and then the next
/// ask is `None`.
#[test]
fn a_full_map_gives_none() {
    // Open ground 11 x 6 tiles holding the hall (5 wide), a gap, and room
    // for one 3x3 house.
    let open = |x: i32, y: i32| (20..31).contains(&x) && (20..26).contains(&y);
    let mut w = empty_world(|x, y| !open(x, y), &[], 2000);
    put_up(&mut w, HALL, (20, 20));
    let stats = house_stats();
    let near = Vec2::new(28.0, 22.0);
    let t = spot_for(&w, ME, HOUSE, &stats, near, &[]).expect("room for one house");
    assert_eq!(w.can_lay(ME, HOUSE, t, 0), Ok(()));
    put_up(&mut w, HOUSE, t);
    // The only room is used; a second house has none.
    assert_eq!(spot_for(&w, ME, HOUSE, &stats, near, &[]), None);
}

/// Short of Grub, the answer is `None`: the simulation's `may_lay` refuses
/// on Price, so no spot can be laid.
#[test]
fn nothing_when_the_player_cannot_afford_it() {
    let mut w = empty_world(|_, _| false, &two_seeps(), u32::from(HOUSE_COST) - 1);
    put_up(&mut w, HALL, (i32::from(HALL_AT.0), i32::from(HALL_AT.1)));
    assert_eq!(w.may_lay(ME, HOUSE), Err(Refusal::Price));
    assert_eq!(
        spot_for(&w, ME, HOUSE, &house_stats(), hall_centre(), &[]),
        None
    );
}

/// With exactly the price in the store the first house is found, and
/// after it is paid for the next is `None`.
#[test]
fn nothing_once_the_last_grub_is_spent() {
    let mut w = empty_world(|_, _| false, &two_seeps(), u32::from(HOUSE_COST));
    put_up(&mut w, HALL, (i32::from(HALL_AT.0), i32::from(HALL_AT.1)));
    let worker = spawn_worker(&mut w, 6, 6);
    let stats = house_stats();
    let keep = lanes_to_nodes(&w);
    let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep).expect("exactly affordable");
    assert!(order(
        &mut w,
        Command::Build {
            regiment: worker,
            building: HOUSE,
            tile: (tile.0 as i16, tile.1 as i16),
            turn: 0,
        }
    ));
    assert_eq!(spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep), None);
}

/// A player with no hall can't lay a house (tier): `None`.
#[test]
fn nothing_for_a_player_with_no_hall() {
    let w = empty_world(|_, _| false, &two_seeps(), 2000);
    assert_eq!(w.may_lay(ME, HOUSE), Err(Refusal::Tier));
    assert_eq!(
        spot_for(&w, ME, HOUSE, &house_stats(), hall_centre(), &[]),
        None
    );
}

/// A type the table doesn't have is `None`, not a panic. (`stats` says how
/// big it is; the simulation refuses the unknown id.)
#[test]
fn an_unknown_building_is_none() {
    let (w, _) = base_world();
    assert_eq!(
        spot_for(&w, ME, 99, &house_stats(), hall_centre(), &[]),
        None
    );
}

/// A world with no Conquest at all (no structures list): `None`.
#[test]
fn a_world_without_conquest_is_none() {
    let w = World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities { regiments: 8 },
        units: &DRILL_UNITS,
        map_tiles: 0,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    assert_eq!(
        spot_for(&w, ME, HOUSE, &house_stats(), Vec2::new(10.0, 10.0), &[]),
        None
    );
}

/// The real test of "never boxed in": after eight houses a worker sent from
/// the far corner still reaches the seep, fills up and brings Grub home.
#[test]
fn workers_still_deliver_after_eight_houses() {
    let (mut w, worker) = base_world();
    let stats = house_stats();
    for n in 0..8 {
        let keep = lanes_to_nodes(&w);
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &keep)
            .unwrap_or_else(|| panic!("no room for house {n}"));
        put_up(&mut w, HOUSE, tile);
    }
    let before = w.bank(ME).unwrap().grub;
    assert!(order(
        &mut w,
        Command::Gather {
            regiment: worker,
            node: 0
        }
    ));
    for _ in 0..120 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert!(
        w.bank(ME).unwrap().grub > before,
        "no Grub came home in two minutes"
    );
}

/// Even without lanes, houses keep [`WIDE_GAP`] from the hall, so they
/// never wall it in.
#[test]
fn even_without_lanes_the_hall_stays_open() {
    let (mut w, _) = base_world();
    let stats = house_stats();
    for n in 0..8 {
        let tile = spot_for(&w, ME, HOUSE, &stats, hall_centre(), &[])
            .unwrap_or_else(|| panic!("no room for house {n}"));
        put_up(&mut w, HOUSE, tile);
    }
    let walled = node_centres(&w)
        .iter()
        .any(|&n| !node_reaches_hall(&w, n, hall_foot(), 2.0));
    assert!(!walled, "houses walled the hall in");
}

/// A building something comes out of keeps ground a regiment can walk
/// round it. A regiment's centre keeps 3 half-tile cells from anything
/// blocked (`nav::CLEARANCE`), so a corridor has to be 3 tiles wide to
/// have any walkable cell: houses may stand [`GAP`] apart (one wall to the
/// pathfinder, with the lanes keeping the base open), anything else keeps
/// [`WIDE_GAP`].
#[test]
fn a_gap_round_a_production_building_can_be_walked() {
    let (mut w, _) = base_world();
    let stats = house_stats();
    let first = spot_for(&w, ME, HOUSE, &stats, Vec2::new(20.0, 20.0), &[]).unwrap();
    put_up(&mut w, HOUSE, first);
    // Ask for a spot just beside it for a building that trains regiments.
    let barracks = BuildingStats {
        role: Role::Production,
        ..stats
    };
    let near = Vec2::new(
        first.0 as f32 + 3.0 + GAP as f32 + 1.5,
        first.1 as f32 + 1.5,
    );
    let second = spot_for(&w, ME, HOUSE, &barracks, near, &[]).unwrap();
    let (a, b) = (foot_at(first, &stats), foot_at(second, &stats));
    assert!(!overlaps(a, b, WIDE_GAP - 1));
    put_up(&mut w, HOUSE, second);
    // Cells between the two footprints, along the rows they share.
    let (x0, x1) = (a[0].min(b[0]) + 3, a[0].max(b[0]));
    let (y0, y1) = (a[1].max(b[1]), (a[1] + 3).min(b[1] + 3));
    let open = (x0 * CELLS_PER_TILE..x1 * CELLS_PER_TILE)
        .any(|cx| (y0 * CELLS_PER_TILE..y1 * CELLS_PER_TILE).any(|cy| cell_walkable(&w, cx, cy)))
        || y0 >= y1
        || x0 >= x1;
    assert!(open, "no walkable ground between {a:?} and {b:?}");
}
