//! Ground that changes mid-match: buildings put up and pulled down (see
//! [`crate::structures`]). A change blocks or frees a rectangle of cells;
//! the walkable grid is brought up to date around it instead of being
//! built again, so a building going up costs a few milliseconds, not the
//! whole map's worth of searches:
//!
//! - **Clearance** is measured again only within [`CLEAR_CAP`] cells of the
//!   change: past that, clearances are capped and can't have moved.
//! - **Areas** are numbered again across the map (one cheap flood fill), as
//!   a wall can cut one area in two anywhere along it.
//! - **Portals** are placed again everywhere (cheap), but linked again only
//!   in the sectors the change touched, or whose portals moved because a
//!   neighbour's did: every other sector's links are copied across.
//!
//! The result is exactly what building the grid from scratch would give
//! (a test checks), so a match's grid never depends on the order its
//! buildings went up in. This runs inside a tick but only when the ground
//! changes, and it may allocate (the portal graph can grow).

use alloc::vec;
use alloc::vec::Vec;

use super::{BLOCKED, CLEAR_CAP, GRAPH_BERTH, Grid, SECTOR, Search};

impl Grid {
    /// Bring the grid up to date after the cells `x0..x1` × `y0..y1` (cell
    /// coordinates, end exclusive) changed cost, searching in `search`'s
    /// room and leaving it room for searches over the new graph.
    pub(super) fn changed(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, search: &mut Search) {
        let reach = i32::from(CLEAR_CAP) + 1;
        let (wx0, wy0) = ((x0 - reach).max(0), (y0 - reach).max(0));
        let (wx1, wy1) = ((x1 + reach).min(self.side), (y1 + reach).min(self.side));
        self.measure_clearance_in(wx0, wy0, wx1, wy1);
        self.area.fill(0);
        self.label_areas();
        // Sectors a cell whose clearance may have changed lies in.
        let mut dirty = vec![false; (self.sectors * self.sectors) as usize];
        for sy in wy0 / SECTOR..=(wy1 - 1) / SECTOR {
            for sx in wx0 / SECTOR..=(wx1 - 1) / SECTOR {
                dirty[(sy * self.sectors + sx) as usize] = true;
            }
        }
        let old = OldGraph::take(self);
        let partner = self.place_portals();
        // A sector whose portals moved (a neighbour's edge changed) is
        // linked again too.
        for (s, d) in dirty.iter_mut().enumerate() {
            if !*d && !old.cells_of(s).eq(self.cells_of(s)) {
                *d = true;
            }
        }
        self.relink(&partner, &old, &dirty, search);
        search.fit(self);
    }

    /// Chessboard distance from each cell in the window `x0..x1` × `y0..y1`
    /// to the nearest blocked cell (the ground beyond the map edge counts
    /// as blocked), up to [`CLEAR_CAP`], in two sweeps, reading the cells
    /// just outside the window as they stand. Over the whole map this is
    /// the grid's clearance; over a window it is exact as long as every
    /// cell outside it is more than [`CLEAR_CAP`] from the cells that
    /// changed.
    pub(super) fn measure_clearance_in(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = self.idx(x, y) as usize;
                self.clear[i] = if self.cost[i] == BLOCKED {
                    0
                } else {
                    CLEAR_CAP
                };
            }
        }
        let side = self.side;
        let whole = x0 == 0 && y0 == 0 && x1 == side && y1 == side;
        let at = |clear: &[u8], x: i32, y: i32| -> u8 {
            if x < 0 || y < 0 || x >= side || y >= side {
                0
            } else {
                clear[(y * side + x) as usize]
            }
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * side + x) as usize;
                if self.clear[i] == 0 {
                    continue;
                }
                let c = &self.clear;
                let mut d = at(c, x - 1, y)
                    .min(at(c, x - 1, y - 1))
                    .min(at(c, x, y - 1))
                    .min(at(c, x + 1, y - 1));
                // In a window, every neighbour, so the cells just outside it
                // on the far sides are read too (over the whole map there
                // are none, and the cells ahead are still to be measured).
                if !whole {
                    d = d
                        .min(at(c, x + 1, y))
                        .min(at(c, x + 1, y + 1))
                        .min(at(c, x, y + 1))
                        .min(at(c, x - 1, y + 1));
                }
                self.clear[i] = self.clear[i].min(d.saturating_add(1).min(CLEAR_CAP));
            }
        }
        for y in (y0..y1).rev() {
            for x in (x0..x1).rev() {
                let i = (y * side + x) as usize;
                if self.clear[i] == 0 {
                    continue;
                }
                let c = &self.clear;
                let d = at(c, x + 1, y)
                    .min(at(c, x + 1, y + 1))
                    .min(at(c, x, y + 1))
                    .min(at(c, x - 1, y + 1));
                self.clear[i] = self.clear[i].min(d.saturating_add(1).min(CLEAR_CAP));
            }
        }
    }

    /// The node cells of sector `s`, in order.
    fn cells_of(&self, s: usize) -> impl Iterator<Item = u32> + '_ {
        self.nodes_in(s).iter().map(|&n| self.node_cell[n as usize])
    }

    /// [`Grid::link_portals`], searching only in `dirty` sectors and copying
    /// the links of every other sector from `old`.
    fn relink(&mut self, partner: &[u32], old: &OldGraph, dirty: &[bool], search: &mut Search) {
        let nodes = self.node_cell.len();
        let place = places(&self.sector_first, &self.sector_nodes, nodes);
        let (mut first, mut to, mut cost) = (
            Vec::with_capacity(nodes + 1),
            Vec::with_capacity(old.edge_to.len()),
            Vec::with_capacity(old.edge_to.len()),
        );
        first.push(0);
        for (u, &p) in partner.iter().enumerate() {
            let cell = self.node_cell[u];
            to.push(p);
            cost.push(self.step_cost(cell, self.node_cell[p as usize], false, GRAPH_BERTH));
            let s = self.sector_of(cell) as usize;
            if dirty[s] {
                self.link_in_sector(search, u, &mut to, &mut cost);
            } else {
                // The same node, by its place in the sector, in the old
                // graph; its first edge was to its partner.
                let was = old.sector_nodes[old.sector_first[s] as usize + place[u] as usize];
                let nodes_here = self.nodes_in(s);
                for e in old.edge_first[was as usize] + 1..old.edge_first[was as usize + 1] {
                    let v = old.edge_to[e as usize];
                    to.push(nodes_here[old.place[v as usize] as usize]);
                    cost.push(old.edge_cost[e as usize]);
                }
            }
            first.push(to.len() as u32);
        }
        self.edge_first = first;
        self.edge_to = to;
        self.edge_cost = cost;
    }
}

/// Each node's place in its sector's list, from a graph's sector lists.
fn places(sector_first: &[u32], sector_nodes: &[u32], nodes: usize) -> Vec<u32> {
    let mut place = vec![0; nodes];
    for w in sector_first.windows(2) {
        for (k, &n) in sector_nodes[w[0] as usize..w[1] as usize]
            .iter()
            .enumerate()
        {
            place[n as usize] = k as u32;
        }
    }
    place
}

/// The portal graph as it stood before a change.
struct OldGraph {
    node_cell: Vec<u32>,
    sector_first: Vec<u32>,
    sector_nodes: Vec<u32>,
    edge_first: Vec<u32>,
    edge_to: Vec<u32>,
    edge_cost: Vec<u32>,
    /// Each node's place in its sector's list.
    place: Vec<u32>,
}

impl OldGraph {
    /// The grid's portal graph, leaving it empty for placing again.
    fn take(grid: &mut Grid) -> OldGraph {
        let sector_first = core::mem::take(&mut grid.sector_first);
        let sector_nodes = core::mem::take(&mut grid.sector_nodes);
        let node_cell = core::mem::take(&mut grid.node_cell);
        OldGraph {
            place: places(&sector_first, &sector_nodes, node_cell.len()),
            node_cell,
            sector_first,
            sector_nodes,
            edge_first: core::mem::take(&mut grid.edge_first),
            edge_to: core::mem::take(&mut grid.edge_to),
            edge_cost: core::mem::take(&mut grid.edge_cost),
        }
    }

    fn cells_of(&self, s: usize) -> impl Iterator<Item = u32> + '_ {
        self.sector_nodes[self.sector_first[s] as usize..self.sector_first[s + 1] as usize]
            .iter()
            .map(|&n| self.node_cell[n as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::CELLS_PER_TILE;
    use crate::scenario::ribs_map;
    use gc_fixed::Pcg32;

    /// Every part of the grid that routes read.
    fn same(a: &Grid, b: &Grid) {
        assert!(a.cost == b.cost, "costs differ");
        for (i, (x, y)) in a.clear.iter().zip(&b.clear).enumerate() {
            assert_eq!(x, y, "clearance differs at cell {i}");
        }
        assert!(a.area == b.area, "areas differ");
        assert_eq!(a.node_cell, b.node_cell, "portals differ");
        assert_eq!(a.sector_first, b.sector_first);
        assert_eq!(a.sector_nodes, b.sector_nodes);
        assert_eq!(a.edge_first, b.edge_first, "links differ");
        assert_eq!(a.edge_to, b.edge_to, "links differ");
        assert_eq!(a.edge_cost, b.edge_cost, "link costs differ");
    }

    #[test]
    fn a_changed_grid_is_the_grid_built_afresh() {
        let tiles: i32 = 96;
        let side = tiles * CELLS_PER_TILE;
        let ground = ribs_map(tiles as u32);
        let mut g = Grid::build(&ground, side);
        let mut search = Search::for_grid(&g);
        let mut build_on = |g: &mut Grid, b, on| {
            assert!(g.set_built(b, on, &mut search).is_some());
        };
        let mut rng = Pcg32::new(11, 1);
        let mut standing: Vec<[i32; 4]> = Vec::new();
        for step in 0..40 {
            // Mostly put up, sometimes pull one down; some by the map edge,
            // some across sector edges, some walls a tile thick.
            if standing.len() > 3 && rng.below(3) == 0 {
                let k = rng.below(standing.len() as u32) as usize;
                let b = standing.swap_remove(k);
                build_on(&mut g, b, false);
            } else {
                let (w, d) = if step % 5 == 0 {
                    (1, 2 + rng.below(6) as i32)
                } else {
                    (2 + rng.below(4) as i32, 2 + rng.below(4) as i32)
                };
                let x = rng.below((tiles - w) as u32) as i32;
                let y = rng.below((tiles - d) as u32) as i32;
                standing.push([x, y, w, d]);
                build_on(&mut g, [x, y, w, d], true);
            }
            let mut fresh = Grid::build(&g.cost, side);
            fresh.ground.clone_from(&g.ground);
            fresh.built.clone_from(&g.built);
            same(&g, &fresh);
        }
        // Everything pulled down: the map as it began.
        for b in standing.drain(..) {
            build_on(&mut g, b, false);
        }
        same(&g, &Grid::build(&ground, side));
    }
}
