//! Ways through (docs/27-living-land.md, stage 2): ground an army can
//! stand on but no start can reach (a plateau ringed by cliffs, a valley
//! shut in by crags, a start cut off by a river) gets a way in. The
//! cheapest crossing from the reachable land is found, and a ramp is cut
//! along it: a gentle slope a few tiles wide, part cut into the high side
//! and part built out on the low side, and a ford where it meets deep
//! water. Small pockets, where a ramp would be more scar than land, are
//! broken into rock as before.
//!
//! Every change is made to a tile and its turned copy alike, so the land
//! stays its own turned copy.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::field::{
    Grid, ONE, Seeds, distance, div, fbm, hypot, lerp, mul, n4, patches, smoothstep,
};
use super::{Habitat, Raster, TOP, WATER_LINE};
use crate::def::Land;

/// A pocket smaller than this (in tiles) is broken into rock instead of
/// getting a ramp.
const POCKET: usize = 40;
/// How much a ramp climbs per tile along it: under half the rise the nav
/// rules call steep, so it is easy going.
const RAMP_RISE: i32 = ONE * 3 / 10;
/// Half the ramp's full-height width, and how far beyond that it fades
/// into the land beside it, in fixed-point tiles.
const HALF_WIDTH: i32 = 3 * ONE;
const EDGE: i32 = 2 * ONE;
/// Water a ramp crosses is raised to a ford this far under the line.
const FORD: i32 = ONE / 5;
/// Water deeper than this costs more to cross when choosing where to cut.
const DEEP: i32 = WATER_LINE - ONE * 35 / 100;
/// Rounds of ramps and passes before what is still cut off is broken.
const ROUNDS: usize = 4;
/// Under measured fairness the walks are what keeps the starts equal, so
/// passes are cut sooner and for longer.
const MEASURED_ROUNDS: usize = 8;
/// The longest the walk between two starts may be, in tenths of the
/// straight line between them, before a pass is cut.
const MAX_STRETCH: i64 = 17;
const MEASURED_STRETCH: i64 = 13;

/// Give every cut-off piece of standable ground a way in (or break it),
/// and cut a pass wherever two starts' way round is much longer than the
/// straight line.
pub(super) fn open_ways(r: &mut Raster, seeds: &mut Seeds) {
    let n = r.height.n;
    let rough = fbm(n, seeds.next(), ONE * 5 / 2, 2);
    let mut standable = super::standable(r);
    let rounds = if r.mirrored { ROUNDS } else { MEASURED_ROUNDS };
    for _ in 0..rounds {
        let opened = open_pockets(r, &standable, &rough, false);
        if opened {
            standable = super::standable(r);
        }
        let cut = passes(r, &standable);
        if cut {
            standable = super::standable(r);
        }
        if !opened && !cut {
            return;
        }
    }
    // Whatever is still cut off is broken.
    open_pockets(r, &standable, &rough, true);
}

/// A ramp into each pocket of standable ground no start reaches (or
/// break it: when it is small, or `only_break`). Returns whether there
/// was any.
fn open_pockets(r: &mut Raster, standable: &[u8], rough: &Grid, only_break: bool) -> bool {
    let n = r.height.n;
    let last = n * n - 1;
    let starts = start_tiles(r);
    let (live, pockets) = pieces(standable, n, starts[0]);
    // With no ground under the first start there is nothing to measure
    // reachable from; leave the land be rather than break all of it.
    if pockets.is_empty() || !live[starts[0]] {
        return false;
    }
    let mut broken = vec![false; n * n];
    let mut cross = None;
    for p in &pockets {
        let holds_start = p.iter().any(|t| starts.contains(t));
        if only_break {
            // A start's own ground is never broken: better cut off than
            // gone.
            if !holds_start {
                mark(&mut broken, p, &live, r.mirrored);
            }
            continue;
        }
        // A pocket and its turned copy are handled together, by
        // whichever holds the lower tile (unless the turned copy is the
        // reachable land itself).
        let lo = *p.iter().min().expect("a pocket has tiles");
        let hi = *p.iter().max().expect("a pocket has tiles");
        if r.mirrored && last - hi < lo && !live[last - hi] {
            continue;
        }
        if p.len() < POCKET && !holds_start {
            mark(&mut broken, p, &live, r.mirrored);
            continue;
        }
        let (dist, parent) = cross.get_or_insert_with(|| crossings(r, &live));
        let a = *p
            .iter()
            .min_by_key(|&&t| (dist[t], t))
            .expect("a pocket has tiles");
        if dist[a] == u32::MAX {
            if !holds_start {
                mark(&mut broken, p, &live, r.mirrored);
            }
            continue;
        }
        // Back along the cheapest crossing to the reachable land.
        let mut b = a;
        while !live[b] {
            b = parent[b] as usize;
        }
        ramp(r, a, b);
    }
    if broken.iter().any(|&d| d) {
        break_ground(r, &broken, rough);
    }
    true
}

/// The start tiles as indices, in player order (as `plateaus` gives them).
fn start_tiles(r: &Raster) -> Vec<usize> {
    let n = r.height.n;
    r.starts
        .iter()
        .map(|&(x, y)| y as usize * n + x as usize)
        .collect()
}

/// Where the walk between two starts is more than [`MAX_STRETCH`] tenths
/// of the straight line, cut the cheapest way through what is in between:
/// a ramp over each stretch of cliff, river or thicket on it. Returns
/// whether any was cut.
fn passes(r: &mut Raster, standable: &[u8]) -> bool {
    let n = r.height.n;
    let last = n * n - 1;
    let starts = start_tiles(r);
    // Each start's turned copy: the start nearest where it turns to (the
    // starts sit on whole tiles, so they can be a tile out). Every pair of
    // starts, or its turned copy, has one of 0, 1 and 4 in it, so walks
    // from those three are enough.
    let twin: Vec<usize> = if !r.mirrored {
        (0..starts.len()).collect()
    } else {
        starts
            .iter()
            .map(|&s| {
                let (x, y) = ((last - s) % n, (last - s) / n);
                (0..starts.len())
                    .min_by_key(|&k| (starts[k] % n).abs_diff(x) + (starts[k] / n).abs_diff(y))
                    .expect("there are starts")
            })
            .collect()
    };
    // (On land that isn't its own turned copy, walks from all six.)
    let from: &[usize] = if r.mirrored {
        debug_assert_eq!(twin, [3, 2, 1, 0, 5, 4]);
        &[0, 1, 4]
    } else {
        // (Pairs are walked from their lower start, so never from 5.)
        &[0, 1, 2, 3, 4]
    };
    let walks: Vec<Vec<u32>> = from
        .iter()
        .map(|&k| walk(standable, n, starts[k], &starts))
        .collect();
    // Only the worst-stretched pair gets a pass each round: one pass often
    // mends several pairs, and the next round measures again.
    let mut worst: Option<(u64, usize, usize)> = None;
    for i in 0..starts.len() {
        for j in i + 1..starts.len() {
            let (ti, tj) = (twin[i].min(twin[j]), twin[i].max(twin[j]));
            if (ti, tj) < (i, j) {
                continue;
            }
            // The pair as a walk from one of `from`, turned if need be.
            let (w, to) = [(i, j), (j, i), (twin[i], twin[j]), (twin[j], twin[i])]
                .into_iter()
                .find_map(|(k, to)| Some((from.iter().position(|&f| f == k)?, to)))
                .expect("every pair of starts, or its turned copy, has one in `from`");
            let walked = walks[w][starts[to]];
            let (si, sj) = (starts[i], starts[j]);
            let (dx, dy) = (
                (si % n) as i32 - (sj % n) as i32,
                (si / n) as i32 - (sj / n) as i32,
            );
            let straight = u64::from((hypot(dx * ONE, dy * ONE) >> 16).max(1) as u32);
            // A start with no ground under it can't anchor a pass.
            let limit = if r.mirrored {
                MAX_STRETCH
            } else {
                MEASURED_STRETCH
            };
            if u64::from(walked) <= straight * limit as u64
                || standable[si] == 0
                || standable[sj] == 0
            {
                continue;
            }
            let stretch = u64::from(walked) * 1024 / straight;
            if worst.is_none_or(|(w, _, _)| stretch > w) {
                worst = Some((stretch, si, sj));
            }
        }
    }
    let Some((_, si, sj)) = worst else {
        return false;
    };
    let path = cut_path(r, standable, si, sj);
    // A ramp over each stretch of the way that isn't standable (both ends
    // of the path are).
    let mut k = 0;
    while k < path.len() {
        if standable[path[k]] != 0 {
            k += 1;
            continue;
        }
        let before = path[k - 1];
        while standable[path[k]] == 0 {
            k += 1;
        }
        ramp(r, path[k], before);
    }
    true
}

/// The walk from `from` over standable ground, in tenths of a tile on
/// open ground (ten a straight step, fourteen a diagonal one, times the
/// ground's cost, no cutting corners, as `gc_mapcheck` walks); `u32::MAX`
/// where it can't go. Only the walks to `targets` are sure to be final:
/// it stops once they all are.
fn walk(standable: &[u8], n: usize, from: usize, targets: &[usize]) -> Vec<u32> {
    let mut dist = vec![u32::MAX; n * n];
    let mut heap = BinaryHeap::new();
    dist[from] = 0;
    heap.push(Reverse((0u32, from as u32)));
    let ni = n as i64;
    let mut left = targets.len();
    while let Some(Reverse((d, i))) = heap.pop() {
        let i = i as usize;
        if d > dist[i] {
            continue;
        }
        if targets.contains(&i) {
            left -= 1;
            if left == 0 {
                break;
            }
        }
        let (x, y) = ((i % n) as i64, (i / n) as i64);
        let ok = |x: i64, y: i64| {
            (0..ni).contains(&x) && (0..ni).contains(&y) && standable[(y * ni + x) as usize] != 0
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) == (0, 0) || !ok(x + dx, y + dy) {
                    continue;
                }
                if dx != 0 && dy != 0 && !(ok(x + dx, y) && ok(x, y + dy)) {
                    continue;
                }
                let j = ((y + dy) * ni + x + dx) as usize;
                let base = if dx != 0 && dy != 0 { 14 } else { 10 };
                let nd = d + base * u32::from(standable[j]) / 10;
                if nd < dist[j] {
                    dist[j] = nd;
                    heap.push(Reverse((nd, j as u32)));
                }
            }
        }
    }
    dist
}

/// The cheapest way four ways over the grid from the `sources` (each at
/// cost nothing) to every tile, or until `stop` is reached; `step(i, j)`
/// is the price of the step from `i` onto `j`, `None` where there is no
/// stepping. The cost to each tile and the tile it came from.
fn cheapest(
    n: usize,
    sources: impl IntoIterator<Item = usize>,
    stop: Option<usize>,
    step: impl Fn(usize, usize) -> Option<u32>,
) -> (Vec<u32>, Vec<u32>) {
    let mut dist = vec![u32::MAX; n * n];
    let mut parent = vec![u32::MAX; n * n];
    let mut heap = BinaryHeap::new();
    for s in sources {
        dist[s] = 0;
        heap.push(Reverse((0u32, s as u32)));
    }
    while let Some(Reverse((d, i))) = heap.pop() {
        let i = i as usize;
        if Some(i) == stop {
            break;
        }
        if d > dist[i] {
            continue;
        }
        for j in n4(i, n) {
            let Some(cost) = step(i, j) else {
                continue;
            };
            let nd = d + cost;
            if nd < dist[j] {
                dist[j] = nd;
                parent[j] = i as u32;
                heap.push(Reverse((nd, j as u32)));
            }
        }
    }
    (dist, parent)
}

/// The price of crossing rough ground from `i` to `j`: four a tenth of a
/// tile of height climbed or dropped, and three open steps more into deep
/// water.
fn rough_step(h: &[i32], i: usize, j: usize) -> u32 {
    let climb = (i64::from((h[j] - h[i]).abs()) * 40 / i64::from(ONE)) as u32;
    climb + if h[j] < DEEP { 30 } else { 0 }
}

/// The cheapest way from `from` to `to` when ground that isn't standable
/// can be crossed at a price: three open steps a step, plus its
/// [`rough_step`]. The tiles along it, from `from`.
fn cut_path(r: &Raster, standable: &[u8], from: usize, to: usize) -> Vec<usize> {
    let h = &r.height.v;
    let (_, parent) = cheapest(r.height.n, [from], Some(to), |i, j| {
        Some(match standable[j] {
            0 => 30 + rough_step(h, i, j),
            cost => u32::from(cost),
        })
    });
    let mut path = vec![to];
    let mut i = to;
    while i != from && parent[i] != u32::MAX {
        i = parent[i] as usize;
        path.push(i);
    }
    path.reverse();
    path
}

/// The reachable ground (the piece the start tile `first` stands on) and
/// every other piece of standable ground four tiles or more, in the order
/// their first tiles come row by row.
fn pieces(standable: &[u8], n: usize, first: usize) -> (Vec<bool>, Vec<Vec<usize>>) {
    let mask: Vec<bool> = standable.iter().map(|&c| c != 0).collect();
    let mut live = vec![false; n * n];
    let mut out = Vec::new();
    for piece in patches(&mask, n) {
        if mask[first] && piece.contains(&first) {
            for &t in &piece {
                live[t] = true;
            }
        } else if piece.len() >= 4 {
            out.push(piece);
        }
    }
    (live, out)
}

/// The cheapest crossing from the reachable land to every tile off it: a
/// step costs ten plus its [`rough_step`]. Returns the cost and the tile
/// each came from (for tiles off the reachable land).
fn crossings(r: &Raster, live: &[bool]) -> (Vec<u32>, Vec<u32>) {
    let n = r.height.n;
    let h = &r.height.v;
    // Only the edge of the reachable land can lead off it.
    let edge = (0..n * n).filter(|&i| live[i] && n4(i, n).any(|j| !live[j]));
    cheapest(n, edge, None, |i, j| {
        (!live[j]).then(|| 10 + rough_step(h, i, j))
    })
}

/// Mark a pocket and its turned copy to be broken (never the reachable
/// land, when the turned copy is part of it).
fn mark(broken: &mut [bool], pocket: &[usize], live: &[bool], mirrored: bool) {
    let last = broken.len() - 1;
    for &t in pocket {
        if !mirrored {
            broken[t] = true;
        } else if !live[last - t] {
            broken[t] = true;
            broken[last - t] = true;
        }
    }
}

/// Cut a ramp from tile `b` on the reachable land to tile `a` in a
/// pocket, and the same ramp on the turned copy: a straight slope through
/// both, long enough to climb no more than [`RAMP_RISE`] a tile, from the
/// land's own height at one end to its height at the other.
fn ramp(r: &mut Raster, a: usize, b: usize) {
    let n = r.height.n;
    let last = n * n - 1;
    let at = |t: usize| ((t % n) as i32 * ONE, (t / n) as i32 * ONE);
    let ((ax, ay), (bx, by)) = (at(a), at(b));
    let (dx, dy) = (ax - bx, ay - by);
    let gap = hypot(dx, dy);
    if gap == 0 {
        return;
    }
    let (ux, uy) = (div(dx, gap), div(dy, gap));
    let climb = (r.height.v[a] - r.height.v[b]).abs();
    let len = (gap + 4 * ONE).max(div(climb, RAMP_RISE) + 4 * ONE);
    let (mx, my) = ((ax + bx) / 2, (ay + by) / 2);
    let (sx, sy) = (mx - mul(ux, len / 2), my - mul(uy, len / 2));
    let (ex, ey) = (sx + mul(ux, len), sy + mul(uy, len));
    let mut hs = r.height.sample(i64::from(sx), i64::from(sy));
    let mut he = r.height.sample(i64::from(ex), i64::from(ey));
    // On a pocket narrower than the ramp is long its far end lands past
    // the pocket; then the ramp runs between the two tiles' own heights.
    if (he - hs).abs() < climb {
        (hs, he) = (r.height.v[b], r.height.v[a]);
    }
    let reach = HALF_WIDTH + EDGE;
    let lo = |a: i32, b: i32| ((a.min(b) - reach) >> 16).max(0);
    let hi = |a: i32, b: i32| ((a.max(b) + reach) >> 16).min(n as i32 - 1);
    // Worked out from the land as it was, then laid weakest first: where
    // the ramp meets its own turned copy (near the middle) the stronger
    // hand wins on both, and nothing is blended twice.
    let mut cut = Vec::new();
    for y in lo(sy, ey)..=hi(sy, ey) {
        for x in lo(sx, ex)..=hi(sx, ex) {
            let (rx, ry) = (x * ONE - sx, y * ONE - sy);
            let t = mul(rx, ux) + mul(ry, uy);
            if t < 0 || t > len {
                continue;
            }
            let side = (mul(rx, uy) - mul(ry, ux)).abs();
            let w = mul(
                smoothstep(reach, HALF_WIDTH, side),
                mul(smoothstep(0, 2 * ONE, t), smoothstep(len, len - 2 * ONE, t)),
            );
            if w == 0 {
                continue;
            }
            let along = hs + mul(he - hs, div(t, len));
            let level = along.clamp(WATER_LINE - FORD, TOP);
            let i = (y as usize) * n + x as usize;
            cut.push((w, i, lerp(r.height.v[i], level, w)));
        }
    }
    cut.sort_by_key(|&(w, ..)| w);
    for (w, i, h) in cut {
        let twin = r.mirrored.then_some(last - i);
        for k in std::iter::once(i).chain(twin) {
            r.height.v[k] = h;
            if w > ONE / 2 {
                r.thicket[k] = false;
                if r.habitat[k] == Habitat::BareRock as u8 {
                    r.habitat[k] = Habitat::Scree as u8;
                }
            }
        }
    }
}

/// Break the marked ground into rock (and the ground just round it), the
/// same bumps on a tile and its turned copy.
fn break_ground(r: &mut Raster, broken: &[bool], rough: &Grid) {
    let n = r.height.n;
    let near = distance(broken, n, 3);
    let last = n * n - 1;
    for (i, &is_broken) in broken.iter().enumerate() {
        if near.v[i] > ONE * 3 / 2 {
            continue;
        }
        // Rough bumps, and on the ground itself every other tile up or
        // down too, so steep that no one stands there (a tile and its
        // turned copy are alike: x + y keeps its evenness).
        let mut bump = mul(
            ONE * 18 / 10,
            rough.v[if r.mirrored { i.min(last - i) } else { i }] - ONE / 2,
        );
        if is_broken {
            bump += if (i % n + i / n).is_multiple_of(2) {
                ONE * 7 / 10
            } else {
                -ONE * 7 / 10
            };
        }
        r.height.v[i] = (r.height.v[i] + bump).clamp(0, TOP);
        r.cover[i] = gc_sim::Cover::Open as u8;
        r.thicket[i] = false;
        r.habitat[i] = Habitat::BareRock as u8;
        r.land[i] = Land::Highland as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Fairness, Grown, grow};
    use super::*;
    use crate::wilds::Wildness;

    /// After the ways are cut, every piece of standable ground a regiment
    /// could use is reachable from the first start, and no two starts'
    /// walk is far longer than the straight line between them.
    #[test]
    fn every_start_and_plateau_has_a_way_in() {
        for (wildness, fairness) in Wildness::ALL
            .into_iter()
            .flat_map(|w| Fairness::ALL.map(|f| (w, f)))
        {
            for seed in 1..=2 {
                let n = 256;
                let r = grow(
                    n as u32,
                    Grown {
                        seed,
                        wildness,
                        fairness,
                    },
                );
                let standable = super::super::standable(&r);
                let starts = start_tiles(&r);
                let (live, pockets) = pieces(&standable, n, starts[0]);
                assert!(
                    pockets.is_empty(),
                    "{wildness:?} {fairness:?} seed {seed}: cut off {:?}",
                    pockets.iter().map(Vec::len).collect::<Vec<_>>()
                );
                assert!(
                    starts.iter().all(|&s| live[s]),
                    "{wildness:?} {fairness:?} seed {seed}"
                );
                let from = walk(&standable, n, starts[0], &starts);
                for &s in &starts[1..] {
                    let (dx, dy) = (
                        (s % n) as i64 - (starts[0] % n) as i64,
                        (s / n) as i64 - (starts[0] / n) as i64,
                    );
                    let straight = ((dx * dx + dy * dy) as f64).sqrt();
                    let stretch = f64::from(from[s]) / 10.0 / straight;
                    assert!(
                        stretch < 1.8,
                        "{wildness:?} {fairness:?} seed {seed}: stretch {stretch:.2}"
                    );
                }
            }
        }
    }

    /// A ramp climbs gently from one end to the other and leaves the land
    /// its own turned copy.
    #[test]
    fn a_ramp_is_gentle_and_turned_alike() {
        let n = 64;
        // A plateau four tiles high in the middle of a small flat map.
        let mut r = Raster {
            height: Grid::from_fn(n, |x, y| {
                let inside = |v: usize| (n / 4..3 * n / 4).contains(&v);
                if inside(x) && inside(y) {
                    7 * ONE
                } else {
                    3 * ONE
                }
            }),
            cover: vec![0; n * n],
            thicket: vec![false; n * n],
            land: vec![0; n * n],
            habitat: vec![0; n * n],
            starts: crate::generate::plateaus(n as u32),
            mirrored: true,
        };
        let mid = 24 * n;
        ramp(&mut r, mid + n / 4 + 2, mid + n / 4 - 3);
        let last = n * n - 1;
        for i in 0..n * n {
            assert_eq!(r.height.v[i], r.height.v[last - i]);
        }
        // Along the ramp's middle line no step climbs more than a little
        // over the ramp's rise.
        let row = &r.height.v[mid..mid + n / 2];
        for w in row.windows(2) {
            assert!((w[1] - w[0]).abs() <= RAMP_RISE * 3 / 2, "{row:?}");
        }
    }
}
