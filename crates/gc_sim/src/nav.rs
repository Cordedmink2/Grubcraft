//! Pathfinding: where regiments can walk, and the routes they take.
//!
//! The map is a grid of half-tile cells, each with a cost: 0 is ground no
//! regiment can cross (cliffs, deep water), 1 is open, and higher costs are
//! slow going that routes avoid when they can. A regiment's centre may
//! stand on any cell that isn't blocked ([`CLEARANCE`]), so the walkable
//! ground is the ground drawn: a gap a tile wide between two buildings, or
//! between a cliff and the water, is a way through. "Open" below means a
//! cell a regiment's centre may stand on.
//!
//! The grid is cut into sectors of [`SECTOR`] x [`SECTOR`] cells. Where open
//! cells face each other across a sector edge there is a portal, and at load
//! every portal is linked to the others in its sector with the true walking
//! cost between them. A route is found in up to three steps:
//! 1. **Straight line:** if the goal is in plain sight, walk straight to it.
//! 2. **Portal search:** A* over the small portal graph, from the start's
//!    sector to the goal's.
//! 3. **Smoothing:** legs between portals that aren't straight lines are
//!    filled in cell by cell inside their sector, then the whole chain is
//!    pulled tight into a few waypoints the block marches between.
//!
//! A regiment's centre may go anywhere open, but its block is wider than
//! that: every route keeps a *berth* for it, the cells from its centre to
//! the edge of its block and a tile more (see [`Nav::plan`]). Cells closer
//! than the berth to blocked ground cost more to cross, so searches run
//! down the middle of a ford or a pass and swing wide round corners, and
//! the chain is never pulled tighter than the berth (or than the chain
//! itself went, through ground too narrow for it). Ground too narrow for
//! the whole block is where it narrows into a column (`Regiments::squeeze`).
//!
//! Searches are budgeted: requests queue up and each tick serves them in
//! order until [`WORK_PER_TICK`] units of work are spent (at least one request
//! a tick), so a hundred regiments ordered at once can't stall a tick.
//! Regiments standing still for their route are served before those that
//! keep walking the way they have meanwhile (the rest of a long route, or
//! one near ground that changed), so nobody stops for a search it needn't.

use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Reverse;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::regiments::Regiments;
use crate::structures::Refusal;
use crate::{MAX_PARTIES, PlayerId, StateHasher, UnitStats};

/// Grid cells along one tile.
pub const CELLS_PER_TILE: i32 = 2;
/// Sector edge, in cells (16 tiles).
pub const SECTOR: i32 = 32;
/// Cells a regiment's centre keeps from blocked ground: a cell is open when
/// the nearest blocked cell is at least this many steps away. One is any
/// cell that isn't blocked itself, so the narrowest way through is a tile
/// wide (two cells, each next to blocked ground), as wide as it looks. How
/// much room a block wants beyond that is its berth (see [`Nav::plan`]):
/// routes keep it where they can and narrow the block where they can't.
pub const CLEARANCE: u8 = 1;
/// Clearances are measured up to this many cells (16 tiles) and no further:
/// nothing asks about open ground wider than that, and it keeps a change of
/// ground (a building going up) from reaching across the map.
pub const CLEAR_CAP: u8 = 32;
/// Cost of a cell no regiment can enter.
pub const BLOCKED: u8 = 0;
/// Cost of a cell on a bridge's deck: open ground.
pub const DECK: u8 = 1;
/// The map's cost of a ford or shallows cell (see `gc_map`), and of open
/// ground, which a frozen ford costs (see [`Nav::set_frozen`]).
pub const FORD: u8 = 3;
pub const OPEN: u8 = 1;
/// The widest berth (cells from a block's centre that routes keep clear of
/// blocked ground) any regiment asks for: 8 tiles.
pub const MAX_BERTH: u8 = 16;
/// The berth the portal graph's links are costed for (a regiment of eight
/// files, four tiles across, wants about this much).
const GRAPH_BERTH: u8 = 8;
/// Most a cell's cost is raised by being close to blocked ground: on the
/// very edge of open ground a step costs this much more than in the open.
const WALL_COST: u32 = 4;
/// [`WALL_COST`] shared out over a berth: `WALL_PENALTY[berth][short]` is
/// what a cell `short` cells closer to blocked ground than `berth` adds to
/// a step's cost (worked out once, as searches ask for it constantly).
const WALL_PENALTY: [[u8; MAX_BERTH as usize + 1]; MAX_BERTH as usize + 1] = {
    let mut table = [[0; MAX_BERTH as usize + 1]; MAX_BERTH as usize + 1];
    let mut berth = 1;
    while berth <= MAX_BERTH as usize {
        let mut short = 0;
        while short <= berth {
            table[berth][short] = (short as u32 * WALL_COST).div_ceil(berth as u32) as u8;
            short += 1;
        }
        berth += 1;
    }
    table
};
/// Waypoints stored per regiment. A longer route is followed in parts: when
/// the last leg of the stored waypoints starts, the rest is searched for.
pub const MAX_WAYPOINTS: usize = 16;
/// Search work allowed per tick, in [`POP_WORK`] per search step and 1 per
/// cell checked along a line.
pub const WORK_PER_TICK: u32 = 32_000;
const POP_WORK: u32 = 4;
/// Longest run of open sector edge one portal serves; longer runs get several.
const PORTAL_SPAN: i32 = 16;
/// How far (cells) to look for open ground when a regiment stands too close
/// to blocked ground to start a route, and when its goal is blocked or cut off.
const SNAP_START: i32 = 16;
const SNAP_GOAL: i32 = 64;
/// A target that drifts less than this from the one a route was found for
/// (a chased enemy moving) keeps the route instead of searching again.
const RETARGET: Fx = Fx::from_int(3);
/// Longest straight leg smoothing makes, in cells (24 tiles). Checking
/// longer sight lines costs more than the extra waypoints save.
const SMOOTH_REACH: i32 = 48;
/// Portals a route is filled in through, cell by cell (about eight
/// sectors, 128 tiles): more than its waypoints usually reach, and a long
/// route's search costs a part of the work and spreads the rest over the
/// march instead of spending it all on the tick it was ordered.
const FILL_PORTALS: usize = 16;
/// Upper bound on cells in one route before smoothing.
const CHAIN_CAPACITY: usize = 32_768;
const NONE: u32 = u32::MAX;
/// Searches trust their distance estimate this much (in quarters): a little
/// over 1 finds routes within a quarter of the shortest, much faster.
const GREED: u32 = 5;

/// The eight neighbours; the last four are diagonals.
pub(crate) const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
];

#[path = "nav_reblock.rs"]
mod reblock;
pub(crate) use reblock::Door;

/// The walkable map and its portal graph. Built when the match starts, and
/// shared with any scratch world on the same map (see [`Nav::sharing`]);
/// brought up to date when buildings change the ground (see `reblock`).
#[derive(Clone)]
struct Grid {
    /// Cells per side.
    side: i32,
    /// Each cell's cost: the ground's own ([`Grid::ground`]), or
    /// [`BLOCKED`] under a building (but for an open door).
    cost: Vec<u8>,
    /// The ground's own cost of each cell, as the map gave it.
    ground: Vec<u8>,
    /// Buildings standing on each cell (see [`Nav::set_built`]).
    built: Vec<u8>,
    /// Doors open on each cell, to the sides walking this grid (see
    /// [`Nav::set_door`]): a cell is blocked while more buildings stand on
    /// it than doors are open there. Empty until the first door opens.
    opened: Vec<u8>,
    /// Cells a flood has shut (see [`Nav::set_flooded_all`]): blocked
    /// whatever stands there. Empty until the first flood.
    flooded: Vec<bool>,
    /// Finished bridge tiles on each cell (see [`Nav::set_bridged_all`]):
    /// open at [`DECK`] cost over deep water or a flood, unless a building
    /// stands there too. Empty until the first bridge.
    bridged: Vec<u8>,
    /// The fords are frozen over (see [`Nav::set_frozen`]): costed as open
    /// ground. Every ford freezes and thaws at once.
    frozen: bool,
    /// Steps (8-way) to the nearest blocked cell or the map edge, capped at
    /// [`CLEAR_CAP`].
    clear: Vec<u8>,
    /// Connected area of each open cell (1, 2, ...); 0 for cells not open.
    area: Vec<u32>,
    /// The highest area number given out so far (areas brought up to date
    /// near a change take numbers above it).
    areas: u32,
    /// Sectors per side.
    sectors: i32,
    /// Portal graph: each node is an open cell on a sector edge, in pairs
    /// facing each other across it (nodes `2k` and `2k + 1`).
    node_cell: Vec<u32>,
    /// The pairs on each sector edge, the east edge then the south edge of
    /// each sector in turn: edge `e`'s are `pair_first[e]..pair_first[e + 1]`.
    pair_first: Vec<u32>,
    /// Nodes of sector `s` are `sector_nodes[sector_first[s]..sector_first[s + 1]]`.
    sector_first: Vec<u32>,
    sector_nodes: Vec<u32>,
    /// Edges of node `n` are `edge_first[n]..edge_first[n + 1]`.
    edge_first: Vec<u32>,
    edge_to: Vec<u32>,
    edge_cost: Vec<u32>,
    /// The most the portal search's heap may hold over this graph (see
    /// [`Grid::node_room`]), so every world searching it searches alike.
    node_room: usize,
}

impl Grid {
    fn idx(&self, x: i32, y: i32) -> u32 {
        (y * self.side + x) as u32
    }

    fn xy(&self, c: u32) -> (i32, i32) {
        (c as i32 % self.side, c as i32 / self.side)
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.side && y < self.side
    }

    fn open(&self, x: i32, y: i32) -> bool {
        self.inside(x, y) && self.clear[self.idx(x, y) as usize] >= CLEARANCE
    }

    /// Whether a flood has shut cell `c` (an index into the grid).
    fn is_flooded(&self, c: usize) -> bool {
        self.flooded.get(c).copied().unwrap_or(false)
    }

    /// Whether a finished bridge spans cell `c` (an index into the grid).
    fn is_bridged(&self, c: usize) -> bool {
        self.bridged.get(c).is_some_and(|&n| n > 0)
    }

    fn open_cell(&self, c: u32) -> bool {
        self.clear[c as usize] >= CLEARANCE
    }

    /// The berth a line or a stretch of route between cells `a` and `b`
    /// can keep: `berth`, or less where either end is closer than that to
    /// blocked ground.
    fn need(&self, a: u32, b: u32, berth: u8) -> u8 {
        berth
            .min(self.clear[a as usize])
            .min(self.clear[b as usize])
            .max(CLEARANCE)
    }

    /// The berth a turn at `c` between `a` and `b` can keep: [`Grid::need`]
    /// at all three cells.
    fn need3(&self, a: u32, c: u32, b: u32, berth: u8) -> u8 {
        self.need(a, b, berth).min(self.need(c, c, berth))
    }

    /// Whether a route can go from `a` to `b` by way of `m` keeping `need`
    /// from blocked ground on ground costing no more than `max_cost`.
    fn legs_keep(&self, [a, m, b]: [FxVec2; 3], need: u8, max_cost: u8, work: &mut u32) -> bool {
        self.segment_within(a, m, need, max_cost, work)
            && self.segment_within(m, b, need, max_cost, work)
    }

    /// What crossing cell `c` costs a regiment wanting `berth`: the
    /// ground's cost, plus up to [`WALL_COST`] the closer it is to
    /// blocked ground than the berth.
    fn weight(&self, c: u32, berth: u8) -> u32 {
        let berth = berth.min(MAX_BERTH);
        let short = berth.saturating_sub(self.clear[c as usize]);
        u32::from(self.cost[c as usize])
            + u32::from(WALL_PENALTY[usize::from(berth)][usize::from(short)])
    }

    fn sector_of(&self, c: u32) -> u32 {
        let (x, y) = self.xy(c);
        ((y / SECTOR) * self.sectors + x / SECTOR) as u32
    }

    /// Cell bounds of a sector: x0, y0 inclusive, x1, y1 exclusive.
    fn sector_bounds(&self, s: u32) -> (i32, i32, i32, i32) {
        let (sx, sy) = (s as i32 % self.sectors, s as i32 / self.sectors);
        let (x0, y0) = (sx * SECTOR, sy * SECTOR);
        (
            x0,
            y0,
            (x0 + SECTOR).min(self.side),
            (y0 + SECTOR).min(self.side),
        )
    }

    /// Whether a map position is on the map.
    fn on_map(&self, p: FxVec2) -> bool {
        let inside = |v: Fx| {
            v.raw() >= 0
                && (i64::from(v.raw()) * i64::from(CELLS_PER_TILE)) >> 16 < i64::from(self.side)
        };
        inside(p.x) && inside(p.y)
    }

    /// The cell under a map position (clamped onto the map).
    fn cell_at(&self, p: FxVec2) -> u32 {
        let to_cell = |v: Fx| {
            ((i64::from(v.raw()) * i64::from(CELLS_PER_TILE)) >> 16)
                .clamp(0, i64::from(self.side) - 1) as i32
        };
        self.idx(to_cell(p.x), to_cell(p.y))
    }

    fn centre(&self, c: u32) -> FxVec2 {
        let (x, y) = self.xy(c);
        // (x + 1/2) / 2 tiles.
        FxVec2::new(
            Fx::from_raw((2 * x + 1) << 14),
            Fx::from_raw((2 * y + 1) << 14),
        )
    }

    /// Cost of one step between neighbouring cells: 10 straight and 14
    /// diagonal on open ground, scaled by the cells' weights for `berth`.
    fn step_cost(&self, a: u32, b: u32, diagonal: bool, berth: u8) -> u32 {
        let base = if diagonal { 7 } else { 5 };
        base * (self.weight(a, berth) + self.weight(b, berth))
    }

    /// Steps between two cells, diagonals counting as one.
    fn span(&self, a: u32, b: u32) -> i32 {
        let (ax, ay) = self.xy(a);
        let (bx, by) = self.xy(b);
        (ax - bx).abs().max((ay - by).abs())
    }

    /// Cheapest possible cost between two cells (ground of cost 1 all the way).
    fn estimate(&self, a: u32, b: u32) -> u32 {
        let (ax, ay) = self.xy(a);
        let (bx, by) = self.xy(b);
        let (dx, dy) = ((ax - bx).unsigned_abs(), (ay - by).unsigned_abs());
        10 * dx.max(dy) + 4 * dx.min(dy)
    }

    /// Whether cell (x, y) is on the map, at least `need` from blocked
    /// ground and costing no more than `max_cost`.
    fn fits(&self, x: i32, y: i32, need: u8, max_cost: u8) -> bool {
        self.inside(x, y) && {
            let i = self.idx(x, y) as usize;
            self.clear[i] >= need && self.cost[i] <= max_cost
        }
    }

    /// Whether the straight line between two cell centres stays on open
    /// cells. A line through a cell corner needs both cells beside the corner
    /// open. Adds the cells checked to `work`.
    fn line_open(&self, a: u32, b: u32, work: &mut u32) -> bool {
        self.line_within(a, b, CLEARANCE, u8::MAX, work)
    }

    /// [`Grid::line_open`], on cells at least `need` from blocked ground
    /// and costing no more than `max_cost`.
    fn line_within(&self, a: u32, b: u32, need: u8, max_cost: u8, work: &mut u32) -> bool {
        let open = |x: i32, y: i32| self.fits(x, y, need, max_cost);
        let (mut x, mut y) = self.xy(a);
        let (x1, y1) = self.xy(b);
        let (nx, ny) = ((x1 - x).abs(), (y1 - y).abs());
        let (sx, sy) = ((x1 - x).signum(), (y1 - y).signum());
        if !open(x, y) {
            return false;
        }
        let (mut ix, mut iy) = (0, 0);
        while ix < nx || iy < ny {
            *work += 1;
            let decision = (1 + 2 * ix) * ny - (1 + 2 * iy) * nx;
            if decision == 0 {
                if !open(x + sx, y) || !open(x, y + sy) {
                    return false;
                }
                x += sx;
                y += sy;
                ix += 1;
                iy += 1;
            } else if decision < 0 {
                x += sx;
                ix += 1;
            } else {
                y += sy;
                iy += 1;
            }
            if !open(x, y) {
                return false;
            }
        }
        true
    }

    /// Whether the straight walk from `p` to `q` (map positions) stays on
    /// cells at least `need` from blocked ground and costing no more than
    /// `max_cost`: every cell the line passes through, and both cells
    /// beside a corner it passes exactly through. What a regiment marching
    /// from `p` to `q` touches, wherever in their cells the two lie (a line
    /// between cell centres, as [`Grid::line_within`] checks, can miss the
    /// corner of a cell a regiment starting off-centre clips). Adds the
    /// cells checked to `work`.
    fn segment_within(&self, p: FxVec2, q: FxVec2, need: u8, max_cost: u8, work: &mut u32) -> bool {
        let ok = |x: i32, y: i32| self.fits(x, y, need, max_cost);
        // In cells, 16 bits of fraction.
        const ONE: i64 = 1 << 16;
        let cells = |v: Fx| i64::from(v.raw()) * i64::from(CELLS_PER_TILE);
        let (x0, y0, x1, y1) = (cells(p.x), cells(p.y), cells(q.x), cells(q.y));
        let (mut x, mut y) = ((x0 >> 16) as i32, (y0 >> 16) as i32);
        let (ex, ey) = ((x1 >> 16) as i32, (y1 >> 16) as i32);
        if !ok(x, y) {
            return false;
        }
        let (sx, sy) = ((x1 - x0).signum() as i32, (y1 - y0).signum() as i32);
        let (adx, ady) = ((x1 - x0).abs(), (y1 - y0).abs());
        // Distance along each axis from the start to the next cell edge
        // the line crosses on it; the line reaches the x edge first when
        // `bx / adx < by / ady`.
        let edge = |v: i64, s: i32| match s {
            1 => ONE - (v & (ONE - 1)),
            -1 => v & (ONE - 1),
            _ => 0,
        };
        let (mut bx, mut by) = (edge(x0, sx), edge(y0, sy));
        // A start exactly on a cell's low edge, heading back across it: the
        // walk is in the cell beyond at once (both beside a corner).
        let (back_x, back_y) = (sx < 0 && bx == 0, sy < 0 && by == 0);
        if back_x && back_y && !(ok(x - 1, y) && ok(x, y - 1)) {
            return false;
        }
        if back_x {
            x -= 1;
            bx = ONE;
        }
        if back_y {
            y -= 1;
            by = ONE;
        }
        if (back_x || back_y) && !ok(x, y) {
            return false;
        }
        while x != ex || y != ey {
            *work += 1;
            // Each factor is under 2^31 on any side under 2^15 cells.
            let (tx, ty) = (bx * ady, by * adx);
            let step_x = sx != 0 && x != ex && (sy == 0 || y == ey || tx <= ty);
            let step_y = sy != 0 && y != ey && (sx == 0 || x == ex || ty <= tx);
            if step_x && step_y {
                // Exactly through a corner: both cells beside it.
                if !ok(x + sx, y) || !ok(x, y + sy) {
                    return false;
                }
            }
            if step_x {
                x += sx;
                bx += ONE;
            }
            if step_y {
                y += sy;
                by += ONE;
            }
            if !(step_x || step_y) {
                break; // can't happen: one axis always has a cell to go
            }
            if !ok(x, y) {
                return false;
            }
        }
        true
    }

    /// The nearest open cell to `c` within `radius` cells (in the given
    /// area, if any), nearest by straight-line distance.
    fn nearest_open(&self, c: u32, area: Option<u32>, radius: i32, work: &mut u32) -> Option<u32> {
        let (cx, cy) = self.xy(c);
        let mut best: Option<(i32, u32)> = None;
        let consider = |x: i32, y: i32, best: &mut Option<(i32, u32)>| {
            if !self.open(x, y) {
                return;
            }
            let cell = self.idx(x, y);
            if area.is_some_and(|a| self.area[cell as usize] != a) {
                return;
            }
            let d = (x - cx) * (x - cx) + (y - cy) * (y - cy);
            if best.is_none_or(|(bd, _)| d < bd) {
                *best = Some((d, cell));
            }
        };
        for r in 1..=radius {
            if best.is_some_and(|(d, _)| r * r > d) {
                break;
            }
            *work += 8 * r as u32;
            for dx in -r..=r {
                consider(cx + dx, cy - r, &mut best);
                consider(cx + dx, cy + r, &mut best);
            }
            for dy in 1 - r..r {
                consider(cx - r, cy + dy, &mut best);
                consider(cx + r, cy + dy, &mut best);
            }
        }
        best.map(|(_, cell)| cell)
    }

    /// The nearest cell at least `berth` from blocked ground within as
    /// many cells of `c` as `c` falls short of that, in `c`'s area and in
    /// plain sight of it: `c` itself if it has the berth already.
    fn fitting(&self, c: u32, berth: u8, work: &mut u32) -> Option<u32> {
        let have = self.clear[c as usize];
        if have >= berth {
            return Some(c);
        }
        let r = i32::from(berth - have);
        let (cx, cy) = self.xy(c);
        let area = self.area[c as usize];
        // (distance squared, cell) of the nearest so far.
        let mut best: Option<(i32, u32)> = None;
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                *work += 1;
                if !self.open(x, y) {
                    continue;
                }
                let m = self.idx(x, y);
                let d = (x - cx) * (x - cx) + (y - cy) * (y - cy);
                if self.clear[m as usize] >= berth
                    && best.is_none_or(|(near, _)| d < near)
                    && self.area[m as usize] == area
                    && self.line_open(c, m, work)
                {
                    best = Some((d, m));
                }
            }
        }
        best.map(|(_, m)| m)
    }
}

/// Scratch space for searches, reserved up front so ticks never allocate.
struct Search {
    /// Search within one sector, indexed by cell within the sector.
    g: Vec<u32>,
    parent: Vec<u32>,
    seen: Vec<u32>,
    /// Cells [`Search::reach`] is done once it has settled (marked with
    /// its stamp), and the weights it reads.
    want: Vec<u32>,
    weights: Vec<u32>,
    stamp: u32,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
    /// Search over the portal graph (two extra nodes: start and goal).
    node_g: Vec<u32>,
    node_parent: Vec<u32>,
    node_seen: Vec<u32>,
    /// Cost from each node to the goal, for nodes in the goal's sector.
    to_goal: Vec<u32>,
    node_stamp: u32,
    node_heap: BinaryHeap<Reverse<(u32, u32)>>,
    links: Vec<(u32, u32)>,
    /// Cells of the portal route: start, portals, goal.
    coarse: Vec<u32>,
    /// Cells of the route being built, before smoothing.
    chain: Vec<u32>,
    work: u32,
}

impl Search {
    /// Room for searches over `grid`'s portal graph.
    #[cfg(test)]
    fn for_grid(grid: &Grid) -> Search {
        let mut search = Search::new();
        search.fit(grid);
        search
    }

    /// Room for searches over each of `grids`' portal graphs.
    fn for_grids(grids: &[Arc<Grid>]) -> Search {
        let mut search = Search::new();
        for g in grids {
            search.fit(g);
        }
        search
    }

    /// Nodes, links and the most nodes in one sector, in `grid`'s graph.
    fn room(grid: &Grid) -> (usize, usize, usize) {
        let most = grid
            .sector_first
            .windows(2)
            .map(|w| (w[1] - w[0]) as usize)
            .max()
            .unwrap_or(0);
        (grid.node_cell.len(), grid.edge_to.len(), most)
    }

    /// Make room for searches over `grid`'s portal graph (a new one, or
    /// one that changed, see `Grid::changed`), keeping what is already
    /// reserved: one search serves every side's grid (see [`Nav`]).
    fn fit(&mut self, grid: &Grid) {
        let (nodes, _, most) = Search::room(grid);
        if self.node_g.len() < nodes + 2 {
            self.node_g.resize(nodes + 2, 0);
            self.node_parent.resize(nodes + 2, NONE);
            self.node_seen.resize(nodes + 2, 0);
            self.to_goal.resize(nodes + 2, 0);
        }
        // `reserve` makes room beyond what is there, so at least this much.
        self.node_heap.reserve(grid.node_room);
        self.links.reserve(most);
        self.coarse.reserve(nodes + 2);
    }

    /// Room for searches inside a sector; [`Search::fit`] makes room for a
    /// grid's portal graph.
    fn new() -> Search {
        let cells = (SECTOR * SECTOR) as usize;
        Search {
            g: vec![0; cells],
            parent: vec![NONE; cells],
            seen: vec![0; cells],
            want: vec![0; cells],
            weights: vec![0; cells],
            stamp: 0,
            heap: BinaryHeap::with_capacity(cells * 8 + 8),
            node_g: vec![0; 2],
            node_parent: vec![NONE; 2],
            node_seen: vec![0; 2],
            to_goal: vec![0; 2],
            node_stamp: 0,
            node_heap: BinaryHeap::with_capacity(8),
            links: Vec::new(),
            coarse: Vec::with_capacity(2),
            chain: Vec::with_capacity(CHAIN_CAPACITY),
            work: 0,
        }
    }

    /// Search inside sector `sec` from cell `from`. With a goal, A* until it
    /// is reached; without one, every reachable cell of the sector. Returns
    /// whether the goal was reached (true without a goal). Afterwards
    /// [`Search::cost_to`] gives the cost to each cell reached.
    fn sector(&mut self, grid: &Grid, sec: u32, from: u32, to: Option<u32>, berth: u8) -> bool {
        self.next_stamp();
        let (x0, y0, x1, y1) = grid.sector_bounds(sec);
        let local = |c: u32| {
            let (x, y) = grid.xy(c);
            ((y - y0) * SECTOR + (x - x0)) as usize
        };
        let h = |c: u32| to.map_or(0, |t| grid.estimate(c, t) * GREED / 4);
        self.heap.clear();
        let l0 = local(from);
        self.seen[l0] = self.stamp;
        self.g[l0] = 0;
        self.parent[l0] = NONE;
        self.heap.push(Reverse((h(from), l0 as u32)));
        while let Some(Reverse((f, l))) = self.heap.pop() {
            self.work += POP_WORK;
            let l = l as usize;
            let (x, y) = (x0 + l as i32 % SECTOR, y0 + l as i32 / SECTOR);
            let c = grid.idx(x, y);
            let g = self.g[l];
            if f != g + h(c) {
                continue; // a stale entry; the cell was reached more cheaply since
            }
            if Some(c) == to {
                return true;
            }
            let here = grid.weight(c, berth);
            for (k, &(dx, dy)) in DIRS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if nx < x0 || ny < y0 || nx >= x1 || ny >= y1 || !grid.open(nx, ny) {
                    continue;
                }
                let diagonal = k >= 4;
                if diagonal && !(grid.open(x + dx, y) && grid.open(x, y + dy)) {
                    continue;
                }
                let n = grid.idx(nx, ny);
                let nl = ((ny - y0) * SECTOR + (nx - x0)) as usize;
                // As `grid.step_cost`, with this cell's weight found once.
                let ng = g + if diagonal { 7 } else { 5 } * (here + grid.weight(n, berth));
                if (self.seen[nl] != self.stamp || ng < self.g[nl])
                    && self.heap.len() < self.heap.capacity()
                {
                    self.seen[nl] = self.stamp;
                    self.g[nl] = ng;
                    self.parent[nl] = l as u32;
                    self.heap.push(Reverse((ng + h(n), nl as u32)));
                }
            }
        }
        to.is_none()
    }

    /// A fresh stamp for a search's marks.
    fn next_stamp(&mut self) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.want.fill(0);
            self.stamp = 1;
        }
    }

    /// Note sector `sec`'s weights for `berth` (see [`Grid::weight`]), 0
    /// on cells that aren't open, for [`Search::reach`]: one sector's
    /// worth of searches reads them instead of working each out again.
    fn weigh(&mut self, grid: &Grid, sec: u32, berth: u8) {
        let (x0, y0, x1, y1) = grid.sector_bounds(sec);
        for y in y0..y1 {
            for x in x0..x1 {
                let c = grid.idx(x, y);
                let w = if grid.open_cell(c) {
                    grid.weight(c, berth)
                } else {
                    0
                };
                self.weights[((y - y0) * SECTOR + (x - x0)) as usize] = w;
            }
        }
    }

    /// [`Search::sector`] with no goal over the weights [`Search::weigh`]
    /// noted for sector `sec`, stopping once it has settled every cell of
    /// `wanted` (all in the sector): their costs from `from` are known
    /// then (other cells' may not be), or every reachable cell's if some
    /// of `wanted` can't be reached.
    fn reach(&mut self, grid: &Grid, sec: u32, from: u32, wanted: &[u32]) {
        self.next_stamp();
        let (x0, y0, x1, y1) = grid.sector_bounds(sec);
        let (w, h) = (x1 - x0, y1 - y0);
        let local = |c: u32| {
            let (x, y) = grid.xy(c);
            ((y - y0) * SECTOR + (x - x0)) as usize
        };
        let mut unsettled = 0;
        for &c in wanted {
            let l = local(c);
            if self.want[l] != self.stamp {
                self.want[l] = self.stamp;
                unsettled += 1;
            }
        }
        self.heap.clear();
        let l0 = local(from);
        self.seen[l0] = self.stamp;
        self.g[l0] = 0;
        self.heap.push(Reverse((0, l0 as u32)));
        let open = |weights: &[u32], x: i32, y: i32| {
            x >= 0 && y >= 0 && x < w && y < h && weights[(y * SECTOR + x) as usize] > 0
        };
        while let Some(Reverse((g, l))) = self.heap.pop() {
            let l = l as usize;
            if g != self.g[l] {
                continue; // a stale entry
            }
            if self.want[l] == self.stamp {
                self.want[l] = 0;
                unsettled -= 1;
                if unsettled == 0 {
                    return;
                }
            }
            let (x, y) = (l as i32 % SECTOR, l as i32 / SECTOR);
            let here = self.weights[l];
            for (k, &(dx, dy)) in DIRS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if !open(&self.weights, nx, ny) {
                    continue;
                }
                let diagonal = k >= 4;
                if diagonal && !(open(&self.weights, x + dx, y) && open(&self.weights, x, y + dy)) {
                    continue;
                }
                let nl = (ny * SECTOR + nx) as usize;
                let ng = g + if diagonal { 7 } else { 5 } * (here + self.weights[nl]);
                if (self.seen[nl] != self.stamp || ng < self.g[nl])
                    && self.heap.len() < self.heap.capacity()
                {
                    self.seen[nl] = self.stamp;
                    self.g[nl] = ng;
                    self.heap.push(Reverse((ng, nl as u32)));
                }
            }
        }
    }

    /// Cost to `c` found by the last [`Search::sector`] in sector `sec`.
    fn cost_to(&self, grid: &Grid, sec: u32, c: u32) -> Option<u32> {
        let (x0, y0, _, _) = grid.sector_bounds(sec);
        let (x, y) = grid.xy(c);
        let l = ((y - y0) * SECTOR + (x - x0)) as usize;
        (self.seen[l] == self.stamp).then_some(self.g[l])
    }

    /// Append the cells of the last sector search's route to `to` (after
    /// its first cell) to the chain. Returns false if the chain is full.
    fn append_sector_route(&mut self, grid: &Grid, sec: u32, to: u32) -> bool {
        let (x0, y0, _, _) = grid.sector_bounds(sec);
        let start = self.chain.len();
        let (x, y) = grid.xy(to);
        let mut l = ((y - y0) * SECTOR + (x - x0)) as u32;
        while self.parent[l as usize] != NONE {
            if self.chain.len() == self.chain.capacity() {
                self.chain.truncate(start);
                return false;
            }
            self.chain
                .push(grid.idx(x0 + l as i32 % SECTOR, y0 + l as i32 / SECTOR));
            l = self.parent[l as usize];
        }
        self.chain[start..].reverse();
        true
    }

    /// Portal nodes of `sec` reachable from cell `c`, with their costs, into
    /// `self.links`. Nodes in plain sight are linked by distance. That is
    /// enough: every node reachable inside the sector is linked to the ones
    /// in sight by the graph's own edges. Only when none is in sight is the
    /// sector searched.
    fn link(&mut self, grid: &Grid, sec: u32, c: u32, berth: u8) {
        self.links.clear();
        let nodes = &grid.sector_nodes[grid.sector_first[sec as usize] as usize
            ..grid.sector_first[sec as usize + 1] as usize];
        for &n in nodes {
            let cell = grid.node_cell[n as usize];
            if grid.line_open(c, cell, &mut self.work) {
                self.links.push((n, grid.estimate(c, cell)));
            }
        }
        if !self.links.is_empty() {
            return;
        }
        self.sector(grid, sec, c, None, berth);
        for &n in nodes {
            if let Some(cost) = self.cost_to(grid, sec, grid.node_cell[n as usize]) {
                self.links.push((n, cost));
            }
        }
    }

    /// A* over the portal graph from cell `from` to cell `to`. On success the
    /// coarse route holds `from`, the portal cells in order, and `to`.
    fn portals(&mut self, grid: &Grid, from: u32, to: u32, berth: u8) -> bool {
        let nodes = grid.node_cell.len() as u32;
        let (start, goal) = (nodes, nodes + 1);
        self.node_stamp = self.node_stamp.wrapping_add(1);
        if self.node_stamp == 0 {
            self.node_seen.fill(0);
            self.node_stamp = 1;
        }
        let stamp = self.node_stamp;
        // Links into the goal (costs are the same both ways); NONE: no link.
        let goal_sector = grid.sector_of(to);
        self.link(grid, goal_sector, to, berth);
        for &n in &grid.sector_nodes[grid.sector_first[goal_sector as usize] as usize
            ..grid.sector_first[goal_sector as usize + 1] as usize]
        {
            self.to_goal[n as usize] = NONE;
        }
        for &(n, cost) in &self.links {
            self.to_goal[n as usize] = cost;
        }
        let in_goal_sector =
            |n: u32| n < nodes && grid.sector_of(grid.node_cell[n as usize]) == goal_sector;
        let cell_of = |n: u32| {
            if n == start {
                from
            } else if n == goal {
                to
            } else {
                grid.node_cell[n as usize]
            }
        };
        let h = |n: u32| grid.estimate(cell_of(n), to) * GREED / 4;

        self.link(grid, grid.sector_of(from), from, berth);
        self.node_heap.clear();
        self.node_seen[start as usize] = stamp;
        self.node_g[start as usize] = 0;
        self.node_parent[start as usize] = NONE;
        self.node_heap.push(Reverse((h(start), start)));
        let mut found = false;
        while let Some(Reverse((f, n))) = self.node_heap.pop() {
            self.work += POP_WORK;
            let g = self.node_g[n as usize];
            if f != g + h(n) {
                continue;
            }
            if n == goal {
                found = true;
                break;
            }
            let relax = |m: u32, cost: u32, s: &mut Search| {
                let ng = g + cost;
                if (s.node_seen[m as usize] != stamp || ng < s.node_g[m as usize])
                    && s.node_heap.len() < grid.node_room
                {
                    s.node_seen[m as usize] = stamp;
                    s.node_g[m as usize] = ng;
                    s.node_parent[m as usize] = n;
                    s.node_heap.push(Reverse((ng + h(m), m)));
                }
            };
            if n == start {
                for k in 0..self.links.len() {
                    let (m, cost) = self.links[k];
                    relax(m, cost, self);
                }
                continue;
            }
            for e in grid.edge_first[n as usize]..grid.edge_first[n as usize + 1] {
                relax(grid.edge_to[e as usize], grid.edge_cost[e as usize], self);
            }
            if in_goal_sector(n) && self.to_goal[n as usize] != NONE {
                let cost = self.to_goal[n as usize];
                relax(goal, cost, self);
            }
        }
        if !found {
            return false;
        }
        self.coarse.clear();
        let mut n = goal;
        while n != NONE {
            self.coarse.push(cell_of(n));
            n = self.node_parent[n as usize];
        }
        self.coarse.reverse();
        true
    }
}

/// A route: where the regiment is heading and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Route {
    /// No target.
    Idle,
    /// The target is in plain sight: walk straight to it.
    Direct,
    /// Following stored waypoints.
    Waypoints,
    /// Queued for a search.
    Waiting,
    /// No way there; stands still until the target moves.
    Failed,
    /// Queued for a search, but still walking the stored waypoints until
    /// it is served (the rest of a long route, or ground changed near the
    /// way ahead): it only stands if they end short of the target and it
    /// reaches their end first.
    Rerouting,
}

/// The map's walkable ground and every regiment's route.
///
/// Every side walks the same grid until a gate goes up. A gate is a door:
/// open ground to some sides and wall to the rest (see [`Nav::set_door`]),
/// so from the first gate a side whose own gates let it through where they
/// stop others walks a grid of its own, a copy of the shared one kept up
/// to date alongside it. A side is a party and its allies. Searches on
/// each grid run just as on the one, at no extra cost; the price is the
/// copies' memory and bringing each up to date when the ground changes.
pub struct Nav {
    /// The grid each side may walk; empty on an open plane with no map
    /// (tests), where everyone walks straight. The first is shared by every
    /// side without one of its own.
    grids: Vec<Arc<Grid>>,
    /// Which of `grids` each party walks.
    view: [u8; MAX_PARTIES],
    /// Each party's side, as a mask of parties: itself and its allies.
    side: [u8; MAX_PARTIES],
    /// The parties in the match, as a mask.
    parties: u8,
    search: Search,
    route: Vec<Route>,
    /// The target the route was found for.
    goal: Vec<FxVec2>,
    /// `MAX_WAYPOINTS` per regiment.
    way: Vec<FxVec2>,
    way_len: Vec<u8>,
    way_next: Vec<u8>,
    /// The stored waypoints stop short of the goal.
    partial: Vec<bool>,
    /// Regiments standing still until their search is served, served
    /// first...
    queue: Queue,
    /// ...then those still walking meanwhile ([`Route::Rerouting`]).
    later: Queue,
}

/// Regiments waiting for a search, oldest first: a ring buffer with room
/// for every regiment, which each appear in it at most once.
struct Queue {
    slots: Vec<u32>,
    head: usize,
    len: usize,
    queued: Vec<bool>,
}

impl Queue {
    fn new(regiments: usize) -> Queue {
        Queue {
            slots: vec![0; regiments],
            head: 0,
            len: 0,
            queued: vec![false; regiments],
        }
    }

    fn push(&mut self, i: usize) {
        if self.queued[i] {
            return;
        }
        let cap = self.slots.len();
        self.slots[(self.head + self.len) % cap] = i as u32;
        self.len += 1;
        self.queued[i] = true;
    }

    fn pop(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let i = self.slots[self.head] as usize;
        self.head = (self.head + 1) % self.slots.len();
        self.len -= 1;
        self.queued[i] = false;
        Some(i)
    }

    fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.len).map(|k| self.slots[(self.head + k) % self.slots.len()])
    }

    /// Take `i` out of the queue, keeping the others' order.
    fn remove(&mut self, i: usize) {
        if !self.queued[i] {
            return;
        }
        let cap = self.slots.len();
        let mut kept = 0;
        for k in 0..self.len {
            let s = self.slots[(self.head + k) % cap];
            if s as usize != i {
                self.slots[(self.head + kept) % cap] = s;
                kept += 1;
            }
        }
        self.len = kept;
        self.queued[i] = false;
    }
}

impl Nav {
    /// `costs` holds one cost per cell, row by row, for a map `map_tiles`
    /// tiles across ([`CELLS_PER_TILE`] cells per tile). Empty means an open
    /// plane with no obstacles.
    pub fn new(costs: &[u8], map_tiles: u32, regiments: usize) -> Nav {
        Nav::standing(costs, map_tiles, regiments, core::iter::empty())
    }

    /// [`Nav::new`] with something already standing on each of `standing`'s
    /// tiles (`[x, y, w, d]`, those on the map), as a building would
    /// ([`Nav::set_built`]): thicket, which clearing takes away.
    pub(crate) fn standing(
        costs: &[u8],
        map_tiles: u32,
        regiments: usize,
        standing: impl Iterator<Item = [i32; 4]>,
    ) -> Nav {
        let grid = (!costs.is_empty()).then(|| {
            let side = map_tiles as i32 * CELLS_PER_TILE;
            assert_eq!(
                costs.len(),
                (side * side) as usize,
                "the nav grid must have {side} x {side} cells"
            );
            Arc::new(Grid::build_standing(costs, side, standing))
        });
        Nav::on(grid.into_iter().collect(), regiments)
    }

    /// Routes for `regiments` regiments on the same map as this one, sharing
    /// its grids and portal graphs rather than building them again (scratch
    /// worlds, see `World::scratch`).
    pub fn sharing(&self, regiments: usize) -> Nav {
        let mut nav = Nav::on(self.grids.clone(), regiments);
        nav.view = self.view;
        nav.side = self.side;
        nav.parties = self.parties;
        nav
    }

    fn on(grids: Vec<Arc<Grid>>, regiments: usize) -> Nav {
        let search = Search::for_grids(&grids);
        Nav {
            grids,
            view: [0; MAX_PARTIES],
            side: core::array::from_fn(|p| 1 << p),
            parties: u8::MAX,
            search,
            route: vec![Route::Idle; regiments],
            goal: vec![FxVec2::ZERO; regiments],
            way: vec![FxVec2::ZERO; regiments * MAX_WAYPOINTS],
            way_len: vec![0; regiments],
            way_next: vec![0; regiments],
            partial: vec![false; regiments],
            queue: Queue::new(regiments),
            later: Queue::new(regiments),
        }
    }

    /// Whether there is a map (a walkable grid) at all.
    pub fn has_map(&self) -> bool {
        !self.grids.is_empty()
    }

    /// The parties on each side, of the first `parties` (all there can be
    /// when 0): `allied(a, b)` says whether `a` and `b` are on the same
    /// side (their own gates let both through). Set once, before the first
    /// gate goes up.
    pub(crate) fn set_sides(
        &mut self,
        parties: usize,
        allied: impl Fn(PlayerId, PlayerId) -> bool,
    ) {
        if (1..MAX_PARTIES).contains(&parties) {
            self.parties = (1u8 << parties) - 1;
        }
        for a in 0..MAX_PARTIES {
            self.side[a] = (0..MAX_PARTIES)
                .filter(|&b| a == b || allied(a as PlayerId, b as PlayerId))
                .fold(0, |m, b| m | 1 << b);
        }
    }

    /// The grid `party`'s regiments walk (`None` with no map).
    fn grid_of(&self, party: PlayerId) -> Option<&Grid> {
        grid_for(&self.grids, &self.view, party)
    }

    /// The first grid, for what is the same on every grid: the ground and
    /// where buildings stand, not doors (`None` with no map).
    fn base(&self) -> Option<&Grid> {
        self.grids.first().map(|g| &**g)
    }

    /// Whether a regiment of `party`'s may have its centre at `p` (always
    /// true with no map).
    pub fn walkable(&self, party: PlayerId, p: FxVec2) -> bool {
        self.grid_of(party)
            .is_none_or(|g| g.open_cell(g.cell_at(p)))
    }

    /// Whether a soldier of `party`'s may stand at `p`: on the map, off deep
    /// water, cliffs and buildings but for gates open to it (always true
    /// with no map).
    pub fn standable(&self, party: PlayerId, p: FxVec2) -> bool {
        self.grid_of(party).is_none_or(|g| {
            let side = Fx::from_ratio(g.side, CELLS_PER_TILE);
            let on_map = p.x >= Fx::ZERO && p.y >= Fx::ZERO && p.x < side && p.y < side;
            on_map && g.cost[g.cell_at(p) as usize] != BLOCKED
        })
    }

    /// Whether a regiment of `party`'s at `p` has open ground all round
    /// for at least `tiles` (always true with no map): nothing blocked that
    /// near. `tiles` is at most half [`CLEAR_CAP`].
    pub fn open_around(&self, party: PlayerId, p: FxVec2, tiles: i32) -> bool {
        debug_assert!(tiles * CELLS_PER_TILE <= i32::from(CLEAR_CAP));
        self.grid_of(party)
            .is_none_or(|g| i32::from(g.clear[g.cell_at(p) as usize]) >= tiles * CELLS_PER_TILE)
    }

    /// How far a block centred at `centre` and facing `facing` can spread
    /// to either side before reaching blocked ground: the least room
    /// either side along each line across the block at the offsets
    /// `lines` (tiles ahead of the centre; behind when negative), up to
    /// `reach`. A line that starts on blocked ground (the rear of a block
    /// hanging over the water) doesn't count. Measured in quarter tiles, on
    /// the ground `party` walks.
    pub(crate) fn room_across(
        &self,
        party: PlayerId,
        centre: FxVec2,
        facing: Angle,
        reach: Fx,
        lines: &[Fx],
    ) -> Fx {
        let Some(g) = self.grid_of(party) else {
            return reach;
        };
        // Steps to blocked ground from the cell under `p` (0 on it or off
        // the map).
        let side = Fx::from_ratio(g.side, CELLS_PER_TILE);
        let clear_at = |p: FxVec2| {
            if p.x < Fx::ZERO || p.y < Fx::ZERO || p.x >= side || p.y >= side {
                0
            } else {
                g.clear[g.cell_at(p) as usize]
            }
        };
        let mut room = reach;
        let ahead = facing.to_vec();
        let right = facing.wrapping_add(Angle::QUARTER).to_vec();
        let quarter = Fx::from_ratio(1, 4);
        for &a in lines {
            let base = centre + ahead * a;
            if clear_at(base) == 0 {
                continue;
            }
            for side in [right, FxVec2::ZERO - right] {
                let mut d = quarter;
                while d < room {
                    let clear = clear_at(base + side * d);
                    if clear == 0 {
                        // The edge lies somewhere in the last quarter.
                        room = d - quarter;
                        break;
                    }
                    // Every point within `clear - 1` cells of this one is
                    // on firm ground too, so the probes in between (a cell
                    // less, for rounding) needn't look: two a cell.
                    d += quarter * (2 * (i32::from(clear) - 2)).max(1);
                }
            }
        }
        room
    }

    /// Where a block wanting `berth` (see [`Regiments::berth`]) ordered to
    /// stand at `p` should stand: `p` if the block fits there, else the
    /// nearest ground within the shortfall of it, in plain sight, where it
    /// does fit, so a block sent to the water's edge stands back from it
    /// instead of hanging over it (and `p` if there is no such ground). `p`
    /// on ground too close to blocked ground for a regiment's centre first
    /// becomes the nearest that isn't (as a route would take it). Always
    /// `p` with no map.
    ///
    /// Only to ground where the block fits: in a narrows the roomiest spot
    /// is one spot for everyone sent near it, and blocks ordered apart
    /// would end up on top of each other. On the ground `party` walks.
    pub(crate) fn settle(&self, party: PlayerId, p: FxVec2, berth: u8) -> FxVec2 {
        let Some(g) = self.grid_of(party) else {
            return p;
        };
        let work = &mut 0;
        let mut c = g.cell_at(p);
        if !g.open_cell(c) {
            match g.nearest_open(c, None, SNAP_GOAL, work) {
                Some(open) => c = open,
                None => return p,
            }
        }
        match g.fitting(c, berth, work) {
            Some(fit) if fit != g.cell_at(p) => g.centre(fit),
            _ => p,
        }
    }

    /// Whether a block of `party`'s wanting `berth` fits with its centre
    /// at `p`: the ground there is that far from blocked ground (always
    /// true with no map).
    pub(crate) fn fits(&self, party: PlayerId, p: FxVec2, berth: u8) -> bool {
        self.grid_of(party)
            .is_none_or(|g| g.clear[g.cell_at(p) as usize] >= berth)
    }

    /// Whether the centre of a regiment of `party`'s could walk the
    /// straight line from `from` to `to` (always true with no map).
    pub fn clear_line(&self, party: PlayerId, from: FxVec2, to: FxVec2) -> bool {
        self.grid_of(party)
            .is_none_or(|g| g.line_open(g.cell_at(from), g.cell_at(to), &mut 0))
    }

    /// Put up (`on`) or pull down a building on the tiles `x..x + w` ×
    /// `y..y + d`, all on the map: while one stands there no regiment may
    /// walk on them. Routes that might cross the changed ground are searched
    /// again. No effect with no map. See [`Nav::set_built_all`].
    pub(crate) fn set_built(&mut self, tiles: [i32; 4], on: bool, regs: &Regiments) {
        self.set_built_all(&[tiles], on, regs);
    }

    /// Whether a building could stand on the tiles `x..x + w` × `y..y + d`:
    /// all on the map, on ground a regiment could walk on, with no building
    /// on them (always, with no map). Thicket stands on its tiles too (see
    /// [`Nav::standing`]), and is cleared before a building goes up, and a
    /// gate replaces its owner's wall tiles, so `spare` says how many of the
    /// things standing on a tile will be gone by then.
    pub(crate) fn footing(
        &self,
        tiles: [i32; 4],
        spare: impl Fn(i32, i32) -> u8,
    ) -> Result<(), Refusal> {
        // Buildings and thicket stand on every side's grid alike.
        let Some(g) = self.base() else {
            return Ok(());
        };
        let [x0, y0, x1, y1] = g.cells(tiles).ok_or(Refusal::OffMap)?;
        let cells = || (y0..y1).flat_map(move |cy| (x0..x1).map(move |cx| (cx, cy)));
        let at = |(cx, cy)| g.idx(cx, cy) as usize;
        if cells().any(|c| g.ground[at(c)] == BLOCKED || g.is_flooded(at(c))) {
            return Err(Refusal::Rough);
        }
        // Nothing is built on a bridge.
        if cells().any(|c| g.is_bridged(at(c))) {
            return Err(Refusal::Built);
        }
        let gone = |(cx, cy)| spare(cx / CELLS_PER_TILE, cy / CELLS_PER_TILE);
        if cells().any(|c| g.built[at(c)] > 0 && g.built[at(c)] > gone(c)) {
            return Err(Refusal::Built);
        }
        Ok(())
    }

    /// Whether a bridge could be laid on the tile `(x, y)`: on the map,
    /// with nothing built or bridged there (always, with no map). Water is
    /// what it spans; whether the tile is water or bank the caller checks.
    pub(crate) fn deck_footing(&self, x: i32, y: i32) -> Result<(), Refusal> {
        let Some(g) = self.base() else {
            return Ok(());
        };
        let [x0, y0, x1, y1] = g.cells([x, y, 1, 1]).ok_or(Refusal::OffMap)?;
        let cells = (y0..y1).flat_map(move |cy| (x0..x1).map(move |cx| (cx, cy)));
        for (cx, cy) in cells {
            let c = g.idx(cx, cy) as usize;
            if g.built[c] > 0 || g.is_bridged(c) {
                return Err(Refusal::Built);
            }
        }
        Ok(())
    }

    /// Whether any cell of the tile `(x, y)` is ground a regiment could
    /// walk on, buildings and floods aside (never off the map, nor with no
    /// map).
    pub(crate) fn walkable_ground(&self, x: i32, y: i32) -> bool {
        let Some(g) = self.base() else {
            return false;
        };
        let Some([x0, y0, x1, y1]) = g.cells([x, y, 1, 1]) else {
            return false;
        };
        (y0..y1).any(|cy| (x0..x1).any(|cx| g.ground[g.idx(cx, cy) as usize] != BLOCKED))
    }

    /// Portal nodes in the graph (for the dev overlay and tests).
    pub fn portal_count(&self) -> usize {
        self.base().map_or(0, |g| g.node_cell.len())
    }

    /// Regiments waiting for a route (some still walking the old one).
    pub fn queued(&self) -> usize {
        self.queue.len + self.later.len
    }

    /// The rest of regiment slot `i`'s route, for drawing: the waypoints
    /// still ahead, ending at the target.
    pub(crate) fn route_ahead(&self, i: usize, target: Option<FxVec2>) -> &[FxVec2] {
        match (self.has_map(), self.route[i]) {
            (true, Route::Waypoints | Route::Rerouting) => self.ahead(i),
            (false, _) | (true, Route::Direct) if target.is_some() => {
                core::slice::from_ref(&self.goal[i])
            }
            _ => &[],
        }
    }

    /// Bring every regiment's route up to date with its target, then run the
    /// queued searches this tick's budget allows.
    ///
    /// Each route keeps the regiment's berth ([`Regiments::berth`]) from
    /// blocked ground where the ground allows: a target in plain sight is
    /// walked to straight only if the line keeps it (or as much as the
    /// regiment has where it stands and the target has).
    pub(crate) fn plan(&mut self, regs: &mut Regiments, units: &[UnitStats]) {
        if self.grids.is_empty() {
            // Open plane: remember targets so the game can draw them.
            for i in 0..regs.alive.len() {
                if let Some(t) = regs.target[i] {
                    self.goal[i] = t;
                }
            }
            return;
        }
        let (grids, view) = (&self.grids, &self.view);
        let grid_of = |owner: PlayerId| grid_for(grids, view, owner).expect("a map");
        self.search.work = 0;
        for i in 0..regs.alive.len() {
            // A regiment on a wall walks nowhere until it comes down (see
            // [`crate::manning`]), and then plans from where it lands.
            let (true, Some(t), false) = (regs.alive[i], regs.target[i], regs.on_wall[i]) else {
                self.route[i] = Route::Idle;
                self.later.remove(i);
                continue;
            };
            let d = t - self.goal[i];
            let near = self.route[i] != Route::Idle && d.x.abs() < RETARGET && d.y.abs() < RETARGET;
            if near {
                match self.route[i] {
                    Route::Direct | Route::Waiting | Route::Rerouting => self.goal[i] = t,
                    Route::Waypoints if !self.partial[i] => {
                        // Keep aiming at the target unless the route ends
                        // short of it (the target was on blocked ground).
                        let last = i * MAX_WAYPOINTS + self.way_len[i] as usize - 1;
                        if self.way[last] == self.goal[i] {
                            self.way[last] = t;
                        }
                        self.goal[i] = t;
                    }
                    _ => {}
                }
                continue;
            }
            self.goal[i] = t;
            let grid = grid_of(regs.owner[i]);
            let (from, to) = (grid.cell_at(regs.position[i]), grid.cell_at(t));
            let need = grid.need(from, to, regs.berth(units, i));
            self.later.remove(i);
            if grid.open_cell(from)
                && grid.open_cell(to)
                && grid.segment_within(regs.position[i], t, need, u8::MAX, &mut self.search.work)
            {
                self.route[i] = Route::Direct;
            } else {
                // Nothing to walk meanwhile: served ahead of those that
                // have.
                self.route[i] = Route::Waiting;
                self.queue.push(i);
            }
        }

        let mut served = 0;
        while served == 0 || self.search.work < WORK_PER_TICK {
            let Some(i) = self.queue.pop().or_else(|| self.later.pop()) else {
                break;
            };
            let Some(t) = regs.target[i] else {
                continue;
            };
            if !regs.alive[i] || !matches!(self.route[i], Route::Waiting | Route::Rerouting) {
                continue;
            }
            served += 1;
            let way = &mut self.way[i * MAX_WAYPOINTS..][..MAX_WAYPOINTS];
            let berth = regs.berth(units, i);
            let grid = grid_of(regs.owner[i]);
            match find(grid, &mut self.search, regs.position[i], t, berth, way) {
                Some((len, partial)) => {
                    self.route[i] = Route::Waypoints;
                    self.way_len[i] = len as u8;
                    self.way_next[i] = 0;
                    self.partial[i] = partial;
                    self.goal[i] = t;
                }
                None => {
                    self.route[i] = Route::Failed;
                    // A plain move order is dropped; a chase waits for its
                    // quarry to move somewhere reachable.
                    if regs.attack_target[i].is_none() {
                        regs.target[i] = None;
                    }
                }
            }
        }
    }

    /// Where regiment slot `i` should walk now on its way to `target`, and
    /// whether that is the end of the route. `None`: stand and wait.
    pub(crate) fn steer(&self, i: usize, target: FxVec2) -> Option<(FxVec2, bool)> {
        if !self.has_map() {
            return Some((target, true));
        }
        match self.route[i] {
            Route::Direct => Some((target, true)),
            // A rerouting regiment walks on until its search is served; its
            // route is partial, so never at its end.
            Route::Waypoints | Route::Rerouting => {
                let n = self.way_next[i] as usize;
                let last = n + 1 == self.way_len[i] as usize && !self.partial[i];
                Some((self.way[i * MAX_WAYPOINTS + n], last))
            }
            Route::Idle | Route::Waiting | Route::Failed => None,
        }
    }

    /// Regiment slot `i` reached the waypoint [`Nav::steer`] gave it. The
    /// rest of a route that stops short of its goal is searched for while
    /// the regiment walks its last stretch, so it doesn't stop for it.
    pub(crate) fn reached(&mut self, i: usize) {
        self.way_next[i] += 1;
        let left = self.way_len[i] - self.way_next[i];
        match (self.route[i], left) {
            (Route::Waypoints, 1) if self.partial[i] => self.reroute_now(i),
            (Route::Waypoints | Route::Rerouting, 0) if self.partial[i] => {
                // Nothing left to walk: served ahead of those still walking.
                self.later.remove(i);
                self.route[i] = Route::Waiting;
                self.queue.push(i);
            }
            (Route::Waypoints | Route::Rerouting, 0) => self.stop(i),
            _ => {}
        }
    }

    /// The waypoints slot `i` has still to walk.
    fn ahead(&self, i: usize) -> &[FxVec2] {
        &self.way[i * MAX_WAYPOINTS..][self.way_next[i] as usize..self.way_len[i] as usize]
    }

    /// Queue a search for regiment slot `i` while it keeps walking the way
    /// it has ([`Route::Rerouting`]): a straight walk becomes a waypoint at
    /// its old goal. Changes nothing when it has no way to walk.
    pub(crate) fn reroute(&mut self, i: usize) {
        match self.route[i] {
            Route::Direct => {
                self.way[i * MAX_WAYPOINTS] = self.goal[i];
                self.way_len[i] = 1;
                self.way_next[i] = 0;
                self.partial[i] = false;
            }
            Route::Waypoints | Route::Rerouting if !self.ahead(i).is_empty() => {}
            _ => return,
        }
        self.reroute_now(i);
    }

    fn reroute_now(&mut self, i: usize) {
        self.route[i] = Route::Rerouting;
        self.later.push(i);
    }

    /// Regiment slot `i` has no route, nor a search queued while it walks.
    fn stop(&mut self, i: usize) {
        self.route[i] = Route::Idle;
        self.later.remove(i);
    }

    /// Whether a step from `from` to `to` by a regiment of `party`'s would
    /// walk onto or across blocked ground (a step of several cells can't
    /// jump a thin wall). Regiments already standing on blocked ground may
    /// step anywhere, so they can walk off it.
    pub(crate) fn blocks(&self, party: PlayerId, from: FxVec2, to: FxVec2) -> bool {
        self.grid_of(party).is_some_and(|g| {
            g.cost[g.cell_at(from) as usize] != BLOCKED
                && (g.cost[g.cell_at(to) as usize] == BLOCKED
                    || g.on_map(from)
                        && g.on_map(to)
                        && !g.segment_within(from, to, CLEARANCE, u8::MAX, &mut 0))
        })
    }

    /// Where a step from `from` toward `to` ends: `to`, unless that is
    /// blocked ground (see [`Nav::blocks`]); then the step slides along the
    /// edge, keeping only its part along x or along y (the longer first)
    /// if that stays off it and is at least half the longer part. `None`
    /// otherwise: the way is shut (a regiment heading into a wall, not
    /// glancing off it, finds a new route rather than creep along it).
    pub(crate) fn slide(&self, party: PlayerId, from: FxVec2, to: FxVec2) -> Option<FxVec2> {
        if !self.blocks(party, from, to) {
            return Some(to);
        }
        let d = to - from;
        let (ax, ay) = (d.x.abs(), d.y.abs());
        let longer = ax.max(ay);
        let along_x = (ax * 2 >= longer).then(|| FxVec2::new(to.x, from.y));
        let along_y = (ay * 2 >= longer).then(|| FxVec2::new(from.x, to.y));
        let tries = if ax >= ay {
            [along_x, along_y]
        } else {
            [along_y, along_x]
        };
        tries
            .into_iter()
            .flatten()
            .find(|&p| p != from && !self.blocks(party, from, p))
    }

    /// Whether `p` is on ground blocked to `party` (never with no map).
    pub(crate) fn blocked(&self, party: PlayerId, p: FxVec2) -> bool {
        self.grid_of(party)
            .is_some_and(|g| g.cost[g.cell_at(p) as usize] == BLOCKED)
    }

    /// Regiment slot `i` was stopped by blocked ground: find a new route.
    pub(crate) fn lost(&mut self, i: usize) {
        self.stop(i);
    }

    /// Give regiment slot `i` the route it has in `from`, a world on the same
    /// map; if it is waiting for a search there, it waits here too (behind
    /// any copied before it).
    pub(crate) fn copy_route(&mut self, from: &Nav, i: usize) {
        self.route[i] = from.route[i];
        self.goal[i] = from.goal[i];
        self.way[i * MAX_WAYPOINTS..][..MAX_WAYPOINTS]
            .copy_from_slice(&from.way[i * MAX_WAYPOINTS..][..MAX_WAYPOINTS]);
        self.way_len[i] = from.way_len[i];
        self.way_next[i] = from.way_next[i];
        self.partial[i] = from.partial[i];
    }

    /// Queue the searches waiting in `from` for the regiments `copied` says
    /// were copied here, in the same order.
    pub(crate) fn copy_queue(&mut self, from: &Nav, copied: impl Fn(usize) -> bool) {
        for i in from.queue.iter() {
            let i = i as usize;
            if copied(i) {
                self.queue.push(i);
            }
        }
        for i in from.later.iter() {
            let i = i as usize;
            if copied(i) {
                self.later.push(i);
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        if !self.has_map() {
            return;
        }
        for i in 0..self.route.len() {
            h.u8(self.route[i] as u8);
            if self.route[i] == Route::Idle {
                continue;
            }
            h.i32(self.goal[i].x.raw());
            h.i32(self.goal[i].y.raw());
            h.u8(self.way_len[i]);
            h.u8(self.way_next[i]);
            h.u8(u8::from(self.partial[i]));
            for p in &self.way[i * MAX_WAYPOINTS..][..self.way_len[i] as usize] {
                h.i32(p.x.raw());
                h.i32(p.y.raw());
            }
        }
        for queue in [&self.queue, &self.later] {
            h.u32(queue.len as u32);
            for i in queue.iter() {
                h.u32(i);
            }
        }
    }
}

/// Find a route from `from` to `to` for a regiment wanting `berth` (cells
/// kept between its centre and blocked ground, where there is room),
/// writing waypoints into `out`. Returns how many, and whether they stop
/// short of the goal (more to search for later); `None` when there is no
/// way there.
/// The grid of `grids` that `party`'s regiments walk, as `view` says.
fn grid_for<'a>(
    grids: &'a [Arc<Grid>],
    view: &[u8; MAX_PARTIES],
    party: PlayerId,
) -> Option<&'a Grid> {
    let v = view.get(usize::from(party)).copied().unwrap_or(0);
    grids.get(usize::from(v)).map(|g| &**g)
}

fn find(
    grid: &Grid,
    s: &mut Search,
    from: FxVec2,
    to: FxVec2,
    berth: u8,
    out: &mut [FxVec2],
) -> Option<(usize, bool)> {
    // Too close to blocked ground to start: first step out to open ground.
    let mut start = grid.cell_at(from);
    let mut lead = false;
    if !grid.open_cell(start) {
        // Out of a flood, as far as a goal is looked for: a wide ford's
        // middle is further from its banks than ground is from a wall.
        let snap = if grid.is_flooded(start as usize) {
            SNAP_GOAL
        } else {
            SNAP_START
        };
        start = grid.nearest_open(start, None, snap, &mut s.work)?;
        lead = true;
    }
    // A goal on blocked ground or cut off from the start: go as near as we can.
    let area = grid.area[start as usize];
    let mut goal = grid.cell_at(to);
    let mut goal_point = to;
    if !grid.open_cell(goal) || grid.area[goal as usize] != area {
        goal = grid.nearest_open(goal, Some(area), SNAP_GOAL, &mut s.work)?;
        goal_point = grid.centre(goal);
    }

    s.chain.clear();
    s.chain.push(start);
    let mut truncated = false;
    let mut via_portals = false;
    let (start_sector, goal_sector) = (grid.sector_of(start), grid.sector_of(goal));
    let straight = |s: &mut Search, a: u32, b: u32| {
        grid.line_within(a, b, grid.need(a, b, berth), u8::MAX, &mut s.work)
    };
    // Where the march starts: here, or the open cell stepped out to.
    let start_point = if lead { grid.centre(start) } else { from };
    let need = grid.need(start, goal, berth);
    if grid.segment_within(start_point, goal_point, need, u8::MAX, &mut s.work) {
        s.chain.push(goal);
    } else if start_sector == goal_sector && s.sector(grid, start_sector, start, Some(goal), berth)
    {
        truncated = !s.append_sector_route(grid, start_sector, goal);
    } else {
        if !s.portals(grid, start, goal, berth) {
            return None;
        }
        via_portals = true;
        // Fill in the legs between portals that aren't straight lines, as
        // far as [`FILL_PORTALS`]: the rest is searched again while the
        // regiment walks the last stretch of this part (see `reached`).
        let mut prev = start;
        for k in 1..s.coarse.len() {
            if k > FILL_PORTALS {
                truncated = true;
                break;
            }
            let c = s.coarse[k];
            if c == prev {
                continue;
            }
            let sec = grid.sector_of(prev);
            let filled = sec == grid.sector_of(c)
                && !straight(s, prev, c)
                && s.sector(grid, sec, prev, Some(c), berth)
                && s.append_sector_route(grid, sec, c);
            if !filled {
                if s.chain.len() == s.chain.capacity() {
                    truncated = true;
                    break;
                }
                s.chain.push(c);
            }
            prev = c;
        }
    }
    let (n, partial) = smooth(
        grid,
        s,
        lead,
        start_point,
        goal_point,
        truncated,
        berth,
        out,
    );
    if via_portals {
        spread_through_portals(grid, s, usize::from(lead), from, n, berth, out);
    }
    tighten(grid, s, usize::from(lead), from, n, berth, out);
    Some((n, partial))
}

/// Waypoints at portals sit where the portal is, the middle of its stretch
/// of sector edge, so regiments marching side by side would all funnel
/// through the same spot and pile into each other. Slide each portal
/// waypoint along its edge toward where the straight line from the
/// waypoint before to the next one that isn't a portal (a turn round
/// something, or the route's end) crosses it, by up to [`PORTAL_SPAN`]
/// cells, as long as the legs either side stay on open, plain (cost 1)
/// ground, as far from blocked ground as the route went (up to `berth`):
/// neighbours keep their spacing across open country, and the route still
/// bends where the search said. Waypoints before `skip` stay
/// put, as does the last. Only for routes that went through portals (the
/// search's coarse route lists them).
fn spread_through_portals(
    grid: &Grid,
    s: &mut Search,
    skip: usize,
    from: FxVec2,
    n: usize,
    berth: u8,
    out: &mut [FxVec2],
) {
    let portals = s.coarse.len().saturating_sub(1);
    let is_portal = |s: &Search, p: FxVec2| s.coarse[1..portals].contains(&grid.cell_at(p));
    let mut prev = if skip > 0 { out[skip - 1] } else { from };
    for i in skip..n.saturating_sub(1) {
        if !is_portal(s, out[i]) {
            prev = out[i];
            continue;
        }
        let c = grid.cell_at(out[i]);
        let (cx, cy) = grid.xy(c);
        // Portals sit on the cells either side of a sector edge: slide
        // along x on an edge between sectors above and below, along y on
        // one between sectors side by side.
        let on_edge = |v: i32| v % SECTOR == 0 || v % SECTOR == SECTOR - 1;
        let along_x = on_edge(cy) && !on_edge(cx);
        let aim = out[i + 1..n]
            .iter()
            .copied()
            .find(|&w| !is_portal(s, w))
            .unwrap_or(out[n - 1]);
        let (p, e, here) = if along_x {
            ((prev.y, prev.x), (aim.y, aim.x), (out[i].y, cx))
        } else {
            ((prev.x, prev.y), (aim.x, aim.y), (out[i].x, cy))
        };
        // Where the line from `prev` to `aim` crosses the edge, if it does.
        let (across, run) = (i64::from((e.0 - p.0).raw()), i64::from((e.1 - p.1).raw()));
        let to_edge = i64::from((here.0 - p.0).raw());
        if across == 0 || to_edge.signum() != across.signum() || to_edge.abs() > across.abs() {
            prev = out[i];
            continue;
        }
        let cross = i64::from(p.1.raw()) + run * to_edge / across;
        let target = ((cross * i64::from(CELLS_PER_TILE)) >> 16) as i32;
        let shift = (target - here.1).clamp(-PORTAL_SPAN, PORTAL_SPAN);
        let next = out[i + 1];
        let (from_cell, to_cell) = (grid.cell_at(prev), grid.cell_at(next));
        let need = grid.need3(from_cell, c, to_cell, berth);
        // Across plain ground only, unless the portal itself is in a ford
        // or on a slope: then across ground no worse than it, so blocks
        // wade a ford side by side instead of in one file.
        let max_cost = grid.cost[c as usize].max(1);
        for shift in [shift, shift * 3 / 4, shift / 2, shift / 4] {
            let (x, y) = if along_x {
                (cx + shift, cy)
            } else {
                (cx, cy + shift)
            };
            if shift == 0 || !grid.inside(x, y) {
                break;
            }
            let moved = grid.centre(grid.idx(x, y));
            if grid.legs_keep([prev, moved, next], need, max_cost, &mut s.work) {
                out[i] = moved;
                break;
            }
        }
        prev = out[i];
    }
}

/// Smoothing keeps only cells of the chain the search found, and the
/// search runs down the middle of narrow ground, so every route through a
/// ford would cross on its middle line, in single file. Pull each turn of
/// the route toward the straight line between its neighbours instead, as
/// far as keeps the berth the route had there (up to all the way): a
/// block coming from the west crosses on the west side of the ford, one
/// from the east on the east side, each still a berth from the banks.
/// Waypoints before `skip` stay put, as does the last; a turn never moves
/// onto costlier ground than its own or its neighbours'.
fn tighten(
    grid: &Grid,
    s: &mut Search,
    skip: usize,
    from: FxVec2,
    n: usize,
    berth: u8,
    out: &mut [FxVec2],
) {
    let mut prev = if skip > 0 { out[skip - 1] } else { from };
    for i in skip..n.saturating_sub(1) {
        let (here, next) = (out[i], out[i + 1]);
        let (a, c, b) = (grid.cell_at(prev), grid.cell_at(here), grid.cell_at(next));
        // The nearest point to the turn on the line from `prev` to `next`.
        let ab = next - prev;
        let len2 = ab.length_sq_raw() as i64;
        let along = i64::from((here - prev).x.raw()) * i64::from(ab.x.raw())
            + i64::from((here - prev).y.raw()) * i64::from(ab.y.raw());
        if len2 == 0 {
            prev = here;
            continue;
        }
        let t = i128::from(along.clamp(0, len2));
        let part = |v: Fx| Fx::from_raw((i128::from(v.raw()) * t / i128::from(len2)) as i32);
        let on_line = FxVec2::new(prev.x + part(ab.x), prev.y + part(ab.y));
        let need = grid.need3(a, c, b, berth);
        let max_cost = grid.cost[a as usize]
            .max(grid.cost[b as usize])
            .max(grid.cost[c as usize]);
        let pull = on_line - here;
        for quarters in [4, 3, 2, 1] {
            let to = here
                + FxVec2::new(
                    Fx::from_raw(pull.x.raw() / 4 * quarters),
                    Fx::from_raw(pull.y.raw() / 4 * quarters),
                );
            if grid.cell_at(to) == c {
                break;
            }
            if grid.legs_keep([prev, to, next], need, max_cost, &mut s.work) {
                out[i] = to;
                break;
            }
        }
        prev = out[i];
    }
}

/// Pull the chain of cells tight: keep only the cells where the straight
/// line from the last kept one would come closer to blocked ground than
/// `berth`, or than the chain itself came between them.
///
/// Lines are checked from where the march really goes: `start_point` for
/// the chain's first cell and `goal_point` for its last (unless it was
/// cut short), the middles of the cells between.
#[allow(clippy::too_many_arguments)]
fn smooth(
    grid: &Grid,
    s: &mut Search,
    lead: bool,
    start_point: FxVec2,
    goal_point: FxVec2,
    truncated: bool,
    berth: u8,
    out: &mut [FxVec2],
) -> (usize, bool) {
    let mut n = 0;
    if lead {
        out[0] = grid.centre(s.chain[0]);
        n = 1;
    }
    let len = s.chain.len();
    if len == 1 {
        out[n] = goal_point;
        return (n + 1, false);
    }
    let mut partial = truncated;
    let point = |s: &Search, k: usize| match k {
        0 => start_point,
        k if k == len - 1 && !truncated => goal_point,
        k => grid.centre(s.chain[k]),
    };
    let (mut anchor, mut k) = (0, 1);
    // The least room along the chain from the anchor to `k`, up to `berth`.
    let room = |c: u32| grid.need(c, c, berth);
    let mut least = room(s.chain[0]).min(room(s.chain[1]));
    while k < len {
        if k + 1 < len {
            let need = least.min(room(s.chain[k + 1]));
            if grid.span(s.chain[anchor], s.chain[k + 1]) <= SMOOTH_REACH && {
                let (a, b) = (point(s, anchor), point(s, k + 1));
                grid.segment_within(a, b, need, u8::MAX, &mut s.work)
            } {
                least = need;
                k += 1;
                continue;
            }
        }
        // A leg to the next chain cell was checked between cell centres
        // (as a straight leg between portals may run far): where it starts
        // or ends off-centre, at the regiment or its target, and doesn't
        // hold there, walk it between the centres.
        let (a, b) = (point(s, anchor), point(s, k));
        let mut bridge = [None, None];
        if k == anchor + 1
            && (anchor == 0 || k == len - 1)
            && !grid.segment_within(a, b, CLEARANCE, u8::MAX, &mut s.work)
        {
            let (ca, cb) = (grid.centre(s.chain[anchor]), grid.centre(s.chain[k]));
            bridge = [(a != ca).then_some(ca), (b != cb).then_some(cb)];
        }
        let extra = bridge.iter().flatten().count();
        if n + extra >= out.len() {
            partial = true;
            break;
        }
        for p in bridge.into_iter().flatten() {
            out[n] = p;
            n += 1;
        }
        out[n] = b;
        n += 1;
        anchor = k;
        k += 1;
        if k < len {
            least = room(s.chain[anchor]).min(room(s.chain[k]));
        }
    }
    (n, partial)
}

impl Grid {
    /// The grid of the cells' `costs`, `side` cells across, with nothing
    /// standing on it.
    #[cfg(test)]
    fn build(costs: &[u8], side: i32) -> Grid {
        Grid::build_standing(costs, side, core::iter::empty())
    }

    /// The grid of the cells' `costs` with something standing on each of
    /// `standing`'s tiles, as a building would (see [`Nav::standing`]).
    fn build_standing(costs: &[u8], side: i32, standing: impl Iterator<Item = [i32; 4]>) -> Grid {
        let n = (side * side) as usize;
        let mut grid = Grid {
            side,
            cost: costs.to_vec(),
            ground: costs.to_vec(),
            built: vec![0; n],
            opened: Vec::new(),
            flooded: Vec::new(),
            bridged: Vec::new(),
            frozen: false,
            clear: vec![0; n],
            area: vec![0; n],
            areas: 0,
            sectors: (side + SECTOR - 1) / SECTOR,
            node_cell: Vec::new(),
            pair_first: Vec::new(),
            sector_first: Vec::new(),
            sector_nodes: Vec::new(),
            edge_first: Vec::new(),
            edge_to: Vec::new(),
            edge_cost: Vec::new(),
            node_room: 0,
        };
        for t in standing {
            let Some([x0, y0, x1, y1]) = grid.cells(t) else {
                continue;
            };
            for cy in y0..y1 {
                for cx in x0..x1 {
                    let i = grid.idx(cx, cy) as usize;
                    grid.built[i] = grid.built[i].saturating_add(1);
                    grid.cost[i] = BLOCKED;
                }
            }
        }
        grid.measure_clearance_in(0, 0, side, side);
        grid.number_areas();
        let partner = grid.place_portals(None);
        grid.link_portals(&partner, &mut Search::new());
        grid.node_room = grid.node_room();
        grid
    }

    /// The most the portal search's heap may hold over this grid's graph:
    /// every link, and each end's links into its sector, with room to
    /// spare.
    fn node_room(&self) -> usize {
        let (_, edges, most) = Search::room(self);
        edges + 2 * most + 8
    }

    /// The cells of the tiles `x..x + w` × `y..y + d`: `[x0, y0, x1, y1]`,
    /// ends exclusive. `None` unless there are some and all are on the map.
    fn cells(&self, [x, y, w, d]: [i32; 4]) -> Option<[i32; 4]> {
        let c = [x, y, x + w, y + d].map(|v| v * CELLS_PER_TILE);
        let on = |v: i32| (0..=self.side).contains(&v);
        (c.iter().all(|&v| on(v)) && c[0] < c[2] && c[1] < c[3]).then_some(c)
    }

    /// Put portals where open cells face each other across sector edges:
    /// in each stretch of edge, on the pair of cells furthest from blocked
    /// ground (the middlemost of those), so routes cross where there is
    /// most room. Returns each node's partner across its edge. With `old`,
    /// the graph before a change and a box of cells holding every cell
    /// whose clearance may have changed, edges clear of the box keep their
    /// old portals (where they would be put again anyway).
    fn place_portals(&mut self, old: Option<(&reblock::OldGraph, [i32; 4])>) -> Vec<u32> {
        // (cell on one side, cell on the other) for each portal.
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut pair_first = Vec::with_capacity(2 * (self.sectors * self.sectors) as usize + 1);
        pair_first.push(0);
        let clear = &self.clear;
        let add_run =
            |pairs: &mut Vec<(u32, u32)>, a: i32, b: i32, cells: &dyn Fn(i32) -> (u32, u32)| {
                let len = b - a;
                let pieces = (len + PORTAL_SPAN - 1) / PORTAL_SPAN;
                for j in 0..pieces {
                    let (pa, pb) = (a + j * len / pieces, a + (j + 1) * len / pieces);
                    let middle = (pa + pb - 1) / 2;
                    let room = |t: i32| {
                        let (u, v) = cells(t);
                        clear[u as usize].min(clear[v as usize])
                    };
                    let best = (pa..pb)
                        .max_by_key(|&t| (room(t), -(t - middle).abs(), -t))
                        .unwrap_or(middle);
                    pairs.push(cells(best));
                }
            };
        for s in 0..self.sectors * self.sectors {
            let (x0, y0, x1, y1) = self.sector_bounds(s as u32);
            // The edge to the east neighbour, then the one to the south.
            for east in [true, false] {
                let (span, inner, outer) = if east {
                    (y0..y1, x1 - 1, x1)
                } else {
                    (x0..x1, y1 - 1, y1)
                };
                let e = pair_first.len() - 1;
                if outer < self.side {
                    // Whether the cells either side of the edge are clear of the box.
                    let clear_of = |[bx0, by0, bx1, by1]: [i32; 4]| {
                        if east {
                            outer < bx0 || inner >= bx1 || y1 <= by0 || y0 >= by1
                        } else {
                            outer < by0 || inner >= by1 || x1 <= bx0 || x0 >= bx1
                        }
                    };
                    if let Some((old, _)) = old.filter(|&(_, b)| clear_of(b)) {
                        for k in old.pair_first[e]..old.pair_first[e + 1] {
                            let k = 2 * k as usize;
                            pairs.push((old.node_cell[k], old.node_cell[k + 1]));
                        }
                    } else {
                        let cells = |t: i32| {
                            if east {
                                (self.idx(inner, t), self.idx(outer, t))
                            } else {
                                (self.idx(t, inner), self.idx(t, outer))
                            }
                        };
                        let mut run = None;
                        for t in span.start..=span.end {
                            let both = t < span.end && {
                                let (a, b) = cells(t);
                                self.open_cell(a) && self.open_cell(b)
                            };
                            match (both, run) {
                                (true, None) => run = Some(t),
                                (false, Some(a)) => {
                                    add_run(&mut pairs, a, t, &cells);
                                    run = None;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                pair_first.push(pairs.len() as u32);
            }
        }
        self.pair_first = pair_first;
        let mut partner = Vec::with_capacity(pairs.len() * 2);
        for (k, &(a, b)) in pairs.iter().enumerate() {
            self.node_cell.push(a);
            self.node_cell.push(b);
            partner.push(2 * k as u32 + 1);
            partner.push(2 * k as u32);
        }
        // Group the nodes by sector.
        let sectors = (self.sectors * self.sectors) as usize;
        self.sector_first = vec![0; sectors + 1];
        for &c in &self.node_cell {
            let s = self.sector_of(c) as usize;
            self.sector_first[s + 1] += 1;
        }
        for s in 0..sectors {
            self.sector_first[s + 1] += self.sector_first[s];
        }
        let mut fill = self.sector_first.clone();
        self.sector_nodes = vec![0; self.node_cell.len()];
        for (n, &c) in self.node_cell.iter().enumerate() {
            let s = self.sector_of(c) as usize;
            self.sector_nodes[fill[s] as usize] = n as u32;
            fill[s] += 1;
        }
        partner
    }

    /// Link each portal node to its partner across the edge and to every
    /// node it can reach inside its own sector, with the walking cost.
    fn link_portals(&mut self, partner: &[u32], search: &mut Search) {
        let costs: Vec<Vec<u32>> = (0..self.sector_first.len() - 1)
            .map(|sec| self.sector_costs(search, sec))
            .collect();
        let place = reblock::places(&self.sector_first, &self.sector_nodes, partner.len());
        let (mut first, mut to, mut cost) = (
            Vec::with_capacity(partner.len() + 1),
            Vec::new(),
            Vec::new(),
        );
        first.push(0);
        for (u, &p) in partner.iter().enumerate() {
            self.link_across(u, p, &mut to, &mut cost);
            let sec = self.sector_of(self.node_cell[u]) as usize;
            self.link_within(u, place[u], &costs[sec], &mut to, &mut cost);
            first.push(to.len() as u32);
        }
        self.edge_first = first;
        self.edge_to = to;
        self.edge_cost = cost;
    }

    /// Node `u`'s first link: to its partner `p` across the sector edge.
    fn link_across(&self, u: usize, p: u32, to: &mut Vec<u32>, cost: &mut Vec<u32>) {
        to.push(p);
        cost.push(self.step_cost(
            self.node_cell[u],
            self.node_cell[p as usize],
            false,
            GRAPH_BERTH,
        ));
    }

    /// Node `u`'s links to the other nodes it can reach inside its sector,
    /// in their order there, from the sector's [`Grid::sector_costs`];
    /// `place` is `u`'s own place in that order.
    fn link_within(
        &self,
        u: usize,
        place: u32,
        costs: &[u32],
        to: &mut Vec<u32>,
        cost: &mut Vec<u32>,
    ) {
        let nodes = self.nodes_in(self.sector_of(self.node_cell[u]) as usize);
        let row = &costs[place as usize * nodes.len()..][..nodes.len()];
        for (k, (&v, &c)) in nodes.iter().zip(row).enumerate() {
            if k != place as usize && c != NONE {
                to.push(v);
                cost.push(c);
            }
        }
    }

    /// The walking cost between every two portal nodes of sector `sec`
    /// inside it (row by row, in the sector's order; [`NONE`] where there
    /// is no way). A way costs the same both ways, so each node's search
    /// only looks for the nodes after it, in its own area, and stops once
    /// it has found them.
    fn sector_costs(&self, search: &mut Search, sec: usize) -> Vec<u32> {
        let nodes = self.nodes_in(sec);
        let n = nodes.len();
        let mut costs = vec![NONE; n * n];
        let mut wanted = Vec::with_capacity(n);
        if n > 1 {
            search.weigh(self, sec as u32, GRAPH_BERTH);
        }
        for (a, &u) in nodes.iter().enumerate() {
            let cell = self.node_cell[u as usize];
            let area = self.area[cell as usize];
            wanted.clear();
            wanted.extend(
                nodes[a + 1..]
                    .iter()
                    .map(|&v| self.node_cell[v as usize])
                    .filter(|&c| self.area[c as usize] == area),
            );
            if wanted.is_empty() {
                continue;
            }
            search.reach(self, sec as u32, cell, &wanted);
            for (b, &v) in nodes.iter().enumerate().skip(a + 1) {
                let to = self.node_cell[v as usize];
                if self.area[to as usize] != area {
                    continue;
                }
                if let Some(c) = search.cost_to(self, sec as u32, to) {
                    costs[a * n + b] = c;
                    costs[b * n + a] = c;
                }
            }
        }
        costs
    }

    /// The portal nodes of sector `s`, in order.
    fn nodes_in(&self, s: usize) -> &[u32] {
        &self.sector_nodes[self.sector_first[s] as usize..self.sector_first[s + 1] as usize]
    }
}
