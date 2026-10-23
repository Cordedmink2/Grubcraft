//! Ground that changes mid-match: buildings put up and pulled down, and
//! gates opening and shutting (see [`crate::structures`]). A change blocks
//! or frees a rectangle of cells;
//! the walkable grid is brought up to date around it instead of being
//! built again, so a building going up costs a few milliseconds, not the
//! whole map's worth of searches:
//!
//! - **Clearance** is measured again only within [`CLEAR_CAP`] cells of the
//!   change: past that, clearances are capped and can't have moved.
//! - **Areas** are numbered again only in a box a sector wider than the
//!   change, when that box can tell (a new piece of ground, or an area cut
//!   in two where at most one side runs on out of the box). Otherwise
//!   (two areas joined, or a wall cut an area right across) they are
//!   numbered again across the map, in two passes (see
//!   [`Grid::number_areas`]) that are about ten times faster than a flood
//!   fill (on a 768-tile map a flood fill took most of the 200 ms a
//!   building cost).
//! - **Portals** are placed again only on sector edges within
//!   [`CLEAR_CAP`] of the change (a portal sits where clearance is
//!   greatest, so no other edge's can move), and linked again only in the
//!   sectors where a cell may have opened or shut (within [`CLEARANCE`] of
//!   the change), or whose portals moved because a neighbour's did: every
//!   other edge's portals and sector's links are copied across.
//!
//! The result has the same areas, portals and links as building the grid
//! from scratch (tests check), though an area's number may differ, as
//! areas are only ever compared. So a match's paths never depend on the
//! order its buildings went up in. This runs inside a tick but only when
//! the ground changes, and it may allocate (the portal graph can grow).

use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use alloc::sync::Arc;
use gc_fixed::{Fx, FxVec2};

use super::{
    BLOCKED, CELLS_PER_TILE, CLEAR_CAP, CLEARANCE, GRAPH_BERTH, Grid, Nav, Route, SECTOR, Search,
};
use crate::PlayerId;
use crate::regiments::Regiments;

/// Buildings put up or pulled down together whose footprints fit in a box
/// this many cells across are brought up to date as one change: a whole
/// base razed at once costs about one building's work, not one per
/// building (most of a change's cost is the same whatever its size).
const GROUP: i32 = 4 * SECTOR;

/// Whether a gate lets its owner's side through, and everyone else (see
/// [`Nav::set_door`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Door {
    pub(crate) own: bool,
    pub(crate) others: bool,
}

/// A change to a rectangle of cells.
#[derive(Clone, Copy)]
enum Change {
    /// A building put up (`true`) or pulled down.
    Built(bool),
    /// A door opened (`true`) or shut, on a building already standing.
    Opened(bool),
}

impl Nav {
    /// Put up (`on`) or pull down a building on each of `all`'s tiles
    /// (`[x, y, w, d]`, all on the map), on every side's grid, bringing
    /// each up to date once for each group of them close together. Routes
    /// that might cross the changed ground are searched again. No effect
    /// with no map.
    pub(crate) fn set_built_all(&mut self, all: &[[i32; 4]], on: bool, regs: &Regiments) {
        for v in 0..self.grids.len() {
            self.change(v, all, Change::Built(on), regs);
        }
    }

    /// The gate standing on `tiles` (put up with [`Nav::set_built`], so a
    /// wall to everyone until now), owned by `owner`, goes from letting
    /// through as `was` says to as `now` does. From the first gate that
    /// lets its owner's side through but not everyone, that side walks a
    /// grid of its own (see [`Nav`]).
    pub(crate) fn set_door(
        &mut self,
        tiles: [i32; 4],
        owner: PlayerId,
        was: Door,
        now: Door,
        regs: &Regiments,
    ) {
        let Some(&side) = self.side.get(usize::from(owner)) else {
            return;
        };
        if self.grids.is_empty() || was == now {
            return;
        }
        let sharing =
            (0..self.view.len()).any(|p| self.view[p] == 0 && self.parties & !side & 1 << p != 0);
        if now.own != now.others && self.view[usize::from(owner)] == 0 && sharing {
            // A copy of the shared grid, where the owner's side's gates
            // stand as for everyone (this is its first that doesn't). The
            // last side left on the shared grid keeps it as its own.
            let own = Arc::new(Grid::clone(&self.grids[0]));
            self.grids.push(own);
            let v = (self.grids.len() - 1) as u8;
            for p in 0..self.view.len() {
                if side & 1 << p != 0 {
                    self.view[p] = v;
                }
            }
        }
        let own = usize::from(self.view[usize::from(owner)]);
        for v in 0..self.grids.len() {
            let pick = |d: Door| if v == own { d.own } else { d.others };
            if pick(was) != pick(now) {
                self.change(v, &[tiles], Change::Opened(pick(now)), regs);
            }
        }
    }

    /// Make `change` on each of `all`'s tiles on grid `v`, rerouting the
    /// regiments walking it whose routes pass near.
    fn change(&mut self, v: usize, all: &[[i32; 4]], change: Change, regs: &Regiments) {
        // Linking the grid again isn't the routes' search work this tick.
        let work = self.search.work;
        let changed = Arc::make_mut(&mut self.grids[v]).change_all(all, change, &mut self.search);
        self.search.work = work;
        let freed = match change {
            Change::Built(on) => !on,
            Change::Opened(open) => open,
        };
        // Any route passing near a change is found again (walking the old
        // way meanwhile: if the change blocks it, the regiment stops at
        // the edge and looks again then), and, once ground is freed, any
        // that found no way may find one now.
        let reach = i32::from(CLEAR_CAP);
        let tile = |c: i32| Fx::from_ratio(c, CELLS_PER_TILE);
        let near: Vec<(FxVec2, FxVec2)> = changed
            .iter()
            .map(|&[x0, y0, x1, y1]| {
                (
                    FxVec2::new(tile(x0 - reach), tile(y0 - reach)),
                    FxVec2::new(tile(x1 + reach), tile(y1 + reach)),
                )
            })
            .collect();
        if near.is_empty() {
            return;
        }
        for i in 0..self.route.len() {
            if !regs.alive[i] || usize::from(self.view[usize::from(regs.owner[i])]) != v {
                continue;
            }
            let ahead = match self.route[i] {
                Route::Direct => core::slice::from_ref(&self.goal[i]),
                Route::Waypoints => self.ahead(i),
                Route::Failed if freed => {
                    self.lost(i);
                    continue;
                }
                Route::Failed | Route::Idle | Route::Waiting | Route::Rerouting => continue,
            };
            let (mut a, mut b) = (regs.position[i], regs.position[i]);
            for p in ahead {
                a = FxVec2::new(a.x.min(p.x), a.y.min(p.y));
                b = FxVec2::new(b.x.max(p.x), b.y.max(p.y));
            }
            if near
                .iter()
                .any(|(lo, hi)| a.x <= hi.x && a.y <= hi.y && b.x >= lo.x && b.y >= lo.y)
            {
                self.reroute(i);
            }
        }
    }
}

impl Grid {
    /// Put up or pull down a building on the tiles `x..x + w` × `y..y + d`
    /// and bring the grid up to date, searching in `search`'s room. Returns
    /// the cells changed; `None` (and no change) unless the tiles are on
    /// the map.
    #[cfg(test)]
    pub(super) fn set_built(
        &mut self,
        tiles: [i32; 4],
        on: bool,
        search: &mut Search,
    ) -> Option<[i32; 4]> {
        self.set_built_all(&[tiles], on, search).pop()
    }

    /// [`Grid::set_built`] for each of `all`'s tiles (those on the map),
    /// bringing the grid up to date once for each group of them that fits
    /// in a [`GROUP`]-wide box. Returns each group's box of changed cells.
    #[cfg(test)]
    pub(super) fn set_built_all(
        &mut self,
        all: &[[i32; 4]],
        on: bool,
        search: &mut Search,
    ) -> Vec<[i32; 4]> {
        self.change_all(all, Change::Built(on), search)
    }

    /// Make `change` on each of `all`'s tiles (those on the map), as
    /// [`Grid::set_built_all`] does.
    fn change_all(
        &mut self,
        all: &[[i32; 4]],
        change: Change,
        search: &mut Search,
    ) -> Vec<[i32; 4]> {
        // Each footprint's cells join the first group they fit in: the
        // group's box of cells, and its footprints' cells.
        let mut groups: Vec<([i32; 4], Vec<[i32; 4]>)> = Vec::new();
        for c in all.iter().filter_map(|&tiles| self.cells(tiles)) {
            let joined = |g: [i32; 4]| {
                [
                    g[0].min(c[0]),
                    g[1].min(c[1]),
                    g[2].max(c[2]),
                    g[3].max(c[3]),
                ]
            };
            let fits = |u: [i32; 4]| u[2] - u[0] <= GROUP && u[3] - u[1] <= GROUP;
            match groups.iter_mut().find(|(g, _)| fits(joined(*g))) {
                Some((g, feet)) => {
                    *g = joined(*g);
                    feet.push(c);
                }
                None => groups.push((c, vec![c])),
            }
        }
        if matches!(change, Change::Opened(_)) && self.opened.is_empty() {
            // Made on the first door, so maps without gates don't carry it.
            self.opened = vec![0; self.built.len()];
        }
        for (g, feet) in &groups {
            for &[cx0, cy0, cx1, cy1] in feet {
                for cy in cy0..cy1 {
                    for cx in cx0..cx1 {
                        let i = self.idx(cx, cy) as usize;
                        let (count, on) = match change {
                            Change::Built(on) => (&mut self.built[i], on),
                            Change::Opened(open) => (&mut self.opened[i], open),
                        };
                        *count = if on {
                            count.saturating_add(1)
                        } else {
                            count.saturating_sub(1)
                        };
                        let opened = self.opened.get(i).copied().unwrap_or(0);
                        self.cost[i] = if self.built[i] > opened {
                            BLOCKED
                        } else {
                            self.ground[i]
                        };
                    }
                }
            }
            let [x0, y0, x1, y1] = *g;
            self.changed(x0, y0, x1, y1, search);
        }
        groups.into_iter().map(|(g, _)| g).collect()
    }

    /// Bring the grid up to date after the cells `x0..x1` × `y0..y1` (cell
    /// coordinates, end exclusive) changed cost, searching in `search`'s
    /// room and leaving it room for searches over the new graph.
    pub(super) fn changed(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, search: &mut Search) {
        let change = [x0, y0, x1, y1];
        let [wx0, wy0, wx1, wy1] = self.grow(change, i32::from(CLEAR_CAP) + 1);
        // Cells that may open or shut: areas read only whether a cell is
        // open (clearance of at least CLEARANCE), so a cell further than
        // that from the change can't have moved them, whatever its
        // clearance did.
        let near = self.grow(change, i32::from(CLEARANCE));
        let was_open: Vec<bool> = self.cells_in(near).map(|c| self.open_cell(c)).collect();
        let wide = [wx0, wy0, wx1, wy1];
        let room_was: Vec<u8> = self.cells_in(wide).map(|c| self.link_room(c)).collect();
        self.measure_clearance_in(wx0, wy0, wx1, wy1);
        if !self.renumber_near(near, &was_open) {
            self.number_areas();
        }
        // Sectors a cell that may have opened or shut lies in, and those
        // where a cell's room as links read it changed (links keep
        // GRAPH_BERTH from blocked ground where they can, so their costs
        // read clearance up to it, further out than cells open or shut).
        let [dx0, dy0, dx1, dy1] = near;
        let mut dirty = vec![false; (self.sectors * self.sectors) as usize];
        for sy in dy0 / SECTOR..=(dy1 - 1) / SECTOR {
            for sx in dx0 / SECTOR..=(dx1 - 1) / SECTOR {
                dirty[(sy * self.sectors + sx) as usize] = true;
            }
        }
        for (c, &was) in self.cells_in(wide).zip(&room_was) {
            if self.link_room(c) != was {
                dirty[self.sector_of(c) as usize] = true;
            }
        }
        let old = OldGraph::take(self);
        // Portals sit where clearance is greatest, so only edges near
        // cells whose clearance may have changed can move.
        let partner = self.place_portals(Some((&old, [wx0, wy0, wx1, wy1])));
        // A sector whose portals moved (a neighbour's edge changed) is
        // linked again too.
        for (s, d) in dirty.iter_mut().enumerate() {
            if !*d && !old.cells_of(s).eq(self.cells_of(s)) {
                *d = true;
            }
        }
        self.relink(&partner, &old, &dirty, search);
        self.node_room = self.node_room();
        search.fit(self);
    }

    /// The box `[x0, y0, x1, y1]` (cells, end exclusive) grown by `by` on
    /// every side, kept within the map.
    fn grow(&self, [x0, y0, x1, y1]: [i32; 4], by: i32) -> [i32; 4] {
        [
            (x0 - by).max(0),
            (y0 - by).max(0),
            (x1 + by).min(self.side),
            (y1 + by).min(self.side),
        ]
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

    /// The cells of the box `[x0, y0, x1, y1]` (end exclusive), row by row.
    /// Cell `c`'s clearance as links' costs read it (see [`Grid::weight`]),
    /// and whether it is open.
    fn link_room(&self, c: u32) -> u8 {
        self.clear[c as usize].min(GRAPH_BERTH)
    }

    fn cells_in(&self, [x0, y0, x1, y1]: [i32; 4]) -> impl Iterator<Item = u32> + '_ {
        (y0..y1).flat_map(move |y| (x0..x1).map(move |x| self.idx(x, y)))
    }

    /// Bring the areas up to date after only cells in `near` (a box) may
    /// have opened or shut, `was_open` saying which were open before, by
    /// looking only in a box [`SECTOR`] cells wider. The box's open cells
    /// are joined into pieces, as they stand now and as they stood before,
    /// and each piece now takes the area its cells had. Pieces that reach
    /// out of the box keep their area; so does the first of an area's
    /// pieces when none of them do. The rest (pockets cut off inside the
    /// box, and new ground) take new numbers. Returns false when that
    /// can't be told from the box alone: two areas joined, or a piece from
    /// before cut into two that both reach out (they may or may not meet
    /// again beyond). Then the whole map must be numbered again: cells in
    /// `near` that opened or shut have already lost their areas. Areas are only ever compared, so their numbers may
    /// differ from a fresh numbering's.
    pub(super) fn renumber_near(&mut self, near: [i32; 4], was_open: &[bool]) -> bool {
        let b @ [bx0, by0, bx1, by1] = self.grow(near, SECTOR);
        let (w, h) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
        let side = self.side;
        // The map's index of the box's `l`th cell.
        let cell = |l: u32| {
            let l = l as usize;
            ((by0 + (l / w) as i32) * side + bx0 + (l % w) as i32) as usize
        };
        // Which of the box's cells are open now, and were before (only
        // cells in `near` can differ).
        let now: Vec<bool> = self.cells_in(b).map(|c| self.open_cell(c)).collect();
        let mut before = now.clone();
        let [nx0, ny0, nx1, ny1] = near;
        let near_local = (ny0..ny1).flat_map(|y| (nx0..nx1).map(move |x| (x, y)));
        for ((x, y), &was) in near_local.zip(was_open) {
            let l = (y - by0) as usize * w + (x - bx0) as usize;
            before[l] = was;
            // Cells that shut have no area; cells that opened have none yet.
            if was != now[l] {
                self.area[(y * side + x) as usize] = 0;
            }
        }
        let (_, pieces, cells) = pieces_of(&now, w, h);
        let (piece_before, pieces_before, _) = pieces_of(&before, w, h);
        // Whether a cell is on a side of the box that isn't the map's edge,
        // and so may lead on beyond. Such cells never change.
        let edge = |l: u32| {
            let (x, y) = (bx0 + (l as usize % w) as i32, by0 + (l as usize / w) as i32);
            (x == bx0 && bx0 > 0)
                || (y == by0 && by0 > 0)
                || (x == bx1 - 1 && bx1 < side)
                || (y == by1 - 1 && by1 < side)
        };
        // Per piece now: whether it reaches out of the box; (area, piece)
        // for each piece's one old area; and which piece now holds each
        // piece from before's way out of the box.
        let mut reaches = vec![false; pieces.len()];
        let mut had: Vec<(u32, usize)> = Vec::new();
        let mut holder = vec![u32::MAX; pieces_before.len()];
        for (p, range) in pieces.iter().enumerate() {
            let mut area = 0;
            for &l in &cells[range.clone()] {
                match (area, self.area[cell(l)]) {
                    (_, 0) => {}
                    (0, a) => area = a,
                    (a, b) if a != b => return false, // two areas joined
                    _ => {}
                }
                if edge(l) {
                    reaches[p] = true;
                    let o = &mut holder[piece_before[l as usize] as usize];
                    if *o == u32::MAX {
                        *o = p as u32;
                    } else if *o != p as u32 {
                        return false; // cut in two, both reaching out
                    }
                }
            }
            if area != 0 {
                had.push((area, p));
            }
        }
        let mut number: Vec<u32> = vec![0; pieces.len()];
        had.sort_unstable();
        for run in had.chunk_by(|a, b| a.0 == b.0) {
            let mut kept = false;
            for &(a, p) in run.iter().filter(|&&(_, p)| reaches[p]) {
                number[p] = a;
                kept = true;
            }
            if !kept {
                number[run[0].1] = run[0].0;
            }
        }
        for n in number.iter_mut().filter(|n| **n == 0) {
            self.areas += 1;
            *n = self.areas;
        }
        for (range, &n) in pieces.iter().zip(&number) {
            for &l in &cells[range.clone()] {
                self.area[cell(l)] = n;
            }
        }
        true
    }

    /// Number the connected areas of open ground, so a search never starts
    /// for a goal that can't be reached: each area by the first cell met in
    /// it, row by row, as a flood fill from each new cell would, but in two
    /// passes over the map. A step on the diagonal
    /// needs both squares beside it open, so areas join along rows and
    /// columns only: the first pass gives each open cell the label of the
    /// cell behind or above it (a new one if both are shut), noting labels
    /// that meet as one; the second numbers each area by the first cell
    /// met in it.
    pub(super) fn number_areas(&mut self) {
        let side = self.side as usize;
        // Each label's parent, never above the label: a root is its own.
        let mut parent: Vec<u32> = vec![0];
        let root = |parent: &mut [u32], mut l: u32| {
            while parent[l as usize] != l {
                let up = parent[parent[l as usize] as usize];
                parent[l as usize] = up;
                l = up;
            }
            l
        };
        for y in 0..side {
            let row = y * side;
            for x in 0..side {
                let i = row + x;
                if self.clear[i] < CLEARANCE {
                    self.area[i] = 0;
                    continue;
                }
                let behind = if x > 0 { self.area[i - 1] } else { 0 };
                let above = if y > 0 { self.area[i - side] } else { 0 };
                self.area[i] = match (behind, above) {
                    (0, 0) => {
                        parent.push(parent.len() as u32);
                        parent.len() as u32 - 1
                    }
                    (l, 0) | (0, l) => l,
                    (a, b) if a == b => a,
                    (a, b) => {
                        // Labels the root, so the cells after it in the run
                        // meet the same label above them and skip this.
                        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                        parent[ra.max(rb) as usize] = ra.min(rb);
                        ra.min(rb)
                    }
                };
            }
        }
        // Flatten (a parent is never above its label), then number roots
        // in the order the map first meets them.
        for l in 1..parent.len() {
            parent[l] = parent[parent[l] as usize];
        }
        let mut number = vec![0u32; parent.len()];
        let mut next = 0;
        for a in &mut self.area {
            if *a == 0 {
                continue;
            }
            let r = parent[*a as usize] as usize;
            if number[r] == 0 {
                next += 1;
                number[r] = next;
            }
            *a = number[r];
        }
        self.areas = next;
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
        let costs: Vec<Vec<u32>> = dirty
            .iter()
            .enumerate()
            .map(|(s, &d)| {
                if d {
                    self.sector_costs(search, s)
                } else {
                    Vec::new()
                }
            })
            .collect();
        let (mut first, mut to, mut cost) = (
            Vec::with_capacity(nodes + 1),
            Vec::with_capacity(old.edge_to.len()),
            Vec::with_capacity(old.edge_to.len()),
        );
        first.push(0);
        for (u, &p) in partner.iter().enumerate() {
            let cell = self.node_cell[u];
            self.link_across(u, p, &mut to, &mut cost);
            let s = self.sector_of(cell) as usize;
            if dirty[s] {
                self.link_within(u, place[u], &costs[s], &mut to, &mut cost);
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

/// The pieces the open cells of a `w` × `h` box (`open`, row by row) form,
/// joined along rows and columns as areas are: each cell's piece
/// (`u32::MAX` if shut), each piece's range of the list of cells, and that
/// list, piece by piece, each piece from the first cell met in it.
fn pieces_of(open: &[bool], w: usize, h: usize) -> (Vec<u32>, Vec<Range<usize>>, Vec<u32>) {
    let mut piece = vec![u32::MAX; w * h];
    let mut ranges = Vec::new();
    let mut cells: Vec<u32> = Vec::with_capacity(w * h);
    for first in 0..w * h {
        if piece[first] != u32::MAX || !open[first] {
            continue;
        }
        let p = ranges.len() as u32;
        let start = cells.len();
        piece[first] = p;
        cells.push(first as u32);
        let mut head = start;
        while head < cells.len() {
            let l = cells[head] as usize;
            head += 1;
            let (x, y) = (l % w, l / w);
            let next = [
                (x + 1 < w).then(|| l + 1),
                (x > 0).then(|| l - 1),
                (y + 1 < h).then(|| l + w),
                (y > 0).then(|| l - w),
            ];
            for m in next.into_iter().flatten() {
                if open[m] && piece[m] == u32::MAX {
                    piece[m] = p;
                    cells.push(m as u32);
                }
            }
        }
        ranges.push(start..cells.len());
    }
    (piece, ranges, cells)
}

/// Each node's place in its sector's list, from a graph's sector lists.
pub(super) fn places(sector_first: &[u32], sector_nodes: &[u32], nodes: usize) -> Vec<u32> {
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
pub(super) struct OldGraph {
    pub(super) node_cell: Vec<u32>,
    pub(super) pair_first: Vec<u32>,
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
            pair_first: core::mem::take(&mut grid.pair_first),
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
    use crate::nav::{CELLS_PER_TILE, DIRS};
    use crate::scenario::ribs_map;
    use gc_fixed::Pcg32;

    /// The same cells in the same areas, whatever the areas' numbers.
    fn same_areas(a: &Grid, b: &Grid) {
        let mut to_b = alloc::collections::BTreeMap::new();
        let mut to_a = alloc::collections::BTreeMap::new();
        for (i, (&x, &y)) in a.area.iter().zip(&b.area).enumerate() {
            assert_eq!(x == 0, y == 0, "open differs at cell {i}");
            if x != 0 {
                assert_eq!(*to_b.entry(x).or_insert(y), y, "areas differ at cell {i}");
                assert_eq!(*to_a.entry(y).or_insert(x), x, "areas differ at cell {i}");
            }
        }
    }

    /// Every part of the grid that routes read.
    fn same(a: &Grid, b: &Grid) {
        assert!(a.cost == b.cost, "costs differ");
        for (i, (x, y)) in a.clear.iter().zip(&b.clear).enumerate() {
            assert_eq!(x, y, "clearance differs at cell {i}");
        }
        same_areas(a, b);
        assert_eq!(a.node_cell, b.node_cell, "portals differ");
        assert_eq!(a.pair_first, b.pair_first, "portals differ");
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

    #[test]
    fn an_open_door_is_ground_as_if_nothing_stood_there() {
        let tiles: i32 = 96;
        let side = tiles * CELLS_PER_TILE;
        let ground = ribs_map(tiles as u32);
        let mut g = Grid::build(&ground, side);
        let mut search = Search::for_grid(&g);
        // A wall right across, a gate in it, and a house the gate's door
        // knows nothing of.
        let (wall, gate, house) = ([0, 40, 50, 1], [50, 40, 3, 1], [70, 60, 3, 3]);
        for b in [wall, gate, house] {
            assert!(g.set_built(b, true, &mut search).is_some());
        }
        let mut walled = Grid::build(&g.cost, side);
        walled.ground.clone_from(&g.ground);
        walled.built.clone_from(&g.built);
        same(&g, &walled);
        g.change_all(&[gate], Change::Opened(true), &mut search);
        let mut free = Grid::build(&ground, side);
        let mut s2 = Search::for_grid(&free);
        for b in [wall, house] {
            assert!(free.set_built(b, true, &mut s2).is_some());
        }
        same(&g, &free);
        // Shut again, the wall it was; the gate still stands either way.
        g.change_all(&[gate], Change::Opened(false), &mut search);
        same(&g, &walled);
        assert!(g.built[g.idx(101, 81) as usize] > 0);
    }

    #[test]
    fn a_straight_walk_is_checked_through_every_cell_it_crosses() {
        // Random blocked cells and random walks, many starting or ending on
        // cell edges: a walk passed as clear never touches a blocked cell
        // anywhere along it.
        let side = 24;
        let mut rng = Pcg32::new(5, 3);
        let mut costs = vec![1u8; (side * side) as usize];
        for c in &mut costs {
            if rng.below(6) == 0 {
                *c = BLOCKED;
            }
        }
        let g = Grid::build(&costs, side);
        // A position in cells, in halves or in finer steps, as tiles.
        let at = |rng: &mut Pcg32| {
            let raw = if rng.below(2) == 0 {
                rng.below(2 * side as u32) as i32 * (1 << 15)
            } else {
                rng.below((side as u32) << 16) as i32
            };
            Fx::from_raw(raw / CELLS_PER_TILE)
        };
        let mut passed = 0;
        for _ in 0..4000 {
            let (p, q) = (
                FxVec2::new(at(&mut rng), at(&mut rng)),
                FxVec2::new(at(&mut rng), at(&mut rng)),
            );
            if !g.segment_within(p, q, CLEARANCE, u8::MAX, &mut 0) {
                continue;
            }
            passed += 1;
            for k in 0..=512i64 {
                let along = |a: Fx, b: Fx| {
                    Fx::from_raw(
                        (i64::from(a.raw()) + (i64::from(b.raw()) - i64::from(a.raw())) * k / 512)
                            as i32,
                    )
                };
                let m = FxVec2::new(along(p.x, q.x), along(p.y, q.y));
                assert!(
                    g.cost[g.cell_at(m) as usize] != BLOCKED,
                    "{p:?} to {q:?} crosses blocked ground at {m:?}"
                );
            }
        }
        assert!(passed > 200, "{passed}");
        // The case that slipped through: a start on a cell's west edge,
        // heading west.
        let mut costs = vec![1u8; (side * side) as usize];
        costs[(side + 2) as usize] = BLOCKED; // cell (2, 1)
        let g = Grid::build(&costs, side);
        let tiles = |x: i32, y: i32| FxVec2::new(Fx::from_raw(x << 14), Fx::from_raw(y << 14));
        // Cells (4.0, 0.5) to (1.5, 3.5), in quarter tiles.
        assert!(!g.segment_within(tiles(8, 1), tiles(3, 7), CLEARANCE, u8::MAX, &mut 0));
    }

    #[test]
    fn a_building_near_a_sector_edge_reprices_the_sector_beyond() {
        // A building a few cells short of a sector edge narrows the room
        // in the next sector too, which its links' costs read, even where
        // that sector's portals stay put.
        let tiles: i32 = 64;
        let side = tiles * CELLS_PER_TILE;
        let ground = vec![1; (side * side) as usize];
        // Sector edges every 16 tiles: from right by x = 16 to 5 tiles short.
        for y in (0..28).step_by(3) {
            for x in 9..14 {
                let mut g = Grid::build(&ground, side);
                let mut search = Search::for_grid(&g);
                assert!(g.set_built([x, y, 2, 3], true, &mut search).is_some());
                let mut fresh = Grid::build(&g.cost, side);
                fresh.ground.clone_from(&g.ground);
                fresh.built.clone_from(&g.built);
                same(&g, &fresh);
            }
        }
    }

    #[test]
    fn buildings_put_up_and_pulled_down_together_leave_the_grid_built_afresh() {
        // Two bases' worth of buildings far apart, and a few strays: all
        // put up in one go, half pulled down in another, then the rest.
        let tiles: i32 = 192;
        let side = tiles * CELLS_PER_TILE;
        let ground = ribs_map(tiles as u32);
        let mut g = Grid::build(&ground, side);
        let mut search = Search::for_grid(&g);
        let mut rng = Pcg32::new(4, 7);
        let mut all: Vec<[i32; 4]> = Vec::new();
        for (bx, by) in [(20, 20), (150, 140), (90, 30)] {
            for _ in 0..12 {
                let (w, d) = (2 + rng.below(4) as i32, 2 + rng.below(4) as i32);
                all.push([bx + rng.below(30) as i32, by + rng.below(30) as i32, w, d]);
            }
        }
        let check = |g: &Grid| {
            let mut fresh = Grid::build(&g.cost, side);
            fresh.ground.clone_from(&g.ground);
            fresh.built.clone_from(&g.built);
            same(g, &fresh);
        };
        let groups = g.set_built_all(&all, true, &mut search);
        assert!(groups.len() >= 3 && groups.len() < all.len(), "{groups:?}");
        check(&g);
        let (half, rest) = all.split_at(all.len() / 2);
        g.set_built_all(half, false, &mut search);
        check(&g);
        g.set_built_all(rest, false, &mut search);
        same(&g, &Grid::build(&ground, side));
    }

    /// The areas numbered by a flood fill from each new open cell met,
    /// row by row: what [`Grid::number_areas`] must give.
    fn flood_areas(g: &Grid) -> Vec<u32> {
        let mut g = g.clone();
        g.area.fill(0);
        let mut queue: Vec<u32> = Vec::with_capacity(g.cost.len());
        let mut next = 0;
        for c in 0..g.cost.len() as u32 {
            if !g.open_cell(c) || g.area[c as usize] != 0 {
                continue;
            }
            next += 1;
            g.area[c as usize] = next;
            queue.clear();
            queue.push(c);
            let mut head = 0;
            while head < queue.len() {
                let (x, y) = g.xy(queue[head]);
                head += 1;
                for (k, &(dx, dy)) in DIRS.iter().enumerate() {
                    if !g.open(x + dx, y + dy)
                        || (k >= 4 && !(g.open(x + dx, y) && g.open(x, y + dy)))
                    {
                        continue;
                    }
                    let m = g.idx(x + dx, y + dy);
                    if g.area[m as usize] == 0 {
                        g.area[m as usize] = next;
                        queue.push(m);
                    }
                }
            }
        }
        g.area
    }

    #[test]
    fn areas_are_numbered_as_the_flood_fill_numbers_them() {
        // Speckled ground: many small areas, and labels that meet late
        // (a U whose arms join at the bottom).
        let side = 96;
        let mut rng = Pcg32::new(5, 2);
        for blocked in [20, 40, 55] {
            let cost: Vec<u8> = (0..side * side)
                .map(|_| if rng.below(100) < blocked { BLOCKED } else { 1 })
                .collect();
            let g = Grid::build(&cost, side);
            assert!(g.area == flood_areas(&g), "{blocked}% blocked");
        }
    }

    /// Put up (or pull down) a building as [`Grid::set_built`] does, but
    /// bring only clearance and areas up to date, and say whether the areas
    /// could be told from near the building alone.
    fn renumbered_near(g: &mut Grid, tiles: [i32; 4], on: bool) -> bool {
        let [x0, y0, x1, y1] = g.cells(tiles).unwrap();
        let near = g.grow([x0, y0, x1, y1], i32::from(CLEARANCE));
        let was_open: Vec<bool> = g.cells_in(near).map(|c| g.open_cell(c)).collect();
        for c in g.cells_in([x0, y0, x1, y1]).collect::<Vec<_>>() {
            let c = c as usize;
            g.built[c] = if on { g.built[c] + 1 } else { g.built[c] - 1 };
            g.cost[c] = if g.built[c] > 0 { BLOCKED } else { g.ground[c] };
        }
        g.measure_clearance_in(0, 0, g.side, g.side);
        let near_only = g.renumber_near(near, &was_open);
        if !near_only {
            g.number_areas();
        }
        let fresh = Grid::build(&g.cost, g.side);
        same_areas(g, &fresh);
        near_only
    }

    #[test]
    fn areas_are_renumbered_near_a_building_when_they_can_be() {
        let tiles = 64;
        let open = vec![1u8; (tiles * CELLS_PER_TILE * tiles * CELLS_PER_TILE) as usize];
        let mut g = Grid::build(&open, tiles * CELLS_PER_TILE);
        // A building in the open: nothing cut off.
        assert!(renumbered_near(&mut g, [30, 30, 2, 2], true));
        // A ring of walls round a pocket in the middle of the map: the last
        // wall cuts the pocket off, inside the box looked in.
        let ring = [[20, 20, 8, 1], [20, 27, 8, 1], [20, 21, 1, 6]];
        for wall in ring {
            assert!(renumbered_near(&mut g, wall, true));
        }
        assert!(
            renumbered_near(&mut g, [27, 21, 1, 6], true),
            "pocket cut off"
        );
        // Pulling a wall down joins two areas: the whole map is numbered.
        assert!(!renumbered_near(&mut g, [27, 21, 1, 6], false));
        // A wall right across the map: the last piece cuts it in two, and
        // both sides reach out of the box.
        for x in (0..tiles - 8).step_by(8) {
            assert!(renumbered_near(&mut g, [x, 50, 8, 1], true));
        }
        // Beside the wall, an area is two pieces in the box, joined round
        // the wall's end out of it; a building there cuts neither.
        assert!(renumbered_near(&mut g, [8, 47, 2, 2], true));
        assert!(renumbered_near(&mut g, [8, 47, 2, 2], false));
        assert!(
            !renumbered_near(&mut g, [tiles - 8, 50, 8, 1], true),
            "map cut in two"
        );
        assert!(
            !renumbered_near(&mut g, [0, 50, 8, 1], false),
            "halves joined"
        );
    }

    #[test]
    fn areas_stay_right_as_walls_come_and_go() {
        // Walls a tile thick put up and pulled down at random in one corner
        // of the ridged map, cutting pockets off and joining them again,
        // the areas checked against a fresh build after each.
        let tiles = 64;
        let mut g = Grid::build(&ribs_map(tiles as u32), tiles * CELLS_PER_TILE);
        let mut rng = Pcg32::new(9, 1);
        let mut up: Vec<[i32; 4]> = Vec::new();
        let (mut near_only, mut whole_map) = (0, 0);
        for _ in 0..200 {
            let done = if !up.is_empty() && rng.below(3) == 0 {
                let k = rng.below(up.len() as u32) as usize;
                renumbered_near(&mut g, up.swap_remove(k), false)
            } else {
                let long = 2 + rng.below(8) as i32;
                let [w, d] = if rng.below(2) == 0 {
                    [long, 1]
                } else {
                    [1, long]
                };
                let b = [rng.below(24) as i32, rng.below(24) as i32, w, d];
                up.push(b);
                renumbered_near(&mut g, b, true)
            };
            if done {
                near_only += 1;
            } else {
                whole_map += 1;
            }
        }
        assert!(
            near_only > 0 && whole_map > 0,
            "{near_only} near, {whole_map} not"
        );
    }
}
