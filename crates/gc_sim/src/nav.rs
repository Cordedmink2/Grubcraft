//! Pathfinding: where regiments can walk, and the routes they take.
//!
//! The map is a grid of half-tile cells, each with a cost: 0 is ground no
//! regiment can cross (cliffs, deep water), 1 is open, and higher costs are
//! slow going that routes avoid when they can. A regiment's centre keeps
//! [`CLEARANCE`] cells away from blocked ground, so "open" below means "far
//! enough from anything blocked".
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
//! Searches are budgeted: requests queue up and each tick serves them in
//! order until [`WORK_PER_TICK`] units of work are spent (at least one request
//! a tick), so a hundred regiments ordered at once can't stall a tick.

use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Reverse;

use gc_fixed::{Fx, FxVec2};

use crate::StateHasher;
use crate::regiments::Regiments;

/// Grid cells along one tile.
pub const CELLS_PER_TILE: i32 = 2;
/// Sector edge, in cells (16 tiles).
pub const SECTOR: i32 = 32;
/// Cells a regiment's centre keeps from blocked ground: a cell is open when
/// the nearest blocked cell is at least this many steps away. Three cells
/// leaves 1.25 tiles from the centre to the edge of blocked ground.
pub const CLEARANCE: u8 = 3;
/// Cost of a cell no regiment can enter.
pub const BLOCKED: u8 = 0;
/// Waypoints stored per regiment. A longer route is followed in parts: when
/// the last stored waypoint is reached, the rest is searched again.
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
/// Upper bound on cells in one route before smoothing.
const CHAIN_CAPACITY: usize = 32_768;
const NONE: u32 = u32::MAX;
/// Searches trust their distance estimate this much (in quarters): a little
/// over 1 finds routes within a quarter of the shortest, much faster.
const GREED: u32 = 5;

/// The eight neighbours; the last four are diagonals.
const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
];

/// The walkable map and its portal graph. Built once when the match starts,
/// and shared with any scratch world on the same map (see [`Nav::sharing`]).
struct Grid {
    /// Cells per side.
    side: i32,
    cost: Vec<u8>,
    /// Steps (8-way) to the nearest blocked cell or the map edge, capped at 255.
    clear: Vec<u8>,
    /// Connected area of each open cell (1, 2, ...); 0 for cells not open.
    area: Vec<u32>,
    /// Sectors per side.
    sectors: i32,
    /// Portal graph: each node is an open cell on a sector edge.
    node_cell: Vec<u32>,
    /// Nodes of sector `s` are `sector_nodes[sector_first[s]..sector_first[s + 1]]`.
    sector_first: Vec<u32>,
    sector_nodes: Vec<u32>,
    /// Edges of node `n` are `edge_first[n]..edge_first[n + 1]`.
    edge_first: Vec<u32>,
    edge_to: Vec<u32>,
    edge_cost: Vec<u32>,
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

    fn open_cell(&self, c: u32) -> bool {
        self.clear[c as usize] >= CLEARANCE
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
    /// diagonal on open ground, scaled by the cells' costs.
    fn step_cost(&self, a: u32, b: u32, diagonal: bool) -> u32 {
        let base = if diagonal { 7 } else { 5 };
        base * (u32::from(self.cost[a as usize]) + u32::from(self.cost[b as usize]))
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

    /// Whether the straight line between two cell centres stays on open
    /// cells. A line through a cell corner needs both cells beside the corner
    /// open. Adds the cells checked to `work`.
    fn line_open(&self, a: u32, b: u32, work: &mut u32) -> bool {
        self.line_within(a, b, u8::MAX, work)
    }

    /// [`Grid::line_open`], on cells costing no more than `max_cost`.
    fn line_within(&self, a: u32, b: u32, max_cost: u8, work: &mut u32) -> bool {
        let open =
            |x: i32, y: i32| self.open(x, y) && self.cost[self.idx(x, y) as usize] <= max_cost;
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
}

/// Scratch space for searches, reserved up front so ticks never allocate.
struct Search {
    /// Search within one sector, indexed by cell within the sector.
    g: Vec<u32>,
    parent: Vec<u32>,
    seen: Vec<u32>,
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
    fn new(nodes: usize, edges: usize, max_sector_nodes: usize) -> Search {
        let cells = (SECTOR * SECTOR) as usize;
        Search {
            g: vec![0; cells],
            parent: vec![NONE; cells],
            seen: vec![0; cells],
            stamp: 0,
            heap: BinaryHeap::with_capacity(cells * 8 + 8),
            node_g: vec![0; nodes + 2],
            node_parent: vec![NONE; nodes + 2],
            node_seen: vec![0; nodes + 2],
            to_goal: vec![0; nodes + 2],
            node_stamp: 0,
            node_heap: BinaryHeap::with_capacity(edges + 2 * max_sector_nodes + 8),
            links: Vec::with_capacity(max_sector_nodes),
            coarse: Vec::with_capacity(nodes + 2),
            chain: Vec::with_capacity(CHAIN_CAPACITY),
            work: 0,
        }
    }

    /// Search inside sector `sec` from cell `from`. With a goal, A* until it
    /// is reached; without one, every reachable cell of the sector. Returns
    /// whether the goal was reached (true without a goal). Afterwards
    /// [`Search::cost_to`] gives the cost to each cell reached.
    fn sector(&mut self, grid: &Grid, sec: u32, from: u32, to: Option<u32>) -> bool {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.stamp = 1;
        }
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
                let ng = g + grid.step_cost(c, n, diagonal);
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
    fn link(&mut self, grid: &Grid, sec: u32, c: u32) {
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
        self.sector(grid, sec, c, None);
        for &n in nodes {
            if let Some(cost) = self.cost_to(grid, sec, grid.node_cell[n as usize]) {
                self.links.push((n, cost));
            }
        }
    }

    /// A* over the portal graph from cell `from` to cell `to`. On success the
    /// coarse route holds `from`, the portal cells in order, and `to`.
    fn portals(&mut self, grid: &Grid, from: u32, to: u32) -> bool {
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
        self.link(grid, goal_sector, to);
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

        self.link(grid, grid.sector_of(from), from);
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
                    && s.node_heap.len() < s.node_heap.capacity()
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
}

/// The map's walkable ground and every regiment's route.
pub struct Nav {
    /// `None` on an open plane with no map (tests): everyone walks straight.
    grid: Option<Arc<Grid>>,
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
    queue: Queue,
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
}

impl Nav {
    /// `costs` holds one cost per cell, row by row, for a map `map_tiles`
    /// tiles across ([`CELLS_PER_TILE`] cells per tile). Empty means an open
    /// plane with no obstacles.
    pub fn new(costs: &[u8], map_tiles: u32, regiments: usize) -> Nav {
        let grid = (!costs.is_empty()).then(|| {
            let side = map_tiles as i32 * CELLS_PER_TILE;
            assert_eq!(
                costs.len(),
                (side * side) as usize,
                "the nav grid must have {side} x {side} cells"
            );
            Arc::new(Grid::build(costs, side))
        });
        Nav::on(grid, regiments)
    }

    /// Routes for `regiments` regiments on the same map as this one, sharing
    /// its grid and portal graph rather than building them again (scratch
    /// worlds, see `World::scratch`).
    pub fn sharing(&self, regiments: usize) -> Nav {
        Nav::on(self.grid.clone(), regiments)
    }

    fn on(grid: Option<Arc<Grid>>, regiments: usize) -> Nav {
        let search = match &grid {
            Some(g) => {
                let max_sector_nodes = g
                    .sector_first
                    .windows(2)
                    .map(|w| (w[1] - w[0]) as usize)
                    .max()
                    .unwrap_or(0);
                Search::new(g.node_cell.len(), g.edge_to.len(), max_sector_nodes)
            }
            None => Search::new(0, 0, 0),
        };
        Nav {
            grid,
            search,
            route: vec![Route::Idle; regiments],
            goal: vec![FxVec2::ZERO; regiments],
            way: vec![FxVec2::ZERO; regiments * MAX_WAYPOINTS],
            way_len: vec![0; regiments],
            way_next: vec![0; regiments],
            partial: vec![false; regiments],
            queue: Queue {
                slots: vec![0; regiments],
                head: 0,
                len: 0,
                queued: vec![false; regiments],
            },
        }
    }

    /// Whether a regiment's centre may stand at `p` (always true with no map).
    pub fn walkable(&self, p: FxVec2) -> bool {
        self.grid.as_ref().is_none_or(|g| g.open_cell(g.cell_at(p)))
    }

    /// Whether a regiment at `p` has open ground all round for at least
    /// `tiles` (always true with no map): nothing blocked that near.
    pub fn open_around(&self, p: FxVec2, tiles: i32) -> bool {
        self.grid
            .as_ref()
            .is_none_or(|g| i32::from(g.clear[g.cell_at(p) as usize]) >= tiles * CELLS_PER_TILE)
    }

    /// Whether a regiment's centre could walk the straight line from `from`
    /// to `to` (always true with no map).
    pub fn clear_line(&self, from: FxVec2, to: FxVec2) -> bool {
        self.grid
            .as_ref()
            .is_none_or(|g| g.line_open(g.cell_at(from), g.cell_at(to), &mut 0))
    }

    /// Portal nodes in the graph (for the dev overlay and tests).
    pub fn portal_count(&self) -> usize {
        self.grid.as_ref().map_or(0, |g| g.node_cell.len())
    }

    /// Regiments waiting for a route.
    pub fn queued(&self) -> usize {
        self.queue.len
    }

    /// The rest of regiment slot `i`'s route, for drawing: the waypoints
    /// still ahead, ending at the target.
    pub(crate) fn route_ahead(&self, i: usize, target: Option<FxVec2>) -> &[FxVec2] {
        match (self.grid.is_some(), self.route[i]) {
            (true, Route::Waypoints) => {
                &self.way[i * MAX_WAYPOINTS..][self.way_next[i] as usize..self.way_len[i] as usize]
            }
            (false, _) | (true, Route::Direct) if target.is_some() => {
                core::slice::from_ref(&self.goal[i])
            }
            _ => &[],
        }
    }

    /// Bring every regiment's route up to date with its target, then run the
    /// queued searches this tick's budget allows.
    pub(crate) fn plan(&mut self, regs: &mut Regiments) {
        let Some(grid) = &self.grid else {
            // Open plane: remember targets so the game can draw them.
            for i in 0..regs.alive.len() {
                if let Some(t) = regs.target[i] {
                    self.goal[i] = t;
                }
            }
            return;
        };
        self.search.work = 0;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                self.route[i] = Route::Idle;
                continue;
            }
            let Some(t) = regs.target[i] else {
                self.route[i] = Route::Idle;
                continue;
            };
            let d = t - self.goal[i];
            let near = self.route[i] != Route::Idle && d.x.abs() < RETARGET && d.y.abs() < RETARGET;
            if near {
                match self.route[i] {
                    Route::Direct | Route::Waiting => self.goal[i] = t,
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
            let (from, to) = (grid.cell_at(regs.position[i]), grid.cell_at(t));
            if grid.open_cell(from)
                && grid.open_cell(to)
                && grid.line_open(from, to, &mut self.search.work)
            {
                self.route[i] = Route::Direct;
            } else {
                self.route[i] = Route::Waiting;
                self.queue.push(i);
            }
        }

        let mut served = 0;
        while served == 0 || self.search.work < WORK_PER_TICK {
            let Some(i) = self.queue.pop() else {
                break;
            };
            let Some(t) = regs.target[i] else {
                continue;
            };
            if !regs.alive[i] || self.route[i] != Route::Waiting {
                continue;
            }
            served += 1;
            let way = &mut self.way[i * MAX_WAYPOINTS..][..MAX_WAYPOINTS];
            match find(grid, &mut self.search, regs.position[i], t, way) {
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
        if self.grid.is_none() {
            return Some((target, true));
        }
        match self.route[i] {
            Route::Direct => Some((target, true)),
            Route::Waypoints => {
                let n = self.way_next[i] as usize;
                let last = n + 1 == self.way_len[i] as usize && !self.partial[i];
                Some((self.way[i * MAX_WAYPOINTS + n], last))
            }
            Route::Idle | Route::Waiting | Route::Failed => None,
        }
    }

    /// Regiment slot `i` reached the waypoint [`Nav::steer`] gave it.
    pub(crate) fn reached(&mut self, i: usize) {
        self.way_next[i] += 1;
        if self.way_next[i] >= self.way_len[i] {
            if self.partial[i] {
                // The rest of a long route.
                self.route[i] = Route::Waiting;
                self.queue.push(i);
            } else {
                self.route[i] = Route::Idle;
            }
        }
    }

    /// Whether a step from `from` to `to` would walk onto blocked ground.
    /// Regiments already standing on blocked ground may step anywhere, so
    /// they can walk off it.
    pub(crate) fn blocks(&self, from: FxVec2, to: FxVec2) -> bool {
        self.grid.as_ref().is_some_and(|g| {
            g.cost[g.cell_at(to) as usize] == BLOCKED && g.cost[g.cell_at(from) as usize] != BLOCKED
        })
    }

    /// Whether `p` is on blocked ground (never with no map).
    pub(crate) fn blocked(&self, p: FxVec2) -> bool {
        self.grid
            .as_ref()
            .is_some_and(|g| g.cost[g.cell_at(p) as usize] == BLOCKED)
    }

    /// Regiment slot `i` was stopped by blocked ground: find a new route.
    pub(crate) fn lost(&mut self, i: usize) {
        self.route[i] = Route::Idle;
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
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        if self.grid.is_none() {
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
        h.u32(self.queue.len as u32);
        for i in self.queue.iter() {
            h.u32(i);
        }
    }
}

/// Find a route from `from` to `to`, writing waypoints into `out`. Returns
/// how many, and whether they stop short of the goal (more to search for
/// later); `None` when there is no way there.
fn find(
    grid: &Grid,
    s: &mut Search,
    from: FxVec2,
    to: FxVec2,
    out: &mut [FxVec2],
) -> Option<(usize, bool)> {
    // Too close to blocked ground to start: first step out to open ground.
    let mut start = grid.cell_at(from);
    let mut lead = false;
    if !grid.open_cell(start) {
        start = grid.nearest_open(start, None, SNAP_START, &mut s.work)?;
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
    if grid.line_open(start, goal, &mut s.work) {
        s.chain.push(goal);
    } else if start_sector == goal_sector && s.sector(grid, start_sector, start, Some(goal)) {
        truncated = !s.append_sector_route(grid, start_sector, goal);
    } else {
        if !s.portals(grid, start, goal) {
            return None;
        }
        via_portals = true;
        // Fill in the legs between portals that aren't straight lines.
        let mut prev = start;
        for k in 1..s.coarse.len() {
            let c = s.coarse[k];
            if c == prev {
                continue;
            }
            let sec = grid.sector_of(prev);
            let filled = sec == grid.sector_of(c)
                && !grid.line_open(prev, c, &mut s.work)
                && s.sector(grid, sec, prev, Some(c))
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
    let (n, partial) = smooth(grid, s, lead, goal_point, truncated, out);
    if via_portals {
        spread_through_portals(grid, s, usize::from(lead), from, n, out);
    }
    Some((n, partial))
}

/// Waypoints at portals sit where the portal is, the middle of its stretch
/// of sector edge, so regiments marching side by side would all funnel
/// through the same spot and pile into each other. Slide each portal
/// waypoint along its edge toward where the straight line from the
/// waypoint before to the next one that isn't a portal (a turn round
/// something, or the route's end) crosses it, by up to [`PORTAL_SPAN`]
/// cells, as long as the legs either side stay on open, plain (cost 1)
/// ground: neighbours keep their spacing across open country, and the
/// route still bends where the search said. Waypoints before `skip` stay
/// put, as does the last. Only for routes that went through portals (the
/// search's coarse route lists them).
fn spread_through_portals(
    grid: &Grid,
    s: &mut Search,
    skip: usize,
    from: FxVec2,
    n: usize,
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
        let (from_cell, to_cell) = (grid.cell_at(prev), grid.cell_at(out[i + 1]));
        for shift in [shift, shift / 2] {
            let (x, y) = if along_x {
                (cx + shift, cy)
            } else {
                (cx, cy + shift)
            };
            if shift == 0 || !grid.inside(x, y) {
                break;
            }
            let moved = grid.idx(x, y);
            if grid.line_within(from_cell, moved, 1, &mut s.work)
                && grid.line_within(moved, to_cell, 1, &mut s.work)
            {
                out[i] = grid.centre(moved);
                break;
            }
        }
        prev = out[i];
    }
}

/// Pull the chain of cells tight: keep only the cells where the straight
/// line from the last kept one would leave open ground.
fn smooth(
    grid: &Grid,
    s: &mut Search,
    lead: bool,
    goal_point: FxVec2,
    truncated: bool,
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
    let (mut anchor, mut k) = (0, 1);
    while k < len {
        if k + 1 < len
            && grid.span(s.chain[anchor], s.chain[k + 1]) <= SMOOTH_REACH
            && grid.line_open(s.chain[anchor], s.chain[k + 1], &mut s.work)
        {
            k += 1;
            continue;
        }
        if n == out.len() {
            partial = true;
            break;
        }
        out[n] = if k == len - 1 && !truncated {
            goal_point
        } else {
            grid.centre(s.chain[k])
        };
        n += 1;
        anchor = k;
        k += 1;
    }
    (n, partial)
}

impl Grid {
    fn build(costs: &[u8], side: i32) -> Grid {
        let n = (side * side) as usize;
        let mut grid = Grid {
            side,
            cost: costs.to_vec(),
            clear: vec![0; n],
            area: vec![0; n],
            sectors: (side + SECTOR - 1) / SECTOR,
            node_cell: Vec::new(),
            sector_first: Vec::new(),
            sector_nodes: Vec::new(),
            edge_first: Vec::new(),
            edge_to: Vec::new(),
            edge_cost: Vec::new(),
        };
        grid.measure_clearance();
        grid.label_areas();
        let partner = grid.place_portals();
        grid.link_portals(&partner);
        grid
    }

    /// Chessboard distance from every cell to the nearest blocked cell (the
    /// ground beyond the map edge counts as blocked), in two sweeps.
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

    /// Number the connected areas of open ground, so a search never starts
    /// for a goal that can't be reached.
    fn label_areas(&mut self) {
        let mut queue: Vec<u32> = Vec::with_capacity(self.cost.len());
        let mut next = 0;
        for c in 0..self.cost.len() as u32 {
            if !self.open_cell(c) || self.area[c as usize] != 0 {
                continue;
            }
            next += 1;
            self.area[c as usize] = next;
            queue.clear();
            queue.push(c);
            let mut head = 0;
            while head < queue.len() {
                let (x, y) = self.xy(queue[head]);
                head += 1;
                for (k, &(dx, dy)) in DIRS.iter().enumerate() {
                    if !self.open(x + dx, y + dy)
                        || (k >= 4 && !(self.open(x + dx, y) && self.open(x, y + dy)))
                    {
                        continue;
                    }
                    let m = self.idx(x + dx, y + dy);
                    if self.area[m as usize] == 0 {
                        self.area[m as usize] = next;
                        queue.push(m);
                    }
                }
            }
        }
    }

    /// Put portals where open cells face each other across sector edges.
    /// Returns each node's partner across its edge.
    fn place_portals(&mut self) -> Vec<u32> {
        // (cell on one side, cell on the other) for each portal.
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut add_run = |a: i32, b: i32, cells: &dyn Fn(i32) -> (u32, u32)| {
            let len = b - a;
            let pieces = (len + PORTAL_SPAN - 1) / PORTAL_SPAN;
            for j in 0..pieces {
                let (pa, pb) = (a + j * len / pieces, a + (j + 1) * len / pieces);
                pairs.push(cells((pa + pb - 1) / 2));
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
                if outer >= self.side {
                    continue;
                }
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
                            add_run(a, t, &cells);
                            run = None;
                        }
                        _ => {}
                    }
                }
            }
        }
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
    fn link_portals(&mut self, partner: &[u32]) {
        let nodes = self.node_cell.len();
        let mut search = Search::new(nodes, 0, 0);
        let (mut first, mut to, mut cost) = (Vec::with_capacity(nodes + 1), Vec::new(), Vec::new());
        first.push(0);
        for (u, &p) in partner.iter().enumerate() {
            let cell = self.node_cell[u];
            to.push(p);
            cost.push(self.step_cost(cell, self.node_cell[p as usize], false));
            let sec = self.sector_of(cell);
            search.sector(self, sec, cell, None);
            for &v in &self.sector_nodes[self.sector_first[sec as usize] as usize
                ..self.sector_first[sec as usize + 1] as usize]
            {
                if v as usize == u {
                    continue;
                }
                if let Some(c) = search.cost_to(self, sec, self.node_cell[v as usize]) {
                    to.push(v);
                    cost.push(c);
                }
            }
            first.push(to.len() as u32);
        }
        self.edge_first = first;
        self.edge_to = to;
        self.edge_cost = cost;
    }
}
