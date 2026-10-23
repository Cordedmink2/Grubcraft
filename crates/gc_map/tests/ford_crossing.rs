//! Ten regiments crossing a river at a ford on a "Random: Ford" map: every
//! soldier stays on walkable ground, nobody gets stuck, and the blocks
//! don't pile up shoving through each other in the water.

use gc_fixed::{Fx, FxVec2};
use gc_map::{FieldMap, Map};
use gc_sim::formation::{self, GroupMember, GroupOrder};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World};

/// A mixed army: drill infantry, archers, cavalry and two monster blocks.
const ARMY: [u16; 10] = [0, 0, 4, 0, 2, 3, 0, 4, 2, 0];
/// How far south of the river the army starts, and north of it it ends, in
/// tiles.
const BACK: i32 = 26;
/// Ticks the army gets to cross (150 seconds).
const LIMIT: u32 = 20 * 150;

/// The drill types, plus a big, loosely spaced monster block like the
/// Deep Hive's.
fn units() -> Vec<UnitStats> {
    let mut u = DRILL_UNITS.to_vec();
    u.push(UnitStats {
        models: 16,
        files: 6,
        file_spacing: Fx::from_ratio(3, 2),
        rank_spacing: Fx::from_ratio(3, 2),
        speed: Fx::from_ratio(7, 2),
        ..DRILL_UNITS[1]
    });
    u
}

struct Field {
    map: Map,
    nav: Vec<u8>,
    /// Map edge in nav cells.
    side: i32,
}

impl Field {
    fn ford(seed: u64) -> Field {
        let map = FieldMap {
            choice: 2,
            tiles: 192,
            seed,
        }
        .make();
        let nav = map.sim_nav_costs();
        let side = map.tiles as i32 * CELLS_PER_TILE;
        Field { map, nav, side }
    }

    /// The river runs across the middle row of tiles.
    fn mid(&self) -> i32 {
        self.map.tiles as i32 / 2
    }

    fn world(&self, units: &[UnitStats]) -> World {
        World::new(&MatchSetup {
            seed: 1,
            capacities: Capacities { regiments: 64 },
            units,
            map_tiles: self.map.tiles,
            nav: &self.nav,
            teams: &[],
            ground: self.map.ground(),
            champions: false,
        })
    }

    /// Whether `p` is deep water, a cliff or off the map.
    fn blocked(&self, p: FxVec2) -> bool {
        let cell = |v: Fx| (i64::from(v.raw()) * i64::from(CELLS_PER_TILE)) >> 16;
        let (x, y) = (cell(p.x), cell(p.y));
        let s = i64::from(self.side);
        x < 0 || y < 0 || x >= s || y >= s || self.nav[(y * s + x) as usize] == BLOCKED
    }

    /// The middle tile of each crossing of the river along the middle row.
    fn fords(&self) -> Vec<i32> {
        let row = self.mid() * CELLS_PER_TILE;
        let mut out = Vec::new();
        let mut run: Option<i32> = None;
        for x in 0..=self.side {
            let open = x < self.side && self.nav[(row * self.side + x) as usize] != BLOCKED;
            match (open, run) {
                (true, None) => run = Some(x),
                (false, Some(a)) => {
                    if a > 0 && x < self.side {
                        out.push((a + x) / 2 / CELLS_PER_TILE);
                    }
                    run = None;
                }
                _ => {}
            }
        }
        out
    }
}

/// Soldier slots standing on blocked ground.
fn soldiers_off(field: &Field, w: &World, units: &[UnitStats]) -> u32 {
    let mut n = 0;
    for r in w.regiments.iter() {
        let (fs, rs) = units[r.unit as usize].spacing(r.formation);
        for k in 0..r.models {
            let local = formation::slot_offset(k, r.files, r.models, fs, rs);
            if field.blocked(formation::local_to_world(r.position, r.facing, local)) {
                n += 1;
            }
        }
    }
    n
}

fn spawn(w: &mut World, unit: u16, at: FxVec2) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit,
            position: at,
            facing: NORTH,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

#[derive(Debug, Default)]
struct Crossing {
    /// Soldier-ticks on blocked ground, and soldier-ticks in all.
    off: u64,
    total: u64,
    /// Regiments that had not arrived when time ran out.
    stuck: usize,
    /// Ticks until the last one arrived.
    ticks: u32,
    /// Regiment-ticks in disorder (from shoving through friends): before
    /// the river, in it, and past it.
    disordered: [u32; 3],
}

/// The army, in two rows of five south of ford number `ford`, is ordered
/// to a line as far north of it.
fn cross(seed: u64, ford: usize, army: &[u16]) -> Crossing {
    let field = Field::ford(seed);
    let units = units();
    let mut w = field.world(&units);
    let fords = field.fords();
    let x = fords[ford.min(fords.len() - 1)];
    let mid = field.mid();
    let handles: Vec<RegimentHandle> = army
        .iter()
        .enumerate()
        .map(|(k, &unit)| {
            let k = k as i32;
            let at = FxVec2::from_ints(x + (k % 5 - 2) * 12, mid + BACK + k / 5 * 10);
            spawn(&mut w, unit, at)
        })
        .collect();
    let mut members: Vec<GroupMember> = handles
        .iter()
        .map(|&h| {
            let r = w.regiments.get(h).unwrap();
            GroupMember::of(&r, &units[r.unit as usize])
        })
        .collect();
    let mut cmds = Vec::new();
    formation::group_move(
        &mut members,
        GroupOrder {
            target: FxVec2::from_ints(x, mid - BACK),
            facing: Some(NORTH),
            width: None,
        },
        &mut cmds,
    );
    let orders: Vec<PlayerCommand> = cmds
        .into_iter()
        .map(|command| PlayerCommand { player: 0, command })
        .collect();
    w.tick(&orders);
    let mut out = Crossing::default();
    for t in 0..LIMIT {
        w.tick(&[]);
        out.off += u64::from(soldiers_off(&field, &w, &units));
        out.total += w.regiments.iter().map(|r| u64::from(r.models)).sum::<u64>();
        let mut moving = 0;
        for &h in &handles {
            let r = w.regiments.get(h).unwrap();
            if r.disorder > 0 {
                // South of the river is "before" it: y grows southwards.
                let y = r.position.y.floor_int() - mid;
                out.disordered[2 - usize::from(y >= -8) - usize::from(y > 8)] += 1;
            }
            moving += usize::from(r.moving);
        }
        out.stuck = moving;
        out.ticks = t;
        if moving == 0 {
            break;
        }
    }
    out
}

#[test]
fn ten_regiments_cross_a_ford_on_firm_ground_without_piling_up() {
    let c = cross(3, 0, &ARMY);
    assert_eq!(c.stuck, 0, "regiments still marching: {c:?}");
    assert_eq!(c.off, 0, "soldier-ticks on deep water: {c:?}");
    assert!(c.ticks < 1200, "took too long to cross: {c:?}");
    assert!(c.disordered[1] < 1500, "piled up in the river: {c:?}");
}

#[test]
#[ignore = "a report: cargo test -p gc_map --release --test ford_crossing -- --ignored --nocapture"]
fn report_ford_crossings() {
    for seed in 1..=8 {
        let fords = Field::ford(seed).fords();
        for (ford, x) in fords.iter().enumerate() {
            let c = cross(seed, ford, &ARMY);
            let alone = cross(seed, ford, &ARMY[5..6]);
            println!(
                "seed {seed} ford {ford} (x {x}): off {:.2}% stuck {} ticks {} disordered {:?} alone {}",
                100.0 * c.off as f64 / c.total.max(1) as f64,
                c.stuck,
                c.ticks,
                c.disordered,
                alone.ticks,
            );
        }
    }
}
