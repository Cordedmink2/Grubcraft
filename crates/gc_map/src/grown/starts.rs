//! Measured fairness (docs/27-living-land.md, stage 3): where the starts
//! stand on land that is not its own turned copy. Each start keeps to a
//! window round its plateau (so north is still north and the middles are
//! still the middles), and within those windows a search on a coarse grid
//! moves them, one at a time, to where the walks to the enemy are as equal
//! as the land allows, for every way the lobby hands the starts out.
//!
//! It runs twice. First on the land's shape before the water and the
//! woods ([`place`]), so the clearings, the open ground round the starts
//! and the rivers all grow round the starts it chose; then on the finished
//! land as the nav rules see it ([`refine`]), a few tiles at most, to
//! even out what the rivers, woods and passes changed. `gc_mapcheck`
//! measures the finished map.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::field::{Grid, ONE};
use super::scaled;
use crate::def::Pos;
use crate::generate::{START_PLATEAUS, plateaus};

/// Ground rising faster than this between two tiles is too steep to march
/// up for the search (a little under the nav rules' steep).
const STEEP: i32 = ONE * 6 / 10;
/// A coarse cell this full of steep tiles (in sixteenths) can't be
/// crossed.
const SHUT: usize = 9;
/// Sweeps over the six starts at most.
const SWEEPS: usize = 3;

/// The ways the lobby pairs starts off (see `gc_match::battle_starts`):
/// north row 0, 1 and 4, south row 2, 3 and 5; corners for four players
/// or fewer, columns (0 and 2, 1 and 3, 4 and 5) for two-against-two-
/// against-two.
const NORTH: [bool; START_PLATEAUS] = [true, true, false, false, true, false];
const COLUMN: [usize; START_PLATEAUS] = [0, 1, 0, 1, 2, 2];

/// The coarse grid the search walks: `c` tiles a cell, `m` cells a side.
struct Coarse {
    c: usize,
    m: usize,
    /// Each cell's height (at its middle tile).
    height: Vec<i32>,
    /// Whether each cell can be crossed, and how rough it is (steep
    /// sixteenths).
    open: Vec<bool>,
    rough: Vec<u32>,
}

impl Coarse {
    /// The grid of `c`-tile cells over `h`, with the tiles in `shut`
    /// counted as ground that can't be crossed.
    fn new(h: &Grid, c: usize, steep: &[bool]) -> Coarse {
        let n = h.n;
        let m = n / c;
        let mut height = Vec::with_capacity(m * m);
        let mut open = Vec::with_capacity(m * m);
        let mut rough = Vec::with_capacity(m * m);
        for cy in 0..m {
            for cx in 0..m {
                let (x0, y0) = (cx * c, cy * c);
                height.push(h.v[(y0 + c / 2) * n + x0 + c / 2]);
                let count = (y0..y0 + c)
                    .flat_map(|y| (x0..x0 + c).map(move |x| y * n + x))
                    .filter(|&i| steep[i])
                    .count();
                let sixteenths = count * 16 / (c * c);
                let edge = cx == 0 || cy == 0 || cx + 1 == m || cy + 1 == m;
                open.push(!edge && sixteenths < SHUT);
                rough.push(sixteenths as u32);
            }
        }
        Coarse {
            c,
            m,
            height,
            open,
            rough,
        }
    }

    /// The cell a tile is in.
    fn cell(&self, at: Pos) -> usize {
        let clamp = |v: i32| (v.max(0) as usize / self.c).min(self.m - 1);
        clamp(at.1) * self.m + clamp(at.0)
    }

    /// A cell's middle tile.
    fn tile(&self, cell: usize) -> Pos {
        let (cx, cy) = (cell % self.m, cell / self.m);
        (
            (cx * self.c + self.c / 2) as i32,
            (cy * self.c + self.c / 2) as i32,
        )
    }

    /// The walk from `from` to each of `to`, eight ways over open cells
    /// (ten a straight step, fourteen a diagonal, more over rough ground
    /// and for every tile of height climbed or dropped); `u32::MAX` for
    /// one it can't reach.
    fn walks(&self, from: usize, to: &[usize]) -> Vec<u32> {
        let m = self.m as i64;
        let mut dist = vec![u32::MAX; self.m * self.m];
        let mut heap = BinaryHeap::new();
        dist[from] = 0;
        heap.push(Reverse((0u32, from as u32)));
        let mut left = to.iter().filter(|&&t| t != from).count();
        while let Some(Reverse((d, i))) = heap.pop() {
            let i = i as usize;
            if d > dist[i] {
                continue;
            }
            if i != from && to.contains(&i) {
                left -= 1;
                if left == 0 {
                    break;
                }
            }
            let (x, y) = ((i % self.m) as i64, (i / self.m) as i64);
            for (dx, dy) in [
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let (px, py) = (x + dx, y + dy);
                if !(0..m).contains(&px) || !(0..m).contains(&py) {
                    continue;
                }
                let j = (py * m + px) as usize;
                if !self.open[j] {
                    continue;
                }
                let climb = (self.height[j] - self.height[i]).abs();
                // Too steep to go straight up, cell to cell.
                if climb > STEEP * self.c as i32 {
                    continue;
                }
                let base: u32 = if dx != 0 && dy != 0 { 14 } else { 10 };
                let cost = base * (16 + self.rough[j]) / 16
                    + (i64::from(climb) * 20 / i64::from(ONE)) as u32;
                let nd = d + cost;
                if nd < dist[j] {
                    dist[j] = nd;
                    heap.push(Reverse((nd, j as u32)));
                }
            }
        }
        to.iter()
            .map(|&t| if t == from { 0 } else { dist[t] })
            .collect()
    }
}

/// How unequal a set of walks is, in thousandths: (most - least) / mean.
/// Any walk that can't be made counts as very unequal.
fn spread(values: impl Iterator<Item = u32>) -> u32 {
    let v: Vec<u32> = values.collect();
    if v.contains(&u32::MAX) {
        return 100_000;
    }
    let (lo, hi) = (
        v.iter().copied().min().unwrap_or(0),
        v.iter().copied().max().unwrap_or(0),
    );
    let mean = (v.iter().map(|&d| u64::from(d)).sum::<u64>() / v.len().max(1) as u64).max(1);
    (u64::from(hi - lo) * 1000 / mean) as u32
}

/// The worst spread, in thousandths, over every way the lobby hands the
/// starts out: each start's walk to its nearest enemy with the corners
/// only and with all six, north against south, everyone for themselves,
/// and column against column.
pub(super) fn unfairness(walk: &[[u32; START_PLATEAUS]; START_PLATEAUS]) -> u32 {
    let nearest = |k: usize, among: &[usize], foe: &dyn Fn(usize, usize) -> bool| {
        among
            .iter()
            .filter(|&&j| j != k && foe(k, j))
            .map(|&j| walk[k][j])
            .min()
            .unwrap_or(u32::MAX)
    };
    let corners = [0, 1, 2, 3];
    let all = [0, 1, 2, 3, 4, 5];
    let rows = |a: usize, b: usize| NORTH[a] != NORTH[b];
    let anyone = |_: usize, _: usize| true;
    let columns = |a: usize, b: usize| COLUMN[a] != COLUMN[b];
    let worst = |among: &[usize], foe: &dyn Fn(usize, usize) -> bool| {
        spread(among.iter().map(|&k| nearest(k, among, foe)))
    };
    [
        worst(&corners, &rows),
        worst(&all, &rows),
        worst(&corners, &anyone),
        worst(&all, &anyone),
        worst(&all, &columns),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

/// Where the six starts stand on `h`, in lobby order (as [`plateaus`]).
pub(super) fn place(h: &Grid) -> [Pos; START_PLATEAUS] {
    let n = h.n;
    let steep: Vec<bool> = (0..n * n)
        .map(|i| {
            let (x, y) = (i % n, i / n);
            let d = |j: usize| (h.v[j] - h.v[i]).abs() > STEEP;
            (x + 1 < n && d(i + 1)) || (y + 1 < n && d(i + n))
        })
        .collect();
    let coarse = Coarse::new(h, (n / 96).max(4), &steep);
    let slots = plateaus(n as u32);
    search(&coarse, slots, scaled(n, 28), 2, &|_| true)
}

/// The starts moved a few tiles on the finished land (`standable` as
/// `super::standable` gives it), where the walks to the enemy come out
/// more equal, onto ground where `fits` says a clearing can go.
pub(super) fn refine(
    h: &Grid,
    standable: &[u8],
    starts: [Pos; START_PLATEAUS],
    fits: &dyn Fn(Pos) -> bool,
) -> [Pos; START_PLATEAUS] {
    let n = h.n;
    let shut: Vec<bool> = standable.iter().map(|&c| c == 0).collect();
    let coarse = Coarse::new(h, (n / 128).max(3), &shut);
    search(&coarse, starts, scaled(n, 12), 1, &|cell| {
        fits(coarse.tile(cell))
    })
}

/// The search: each start, one at a time, moved to the cell within
/// `reach` tiles of where it is that makes the walks most equal, trying
/// every `stride`th cell and then the cells round the best; then again,
/// while any moved. Only cells `ok` takes are tried.
fn search(
    coarse: &Coarse,
    from: [Pos; START_PLATEAUS],
    reach: i32,
    stride: usize,
    ok: &dyn Fn(usize) -> bool,
) -> [Pos; START_PLATEAUS] {
    let reach = (reach as usize / coarse.c).max(1) as i64;
    let home: Vec<usize> = from.iter().map(|&s| coarse.cell(s)).collect();
    let mut at = home.clone();
    let mut walk = [[0u32; START_PLATEAUS]; START_PLATEAUS];
    let fill = |walk: &mut [[u32; START_PLATEAUS]; START_PLATEAUS], k: usize, row: &[u32]| {
        for j in 0..START_PLATEAUS {
            if j != k {
                walk[k][j] = row[j];
                walk[j][k] = row[j];
            }
        }
    };
    for k in 0..START_PLATEAUS {
        let row = coarse.walks(at[k], &at);
        fill(&mut walk, k, &row);
    }
    // A start pays a little for each cell it wanders and for rough ground,
    // so of two equally fair places the nearer, smoother one wins.
    let price = |cell: usize, k: usize| {
        let m = coarse.m as i64;
        let (dx, dy) = (
            (cell as i64 % m - home[k] as i64 % m).abs(),
            (cell as i64 / m - home[k] as i64 / m).abs(),
        );
        (dx.max(dy) as u32) * 2 + coarse.rough[cell] * 4
    };
    let mut score = unfairness(&walk) + (0..START_PLATEAUS).map(|k| price(at[k], k)).sum::<u32>();
    for _ in 0..SWEEPS {
        let mut moved = false;
        for k in 0..START_PLATEAUS {
            let m = coarse.m as i64;
            let (hx, hy) = (home[k] as i64 % m, home[k] as i64 / m);
            let rest: u32 = (0..START_PLATEAUS)
                .filter(|&j| j != k)
                .map(|j| price(at[j], j))
                .sum();
            let mut best: Option<(u32, usize, Vec<u32>)> = None;
            // Every other cell first, then the ones round the best of
            // those.
            let try_cell = |cell: usize, best: &mut Option<(u32, usize, Vec<u32>)>| {
                if !coarse.open[cell]
                    || !ok(cell)
                    || at.iter().enumerate().any(|(j, &a)| j != k && a == cell)
                {
                    return;
                }
                let mut targets = at.clone();
                targets[k] = cell;
                let row = coarse.walks(cell, &targets);
                let mut trial = walk;
                fill(&mut trial, k, &row);
                let s = unfairness(&trial) + rest + price(cell, k);
                if best.as_ref().is_none_or(|(b, _, _)| s < *b) {
                    *best = Some((s, cell, row));
                }
            };
            for dy in (-reach..=reach).step_by(stride) {
                for dx in (-reach..=reach).step_by(stride) {
                    let (x, y) = (hx + dx, hy + dy);
                    if (0..m).contains(&x) && (0..m).contains(&y) {
                        try_cell((y * m + x) as usize, &mut best);
                    }
                }
            }
            if let Some((_, c, _)) = best.as_ref().filter(|_| stride > 1) {
                let c = *c;
                let (cx, cy) = (c as i64 % m, c as i64 / m);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (x, y) = (cx + dx, cy + dy);
                        let near_home = (x - hx).abs() <= reach && (y - hy).abs() <= reach;
                        if (dx, dy) != (0, 0)
                            && near_home
                            && (0..m).contains(&x)
                            && (0..m).contains(&y)
                        {
                            try_cell((y * m + x) as usize, &mut best);
                        }
                    }
                }
            }
            if let Some((s, cell, row)) = best
                && s < score
            {
                score = s;
                at[k] = cell;
                fill(&mut walk, k, &row);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // A start that never moved keeps its own tile.
    let mut out = from;
    for k in 0..START_PLATEAUS {
        if at[k] != home[k] {
            out[k] = coarse.tile(at[k]);
        }
    }
    out
}
