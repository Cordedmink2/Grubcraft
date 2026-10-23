//! The ground as a mind knows it: worked out once when the match loads,
//! from the same walkable-ground grid the simulation routes on, and the
//! map's heights when the game has them.
//!
//! It answers "can a regiment stand here?", "is this blocked?", "is this
//! water?", "how high is it?" and "is it wooded?". What a battle line makes
//! of those (flank anchors, high ground, fords) is worked out in
//! [`crate::ground`]; where to hide, in the Captain's ambush plans.

use gc_sim::nav::{BLOCKED, CELLS_PER_TILE, CLEARANCE};
use gc_sim::{Cover, Growth};
use glam::Vec2;

#[derive(Clone)]
pub struct Terrain {
    map_tiles: u32,
    /// Cells per side; zero on an open plane.
    side: i32,
    cost: Vec<u8>,
    /// Chessboard distance from each cell to blocked ground, as the sim
    /// measures it: a regiment's centre needs [`CLEARANCE`]. Only whether
    /// it reaches [`CLEARANCE`] is read, so near cleared thicket it is kept
    /// only that far (see [`Terrain::follow`]).
    clear: Vec<u8>,
    /// Ground height at each tile's centre, tiles, row by row; empty when
    /// the map has no heights (tests' open planes).
    height: Vec<f32>,
    /// Ground lower than this is under water: deep water is blocked, the
    /// rest is a ford.
    water_level: f32,
    /// Woods and tall grass per tile, row by row, as `gc_sim::Ground::cover`;
    /// empty when the map has none.
    cover: Vec<u8>,
    /// What grows on each tile, row by row, as `gc_sim::Ground::growth`;
    /// empty when the map has none. Thicket is blocked in `cost`.
    growth: Vec<u8>,
    /// The ground's costs as they are under the thicket, for when it is
    /// cleared; empty when the map has none.
    under: Vec<u8>,
    /// The simulation's ground digest when the minds last looked
    /// (`gc_sim::Terrain::changes`).
    seen: u64,
}

impl Terrain {
    /// An open plane `map_tiles` across (zero: unbounded), as tests use.
    pub fn open(map_tiles: u32) -> Terrain {
        Terrain {
            map_tiles,
            side: 0,
            cost: Vec::new(),
            clear: Vec::new(),
            height: Vec::new(),
            water_level: f32::MIN,
            cover: Vec::new(),
            growth: Vec::new(),
            under: Vec::new(),
            seen: 0,
        }
    }

    /// From the match's nav costs (`MatchSetup::nav`); empty means open.
    pub fn new(nav: &[u8], map_tiles: u32) -> Terrain {
        let side = map_tiles as i32 * CELLS_PER_TILE;
        if nav.is_empty() || nav.len() != (side * side) as usize {
            return Terrain::open(map_tiles);
        }
        let mut t = Terrain {
            map_tiles,
            side,
            cost: nav.to_vec(),
            clear: vec![0; nav.len()],
            height: Vec::new(),
            water_level: f32::MIN,
            cover: Vec::new(),
            growth: Vec::new(),
            under: Vec::new(),
            seen: 0,
        };
        t.measure_clearance();
        t
    }

    /// Add the map's heights: `heights` has one per tile, row by row, taken
    /// at each tile's centre, in tiles; ground below `water_level` is water.
    /// Ignored if there isn't one per tile.
    pub fn with_heights(mut self, heights: Vec<f32>, water_level: f32) -> Terrain {
        let n = self.map_tiles as usize;
        if heights.len() == n * n && n > 0 {
            self.height = heights;
            self.water_level = water_level;
        }
        self
    }

    /// Add the map's woods and tall grass, one byte per tile as the
    /// simulation has them (`gc_sim::Ground::cover`). Ignored if there isn't
    /// one per tile.
    pub fn with_cover(mut self, cover: Vec<u8>) -> Terrain {
        let n = self.map_tiles as usize;
        if cover.len() == n * n && n > 0 {
            // Until told otherwise, woods grow where the cover is woods, as
            // the simulation has it (so clearing them is followed too).
            if self.growth.is_empty() {
                self.growth = (cover.iter())
                    .map(|&c| match Cover::from_u8(c) {
                        Cover::Wood => Growth::Woods as u8,
                        _ => Growth::Open as u8,
                    })
                    .collect();
            }
            self.cover = cover;
        }
        self
    }

    /// Add what grows on each tile, one byte per tile as the simulation has
    /// it (`gc_sim::Ground::growth`): thicket is blocked, as no regiment
    /// marches through it, until it is cleared (see [`Terrain::follow`]).
    /// Ignored if there isn't one per tile, or on an open plane.
    pub fn with_growth(mut self, growth: Vec<u8>) -> Terrain {
        let n = self.map_tiles as usize;
        if growth.len() != n * n || n == 0 || self.side == 0 {
            return self;
        }
        // Read as the simulation reads it: anything unknown grows nothing.
        let growth: Vec<u8> = growth.iter().map(|&g| Growth::from_u8(g) as u8).collect();
        let thicket = |g: &u8| *g == Growth::Thicket as u8;
        if growth.iter().any(thicket) {
            self.under.clone_from(&self.cost);
            for (t, _) in growth.iter().enumerate().filter(|(_, g)| thicket(g)) {
                self.set_tile(t, BLOCKED);
            }
            self.remeasure();
        }
        self.growth = growth;
        self
    }

    /// Catch up with the simulation's ground, if it has changed since the
    /// last look: cleared tiles lose their cover, and cleared thicket can
    /// be walked again.
    pub fn follow(&mut self, ground: &gc_sim::Terrain) {
        if self.seen == ground.changes() {
            return;
        }
        self.seen = ground.changes();
        let n = self.map_tiles as i32;
        for t in 0..self.growth.len() {
            let was = self.growth[t];
            if was == Growth::Open as u8 {
                continue;
            }
            let now = ground.growth(t as i32 % n, t as i32 / n) as u8;
            if now == was {
                continue;
            }
            self.growth[t] = now;
            if let Some(c) = self.cover.get_mut(t) {
                *c = Cover::Open as u8;
            }
            if was == Growth::Thicket as u8 && !self.under.is_empty() {
                self.restore_tile(t);
            }
        }
    }

    /// Whether the minds' ground lags the simulation's.
    pub fn stale(&self, ground: &gc_sim::Terrain) -> bool {
        !self.growth.is_empty() && self.seen != ground.changes()
    }

    /// The first cell of tile `t`, as (column, row).
    fn tile_cell(&self, t: usize) -> (i32, i32) {
        let n = self.map_tiles as usize;
        (
            (t % n) as i32 * CELLS_PER_TILE,
            (t / n) as i32 * CELLS_PER_TILE,
        )
    }

    fn set_tile(&mut self, t: usize, cost: u8) {
        let ((x, y), side) = (self.tile_cell(t), self.side);
        for cy in y..y + CELLS_PER_TILE {
            let row = (cy * side + x) as usize;
            self.cost[row..row + CELLS_PER_TILE as usize].fill(cost);
        }
    }

    /// Tile `t`'s ground as it is under the thicket, and the clearance
    /// [`CLEARANCE`] cells round it, which is all that can change.
    fn restore_tile(&mut self, t: usize) {
        let ((x, y), side) = (self.tile_cell(t), self.side);
        for cy in y..y + CELLS_PER_TILE {
            let row = (cy * side + x) as usize..(cy * side + x + CELLS_PER_TILE) as usize;
            self.cost[row.clone()].copy_from_slice(&self.under[row]);
        }
        let reach = i32::from(CLEARANCE);
        for cy in (y - reach).max(0)..(y + CELLS_PER_TILE + reach).min(side) {
            for cx in (x - reach).max(0)..(x + CELLS_PER_TILE + reach).min(side) {
                self.clear[(cy * side + cx) as usize] = self.clearance_at(cx, cy);
            }
        }
    }

    /// The clearance of cell `(x, y)`, up to [`CLEARANCE`]: 0 on blocked
    /// ground, else how far blocked ground or the map's edge is.
    fn clearance_at(&self, x: i32, y: i32) -> u8 {
        let side = self.side;
        let blocked = |cx: i32, cy: i32| {
            cx < 0
                || cy < 0
                || cx >= side
                || cy >= side
                || self.cost[(cy * side + cx) as usize] == BLOCKED
        };
        if blocked(x, y) {
            return 0;
        }
        (1..=i32::from(CLEARANCE))
            .find(|&r| {
                (-r..=r).any(|d| {
                    blocked(x + d, y - r)
                        || blocked(x + d, y + r)
                        || blocked(x - r, y + d)
                        || blocked(x + r, y + d)
                })
            })
            .map_or(CLEARANCE, |r| r as u8)
    }

    /// Measure every cell's clearance afresh.
    fn remeasure(&mut self) {
        self.clear.fill(0);
        self.measure_clearance();
    }

    /// Whether the map has woods or tall grass anywhere.
    pub fn has_cover(&self) -> bool {
        !self.cover.is_empty()
    }

    /// The cover at `p`: open off the map and where the map has none.
    pub fn cover(&self, p: Vec2) -> Cover {
        let n = self.map_tiles as i32;
        if self.cover.is_empty() || p.x < 0.0 || p.y < 0.0 {
            return Cover::Open;
        }
        let (x, y) = (p.x as i32, p.y as i32);
        if x >= n || y >= n {
            return Cover::Open;
        }
        Cover::from_u8(self.cover[(y * n + x) as usize])
    }

    /// Ground height at `p`, tiles (0 where the map has none).
    pub fn height(&self, p: Vec2) -> f32 {
        if self.height.is_empty() {
            return 0.0;
        }
        let n = self.map_tiles as i32;
        let x = (p.x.floor() as i32).clamp(0, n - 1);
        let y = (p.y.floor() as i32).clamp(0, n - 1);
        self.height[(y * n + x) as usize]
    }

    /// Whether `p` is off the map or on ground no regiment crosses (a cliff
    /// or deep water).
    pub fn blocked(&self, p: Vec2) -> bool {
        !self.inside(p, 0.0) || self.cell(p).is_some_and(|c| self.cost[c] == BLOCKED)
    }

    /// Whether `p` is under water, deep or not.
    pub fn water(&self, p: Vec2) -> bool {
        !self.height.is_empty() && self.height(p) < self.water_level
    }

    /// Whether `p` is water a regiment can wade: a ford or shallows.
    pub fn wet(&self, p: Vec2) -> bool {
        self.water(p) && !self.blocked(p)
    }

    pub fn map_tiles(&self) -> u32 {
        self.map_tiles
    }

    fn cell(&self, p: Vec2) -> Option<usize> {
        if self.side == 0 {
            return None;
        }
        let x = (p.x * CELLS_PER_TILE as f32).floor() as i32;
        let y = (p.y * CELLS_PER_TILE as f32).floor() as i32;
        let (x, y) = (x.clamp(0, self.side - 1), y.clamp(0, self.side - 1));
        Some((y * self.side + x) as usize)
    }

    /// Whether a regiment's centre can stand at `p`.
    pub fn walkable(&self, p: Vec2) -> bool {
        if !self.inside(p, 0.0) {
            return false;
        }
        self.cell(p).is_none_or(|c| self.clear[c] >= CLEARANCE)
    }

    /// Ground cost at `p`: 1 open, higher slow, 0 blocked.
    pub fn cost(&self, p: Vec2) -> u8 {
        self.cell(p).map_or(1, |c| self.cost[c])
    }

    /// Whether `p` is on the map with `margin` tiles to spare.
    pub fn inside(&self, p: Vec2, margin: f32) -> bool {
        if self.map_tiles == 0 {
            return true;
        }
        let hi = self.map_tiles as f32 - margin;
        p.x >= margin && p.y >= margin && p.x <= hi && p.y <= hi
    }

    /// `p` moved onto the map, `margin` tiles in from the edge.
    pub fn clamp(&self, p: Vec2, margin: f32) -> Vec2 {
        if self.map_tiles == 0 {
            return p;
        }
        let hi = self.map_tiles as f32 - margin;
        Vec2::new(p.x.clamp(margin, hi), p.y.clamp(margin, hi))
    }

    /// The nearest place within `reach` tiles of `p` where a regiment can
    /// stand, searching outward in rings half a tile apart.
    pub fn nearest_walkable(&self, p: Vec2, reach: f32) -> Option<Vec2> {
        let p = self.clamp(p, 2.0);
        if self.walkable(p) {
            return Some(p);
        }
        let rings = (reach * 2.0) as i32;
        for ring in 1..=rings {
            let r = ring as f32 * 0.5;
            // Points around the ring about half a tile apart.
            let steps = (r * 12.0) as i32;
            for k in 0..steps {
                let a = k as f32 / steps as f32;
                // A square ring, walked without trigonometry so every
                // platform finds the same point.
                let q = Vec2::from(square_ring(a)) * r;
                let c = p + q;
                if self.walkable(c) {
                    return Some(c);
                }
            }
        }
        None
    }

    fn measure_clearance(&mut self) {
        let side = self.side;
        let at = |clear: &[u8], x: i32, y: i32| -> u16 {
            if x < 0 || y < 0 || x >= side || y >= side {
                0
            } else {
                u16::from(clear[(y * side + x) as usize])
            }
        };
        for y in 0..side {
            for x in 0..side {
                let i = (y * side + x) as usize;
                if self.cost[i] == BLOCKED {
                    continue;
                }
                let c = &self.clear;
                let d = at(c, x - 1, y)
                    .min(at(c, x - 1, y - 1))
                    .min(at(c, x, y - 1))
                    .min(at(c, x + 1, y - 1));
                self.clear[i] = (d + 1).min(255) as u8;
            }
        }
        for y in (0..side).rev() {
            for x in (0..side).rev() {
                let i = (y * side + x) as usize;
                if self.clear[i] == 0 {
                    continue;
                }
                let c = &self.clear;
                let d = at(c, x + 1, y)
                    .min(at(c, x + 1, y + 1))
                    .min(at(c, x, y + 1))
                    .min(at(c, x - 1, y + 1));
                self.clear[i] = self.clear[i].min((d + 1).min(255) as u8);
            }
        }
    }
}

/// A point on the unit square's outline (corners at ±1), `a` of the way round.
fn square_ring(a: f32) -> [f32; 2] {
    let t = a * 8.0;
    match t as i32 {
        0 | 7 => [1.0, if t < 1.0 { t } else { t - 8.0 }],
        1 | 2 => [1.0 - (t - 1.0), 1.0],
        3 | 4 => [-1.0, 1.0 - (t - 3.0)],
        _ => [-1.0 + (t - 5.0), -1.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_sims_idea_of_walkable() {
        use gc_fixed::FxVec2;
        use gc_sim::scenario::{DRILL_UNITS, ribs_map};
        use gc_sim::{Capacities, MatchSetup, World};
        let nav = ribs_map(256);
        let world = World::new(&MatchSetup {
            seed: 1,
            capacities: Capacities { regiments: 8 },
            units: &DRILL_UNITS,
            map_tiles: 256,
            nav: &nav,
            teams: &[],
            ground: gc_sim::Ground::FLAT,
            champions: false,
        });
        let t = Terrain::new(&nav, 256);
        for y in (3..253).step_by(3) {
            for x in (3..253).step_by(3) {
                let p = Vec2::new(x as f32 + 0.25, y as f32 + 0.25);
                let sim = world.nav().walkable(
                    0,
                    FxVec2::new(
                        gc_fixed::Fx::from_raw((p.x * 65536.0) as i32),
                        gc_fixed::Fx::from_raw((p.y * 65536.0) as i32),
                    ),
                );
                assert_eq!(t.walkable(p), sim, "at {p}");
            }
        }
        // The lake in the middle is not; somewhere near its shore is.
        let centre = Vec2::splat(128.0);
        assert!(!t.walkable(centre));
        let shore = t.nearest_walkable(centre, 30.0).unwrap();
        assert!(t.walkable(shore));
        assert!(shore.distance(centre) > 10.0);
    }

    #[test]
    fn square_ring_goes_all_the_way_round() {
        let pts: Vec<[f32; 2]> = (0..16).map(|k| square_ring(k as f32 / 16.0)).collect();
        assert!(
            pts.iter().all(|p| p[0].abs().max(p[1].abs()) == 1.0),
            "{pts:?}"
        );
        assert!(pts.iter().any(|p| p[0] < 0.0) && pts.iter().any(|p| p[1] < 0.0));
    }
}
