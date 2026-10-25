//! The ground as a mind knows it: worked out once when the match loads,
//! from the same walkable-ground grid the simulation routes on, and the
//! map's heights when the game has them.
//!
//! It answers "can a regiment stand here?", "is this blocked?", "is this
//! water?", "how high is it?" and "is it wooded?". What a battle line makes
//! of those (flank anchors, high ground, fords) is worked out in
//! [`crate::ground`]; where to hide, in the Captain's ambush plans.

use gc_sim::nav::{BLOCKED, CELLS_PER_TILE, CLEARANCE, FORD, OPEN};
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
    /// Which tiles are soft underfoot, row by row: bog, and mud the rain
    /// has soaked. Empty when none is.
    soft: Vec<bool>,
    /// The simulation's soaked count and rubble moves when the minds last
    /// read the soft tiles (`gc_sim::Terrain::soaked`, `rubble_moves`);
    /// `None` before the first look.
    soaked: Option<(u32, u32)>,
    /// The water is frozen over (`gc_sim::Terrain::frozen`): fords and
    /// shallows are dry ground till the thaw; deep water stays blocked.
    frozen: bool,
    /// The water as the minds last read it (see [`Terrain::read_water`]).
    water: Water,
    /// The tiles the water shuts and opens (fords and summer fords), with
    /// their cells' costs as the match began; empty till it first does.
    wet_tiles: Vec<WetTile>,
    /// The ford tiles the water has shut, as (column, row).
    shut: Vec<(i32, i32)>,
}

/// What the rivers are doing, as far as where a mind may wade goes
/// (docs/38-living-waterways.md, stage 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Water {
    /// Every ford is out: a flood or the spring melt has shut them, or
    /// the water is rising (or still high) towards it.
    pub keep_off: bool,
    /// The summer fords are open.
    pub summer: bool,
    /// A flash flood is coming down a river, or has shut it: the first of
    /// its fords, to tell one wave's river from another's.
    pub wave: Option<(i32, i32)>,
}

impl Water {
    /// The rivers of `world` now (cheap: read every tick).
    pub fn of(world: &gc_sim::World) -> Water {
        let flood = world.flood();
        Water {
            keep_off: flood.closed() || world.terrain().rise() >= gc_sim::flood::FLOOD_RISE / 2,
            summer: flood.exposed(),
            wave: flood.flash().threatened().first().copied(),
        }
    }
}

/// Cells a tile has.
const TILE_CELLS: usize = (CELLS_PER_TILE * CELLS_PER_TILE) as usize;

/// A tile the water shuts and opens.
#[derive(Clone, Debug)]
struct WetTile {
    /// Row by row.
    t: usize,
    /// A ford (some of it walkable as the map began), a summer ford (some
    /// of it deep), or both.
    ford: bool,
    shallow: bool,
    /// The water is shutting it, or about to.
    out: bool,
    /// Its cells' costs as the match began, row by row.
    base: [u8; TILE_CELLS],
}

/// What changes which tiles are soft: the rain's soaking and rubble.
fn soft_stamp(ground: &gc_sim::Terrain) -> (u32, u32) {
    (ground.soaked(), ground.rubble_moves())
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
            soft: Vec::new(),
            soaked: None,
            frozen: false,
            water: Water::default(),
            wet_tiles: Vec::new(),
            shut: Vec::new(),
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
            soft: Vec::new(),
            soaked: None,
            frozen: false,
            water: Water::default(),
            wet_tiles: Vec::new(),
            shut: Vec::new(),
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
    /// The soft tiles are read again when rain soaks or dries any.
    pub fn follow(&mut self, ground: &gc_sim::Terrain) {
        self.frozen = ground.frozen();
        if self.soaked != Some(soft_stamp(ground)) {
            self.soaked = Some(soft_stamp(ground));
            self.read_soft(ground);
        }
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
                if let Some(i) = self.wet_tiles.iter().position(|w| w.t == t) {
                    self.wet_tile(i);
                }
            }
        }
    }

    /// The rivers as the minds last read them.
    pub fn seen_water(&self) -> Water {
        self.water
    }

    /// Read the rivers of `world`, which are now `water`: fords out of
    /// the water's reach are blocked, and open summer fords can be waded,
    /// as the simulation routes. A mind then neither plans to wade nor
    /// waits for an enemy to wade where the water has shut, or is about to.
    pub fn read_water(&mut self, world: &gc_sim::World, water: Water) {
        let flood = world.flood();
        let fords: Vec<_> = flood.fords().collect();
        let shallows: Vec<_> = flood.shallows().collect();
        self.set_water(water, &fords, &shallows, flood.flash().threatened());
    }

    /// The water now `water`, over the `fords` and summer fords
    /// (`shallows`), as (column, row), with a flash flood shutting the
    /// `wave` tiles (as (row, column), sorted).
    fn set_water(
        &mut self,
        water: Water,
        fords: &[(i32, i32)],
        shallows: &[(i32, i32)],
        wave: &[(i32, i32)],
    ) {
        let was = core::mem::replace(&mut self.water, water);
        if was == water || self.side == 0 {
            return;
        }
        let n = self.map_tiles as i32;
        if self.wet_tiles.is_empty() {
            self.find_wet_tiles(fords, shallows);
        }
        self.shut.clear();
        for i in 0..self.wet_tiles.len() {
            let w = &mut self.wet_tiles[i];
            let (x, y) = (w.t as i32 % n, w.t as i32 / n);
            w.out = water.keep_off || wave.binary_search(&(y, x)).is_ok();
            // A summer ford going deep again shuts as surely as a flood.
            let ends = w.shallow && was.summer && !water.summer;
            if (w.out && (w.ford || water.summer)) || ends {
                self.shut.push((x, y));
            }
            self.wet_tile(i);
        }
    }

    /// The fords and summer fords, with their cells as the match began
    /// (under any thicket).
    fn find_wet_tiles(&mut self, fords: &[(i32, i32)], shallows: &[(i32, i32)]) {
        let n = self.map_tiles as i32;
        let mut tiles: Vec<(usize, bool)> = fords
            .iter()
            .map(|&(x, y)| ((y * n + x) as usize, true))
            .collect();
        tiles.extend(shallows.iter().map(|&(x, y)| ((y * n + x) as usize, false)));
        tiles.sort_unstable();
        let begun = if self.under.is_empty() {
            &self.cost
        } else {
            &self.under
        };
        let mut wet: Vec<WetTile> = Vec::new();
        for (t, ford) in tiles {
            if let Some(w) = wet.last_mut().filter(|w| w.t == t) {
                w.ford |= ford;
                w.shallow |= !ford;
                continue;
            }
            let ((x, y), side) = (self.tile_cell(t), self.side);
            let mut base = [0; TILE_CELLS];
            for (k, b) in base.iter_mut().enumerate() {
                let (cx, cy) = (x + k as i32 % CELLS_PER_TILE, y + k as i32 / CELLS_PER_TILE);
                *b = begun[(cy * side + cx) as usize];
            }
            wet.push(WetTile {
                t,
                ford,
                shallow: !ford,
                out: false,
                base,
            });
        }
        self.wet_tiles = wet;
    }

    /// Wet tile `i`'s cells as the water has them, and the clearance round
    /// it if any changed: walkable cells of a ford out of the water's reach
    /// are blocked, and deep cells of an open summer ford can be waded.
    /// Thicket still standing is left blocked.
    fn wet_tile(&mut self, i: usize) {
        let w = &self.wet_tiles[i];
        if self.growth.get(w.t) == Some(&(Growth::Thicket as u8)) {
            return;
        }
        let open_deep = w.shallow && self.water.summer && !w.out;
        let ((x, y), side) = (self.tile_cell(w.t), self.side);
        let mut changed = false;
        for (k, &b) in w.base.iter().enumerate() {
            let cost = match b {
                BLOCKED if open_deep => FORD,
                BLOCKED => BLOCKED,
                _ if w.ford && w.out => BLOCKED,
                b => b,
            };
            let (cx, cy) = (x + k as i32 % CELLS_PER_TILE, y + k as i32 / CELLS_PER_TILE);
            let c = &mut self.cost[(cy * side + cx) as usize];
            changed |= *c != cost;
            *c = cost;
        }
        if changed {
            self.reclear(w.t);
        }
    }

    /// Whether the water has shut a ford within `reach` tiles of `p`.
    pub fn ford_shut_near(&self, p: Vec2, reach: f32) -> bool {
        let r2 = reach * reach;
        self.shut
            .iter()
            .any(|&(x, y)| Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance_squared(p) <= r2)
    }

    /// Whether the minds' ground lags the simulation's.
    pub fn stale(&self, ground: &gc_sim::Terrain) -> bool {
        (!self.growth.is_empty() && self.seen != ground.changes())
            || self.soaked != Some(soft_stamp(ground))
            || self.frozen != ground.frozen()
    }

    /// Read which tiles are soft from the simulation's ground.
    fn read_soft(&mut self, ground: &gc_sim::Terrain) {
        let n = self.map_tiles as i32;
        self.soft.clear();
        let centre = |t: i32| {
            crate::perception::to_fx_vec(Vec2::new((t % n) as f32 + 0.5, (t / n) as f32 + 0.5))
        };
        if (0..n * n).any(|t| ground.soft(centre(t))) {
            self.soft.extend((0..n * n).map(|t| ground.soft(centre(t))));
        }
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
        self.reclear(t);
    }

    /// The clearance of the cells within [`CLEARANCE`] of tile `t`, which
    /// is all a change to its ground can change.
    fn reclear(&mut self, t: usize) {
        let ((x, y), side) = (self.tile_cell(t), self.side);
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

    /// Whether `p` is under water, deep or not; frozen over, only deep
    /// water counts (the ice on the fords is walked as dry ground).
    pub fn water(&self, p: Vec2) -> bool {
        !self.height.is_empty()
            && self.height(p) < self.water_level
            && !(self.frozen && !self.blocked(p))
    }

    /// Whether `p` is soft underfoot: bog, or mud the rain has soaked (slow
    /// going, and no charges out of it or into it).
    pub fn soft(&self, p: Vec2) -> bool {
        let n = self.map_tiles as i32;
        let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
        !self.soft.is_empty()
            && (0..n).contains(&x)
            && (0..n).contains(&y)
            && self.soft[(y * n + x) as usize]
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

    /// Whether water (or cliffs) cut the open ground nearest `home` off
    /// from that nearest every one of `others` (each within a few tiles),
    /// the ground's own way (buildings aside): reaching any of them needs
    /// a bridge. Never on an open plane.
    pub fn cut_off(&self, home: Vec2, others: impl IntoIterator<Item = Vec2>) -> bool {
        if self.side == 0 {
            return false;
        }
        let Some(start) = self.open_near(home) else {
            return false;
        };
        let side = self.side as usize;
        let mut seen = vec![false; self.cost.len()];
        let mut todo = vec![start];
        seen[start] = true;
        while let Some(c) = todo.pop() {
            let (x, y) = (c % side, c / side);
            let next = [
                (x > 0).then(|| c - 1),
                (x + 1 < side).then(|| c + 1),
                (y > 0).then(|| c - side),
                (y + 1 < side).then(|| c + side),
            ];
            for n in next.into_iter().flatten() {
                if !seen[n] && self.cost[n] != BLOCKED {
                    seen[n] = true;
                    todo.push(n);
                }
            }
        }
        others
            .into_iter()
            .all(|p| self.open_near(p).is_none_or(|c| !seen[c]))
    }

    /// The open cell nearest `p` within 4 tiles, if any (any cell on an
    /// open plane).
    fn open_near(&self, p: Vec2) -> Option<usize> {
        let c = self.cell(p)?;
        if self.cost[c] != BLOCKED {
            return Some(c);
        }
        let side = self.side;
        let (x, y) = (c as i32 % side, c as i32 / side);
        let reach = 4 * CELLS_PER_TILE;
        (1..=reach).find_map(|r| {
            (-r..=r)
                .flat_map(|d| {
                    [
                        (x + d, y - r),
                        (x + d, y + r),
                        (x - r, y + d),
                        (x + r, y + d),
                    ]
                })
                .filter(|&(cx, cy)| cx >= 0 && cy >= 0 && cx < side && cy < side)
                .map(|(cx, cy)| (cy * side + cx) as usize)
                .find(|&n| self.cost[n] != BLOCKED)
        })
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
        match self.cell(p).map_or(OPEN, |c| self.cost[c]) {
            FORD if self.frozen => OPEN,
            c => c,
        }
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

    /// A 16-tile map: tile (5, 5) half ford and half deep, tile (6, 5)
    /// wholly deep, the rest open.
    fn river() -> Terrain {
        let side = 16 * CELLS_PER_TILE;
        let nav: Vec<u8> = (0..side * side)
            .map(|c| {
                let (cx, cy) = (c % side, c / side);
                let (x, y) = (cx / CELLS_PER_TILE, cy / CELLS_PER_TILE);
                match (x, y) {
                    (5, 5) if cx % CELLS_PER_TILE == 0 => BLOCKED,
                    (5, 5) => FORD,
                    (6, 5) => BLOCKED,
                    _ => OPEN,
                }
            })
            .collect();
        Terrain::new(&nav, 16)
    }

    #[test]
    fn the_water_shuts_and_opens_fords_cell_by_cell() {
        let mut t = river();
        let (ford, deep, bar) = (
            Vec2::new(5.9, 5.5),
            Vec2::new(5.05, 5.5),
            Vec2::new(6.5, 5.5),
        );
        let (fords, shallows) = ([(5, 5)], [(5, 5), (6, 5)]);
        let flood = Water {
            keep_off: true,
            ..Water::default()
        };
        t.set_water(flood, &fords, &shallows, &[]);
        assert_eq!(t.cost(ford), BLOCKED);
        assert!(t.ford_shut_near(ford, 1.0));
        t.set_water(Water::default(), &fords, &shallows, &[]);
        assert_eq!(
            (t.cost(ford), t.cost(deep)),
            (FORD, BLOCKED),
            "deep cells stay deep"
        );
        let summer = Water {
            summer: true,
            ..Water::default()
        };
        t.set_water(summer, &fords, &shallows, &[]);
        assert_eq!(
            (t.cost(deep), t.cost(bar)),
            (FORD, FORD),
            "summer fords open"
        );
        t.set_water(Water::default(), &fords, &shallows, &[]);
        assert_eq!(
            (t.cost(ford), t.cost(deep), t.cost(bar)),
            (FORD, BLOCKED, BLOCKED)
        );
        assert!(
            t.ford_shut_near(bar, 1.0),
            "a summer ford going deep is shut"
        );
        let wave = Water {
            wave: Some((5, 6)),
            summer: true,
            ..Water::default()
        };
        t.set_water(wave, &fords, &shallows, &[(5, 6)]);
        assert_eq!(
            (t.cost(ford), t.cost(bar)),
            (FORD, BLOCKED),
            "only the wave's tiles"
        );
    }

    #[test]
    fn water_cuts_the_land_till_a_ford_joins_it() {
        // 32 tiles across, a deep river across rows 30 to 33 (cells).
        let side = 64usize;
        let mut nav = vec![1u8; side * side];
        for cy in 30..34 {
            nav[cy * side..(cy + 1) * side].fill(BLOCKED);
        }
        let (north, south) = (Vec2::new(5.0, 5.0), Vec2::new(25.0, 25.0));
        assert!(Terrain::new(&nav, 32).cut_off(north, [south]));
        for cy in 30..34 {
            nav[cy * side + 40] = FORD;
        }
        let t = Terrain::new(&nav, 32);
        assert!(!t.cut_off(north, [south]));
        // Frozen over, the ford costs as open ground.
        let ford = Vec2::new(20.25, 15.25);
        assert_eq!(t.cost(ford), FORD);
        let mut iced = t.clone();
        iced.frozen = true;
        assert_eq!(iced.cost(ford), OPEN);
    }

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
            sky: gc_sim::Sky::STILL,
            generals: true,
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
