//! Water: drainage, rivers, lakes and valleys (docs/27, "How a map grows",
//! step 4).
//!
//! Rain is let fall on the whole land and every drop finds its way down to
//! the map's edge:
//!
//! 1. **Flood.** A priority flood fills every basin that cannot drain, so
//!    each tile has a way downhill. The way round each start's clearing is
//!    found on a copy where the clearing stands well above the land, so
//!    the water goes round it and never into it.
//! 2. **Flow.** Each tile hands its water to its steepest downhill
//!    neighbour (eight directions); adding it all up says how many tiles
//!    drain through every tile.
//! 3. **Rivers.** Where enough gathers a river starts. It is traced down
//!    the drainage from its source, pushed sideways by a wandering wave
//!    (bigger on flat land, longer for bigger rivers), joined onto the
//!    river it flows into, rounded off, and laid on the map as wide as its
//!    flow.
//! 4. **Lakes.** Basins the flood had to fill deeper than half a tile
//!    (tidied up, and big enough to matter) become lakes.
//! 5. **Valleys.** The ground eases down to a gentle bank beside every
//!    river and lake, over a reach that varies from place to place; the
//!    channels and lake floors are cut below the water line, deeper in the
//!    middle.
//! 6. **Fords.** Every long river gets shallows now and then, where it is
//!    narrowest, so armies can cross.
//!
//! Heights are fixed-point tiles. All of it is whole-number maths in a
//! fixed order, so the same land makes the same water on every machine.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use gc_fixed::Angle;

use super::field::{
    Grid, ONE, Seeds, distance, fbm, hypot, lerp, mul, patches, smoothstep, sqrt, warp,
};
use super::{Dials, WATER_LINE};

/// What the water layer leaves for the biomes.
pub(super) struct Water {
    /// How many tiles drain through each tile (1 for a tile nothing drains into).
    pub(super) acc: Vec<u32>,
    /// River channel tiles (the fords are still river tiles, just shallow).
    pub(super) river: Vec<bool>,
    /// Lake tiles.
    pub(super) lake: Vec<bool>,
    /// The middle tile of each ford (shallows across a river), for sites
    /// and trails to use.
    #[allow(dead_code)]
    pub(super) fords: Vec<(i32, i32)>,
}

/// How far (tiles) round a clearing the drainage keeps off it.
const AVOID: i32 = 6;
/// How high (tiles) the clearings are raised on the drainage copy.
const MOUND: i32 = 10;
/// Drained tiles of the thinnest river, as a width: a river is
/// `1.4 + 2 * sqrt(flow / threshold)` tiles wide, at most [`MAX_WIDTH`].
const MIN_WIDTH: i32 = ONE * 14 / 10;
const MAX_WIDTH: i32 = 9 * ONE;
/// A river whose stub joins another before this many tiles is dropped.
const MIN_RIVER: usize = 45;
/// The smallest lake, in tiles.
const MIN_LAKE: usize = 300;
/// The biggest a lake may be, as a fraction (1 over this) of the map: a
/// bigger basin keeps only its deepest part.
const MAX_LAKE_DIV: usize = 120;
/// About how many tiles of river lie between fords.
const FORD_SPACING: i32 = 140;
/// A river at least this wide somewhere (fixed point) has fords a third
/// closer together.
const WIDE_RIVER: i32 = 6 * ONE;
/// The eight neighbours, as the prototype's order (ties go to the first).
const N8: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// A river's smoothed line: points in fixed-point tiles and the width at
/// each.
struct Line {
    pts: Vec<(i32, i32)>,
    width: Vec<i32>,
    /// True if it runs out to the edge or the middle rows (rather than
    /// joining another river).
    outlet: bool,
}

/// Cut rivers, lakes and their valleys into `h`, leaving `keep` alone.
pub(super) fn water(h: &mut Grid, seeds: &mut Seeds, dials: &Dials, keep: &[bool]) -> Water {
    let n = h.n;
    let nn = n * n;
    // Round the clearings the drainage goes (they stand as mounds).
    let near_keep = distance(keep, n, AVOID + 2);
    let avoid: Vec<bool> = near_keep.v.iter().map(|&d| d <= AVOID * ONE).collect();
    let mut mounded = h.v.clone();
    for (m, &a) in mounded.iter_mut().zip(&avoid) {
        if a {
            *m += MOUND * ONE;
        }
    }

    // No rain falls on the clearings, or all of it would gather in a
    // moat round each.
    let dry_land: Vec<bool> = avoid.iter().map(|&a| !a).collect();
    let filled = priority_flood(&mounded, n);
    let (acc, rec) = flow(&filled.height, &filled.order, n, &dry_land);
    let thresh = ((nn as i64 * i64::from(dials.river_per_mille)) / 1000).max(4) as u32;

    let phase = fbm(n, seeds.next(), n as i32 * ONE / 6, 3);
    let meander = fbm(n, seeds.next(), n as i32 * ONE / 4, 3);
    let lines = river_lines(h, &filled.height, &acc, &rec, thresh, &phase, &meander);
    let mut river = rasterize(&lines, n);

    // Lakes: basins the flood filled deeper than half a tile.
    let depth: Vec<i32> = (0..nn)
        .map(|i| {
            if avoid[i] {
                0
            } else {
                filled.height[i] - mounded[i]
            }
        })
        .collect();
    let mut lake: Vec<bool> = depth.iter().map(|&d| d > ONE / 2).collect();
    trim_lakes(&mut lake, &depth, n, nn / MAX_LAKE_DIV);
    for _ in 0..2 {
        lake = erode_cross(&lake, n);
    }
    for _ in 0..2 {
        lake = dilate_cross(&lake, n);
    }
    keep_big(&mut lake, n, MIN_LAKE);
    for i in 0..nn {
        if keep[i] {
            river[i] = false;
            lake[i] = false;
        }
    }

    // Rivers that reach the middle rows end in a pond (their turned copies
    // do too), so none stops dead.
    let mut pond = vec![false; nn];
    for line in lines.iter().filter(|l| l.outlet) {
        let (Some(&(ex, ey)), Some(&w)) = (line.pts.last(), line.width.last()) else {
            continue;
        };
        if (ey >> 16) as usize != n / 2 - 1 {
            continue;
        }
        let r = i64::from(w) + 4 * i64::from(ONE);
        let reach = ((r * 3 / 2) >> 16) + 1;
        let (cx, cy) = (i64::from(ex >> 16), i64::from(ey >> 16));
        for y in (cy - reach).max(0)..(cy + reach + 1).min(n as i64) {
            for x in (cx - reach).max(0)..(cx + reach + 1).min(n as i64) {
                let d = i64::from(hypot(
                    (x * i64::from(ONE) - i64::from(ex)) as i32,
                    (y * i64::from(ONE) - i64::from(ey)) as i32,
                ));
                // A lobed outline, not a disc: the reach wobbles with the
                // direction, differently for each pond.
                let (dx, dy) = ((x - cx) as i32, (y - cy) as i32);
                let turn = gc_fixed::Angle::of_vec(gc_fixed::FxVec2::new(
                    gc_fixed::Fx::from_int(dx),
                    gc_fixed::Fx::from_int(dy),
                ))
                .0;
                let phase = (ex ^ ey.rotate_left(7)) as u16;
                let wave = |k: u16, p: u16, amp: i32| {
                    mul(
                        amp,
                        gc_fixed::Angle(turn.wrapping_mul(k).wrapping_add(p))
                            .sin()
                            .raw(),
                    )
                };
                let lobes = ONE + wave(2, phase, ONE / 4) + wave(3, phase.rotate_left(5), ONE / 7);
                if d <= (r * i64::from(lobes)) >> 16 {
                    pond[(y * n as i64 + x) as usize] = true;
                }
            }
        }
    }
    // The south half is the north's turned copy, so only the north's water
    // is kept (ponds across the middle are kept from both).
    let last = nn - 1;
    for i in 0..nn / 2 {
        let j = last - i;
        river[j] = river[i];
        lake[j] = lake[i];
    }
    for i in 0..nn {
        if pond[i] {
            lake[i] = true;
            lake[last - i] = true;
        }
    }
    for i in 0..nn {
        if keep[i] {
            river[i] = false;
            lake[i] = false;
        }
    }
    let mut acc = acc;
    for i in 0..nn / 2 {
        acc[last - i] = acc[i];
    }

    let mut fords = carve(h, seeds, keep, &near_keep, &river, &lake, &lines);
    // Make the carved land (and the fords) the same turned copy.
    for i in 0..nn / 2 {
        let j = last - i;
        if !keep[i] && !keep[j] {
            h.v[j] = h.v[i];
        }
    }
    fords.retain(|&(_, y)| (y as usize) < n / 2);
    let turned: Vec<(i32, i32)> = fords
        .iter()
        .map(|&(x, y)| (n as i32 - 1 - x, n as i32 - 1 - y))
        .collect();
    fords.extend(turned);
    Water {
        acc,
        river,
        lake,
        fords,
    }
}

/// The land with its basins filled, and the order the flood reached the
/// tiles in (lowest first, so every tile comes after the ones it drains to).
struct Flooded {
    height: Vec<i32>,
    order: Vec<u32>,
}

/// Fill every basin that cannot drain to the map's edge: the flood starts
/// at the edge and climbs, always taking the lowest tile next. Ties go to
/// the lowest index, so it is the same on every machine.
fn priority_flood(h: &[i32], n: usize) -> Flooded {
    let nn = n * n;
    let mut height = h.to_vec();
    let mut seen = vec![false; nn];
    let mut order = Vec::with_capacity(nn);
    // Level tiles are taken in the order they were found (a wave spreading
    // from the outlet), not by index: indexes would sweep a filled flat
    // row by row and the drops' ways across it would zigzag.
    let mut seq = 0u32;
    let mut heap = BinaryHeap::with_capacity(nn / 4);
    for i in 0..n {
        for (x, y) in [(i, 0), (i, n - 1), (0, i), (n - 1, i)] {
            let k = y * n + x;
            if !seen[k] {
                seen[k] = true;
                heap.push(Reverse((h[k], seq, k as u32)));
                seq += 1;
            }
        }
    }
    // The middle rows are a second coast: the south half of the map is
    // made a turned copy of the north, so no river may flow across them.
    for x in 0..n {
        for y in [n / 2 - 1, n / 2] {
            let k = y * n + x;
            if !seen[k] {
                seen[k] = true;
                heap.push(Reverse((h[k], seq, k as u32)));
                seq += 1;
            }
        }
    }
    while let Some(Reverse((z, _, i))) = heap.pop() {
        order.push(i);
        let (x, y) = ((i as usize % n) as i32, (i as usize / n) as i32);
        for (ox, oy) in N8 {
            let (a, b) = (x + ox, y + oy);
            if a < 0 || b < 0 || a >= n as i32 || b >= n as i32 {
                continue;
            }
            let k = b as usize * n + a as usize;
            if !seen[k] {
                seen[k] = true;
                // A hair above the tile it drained from, so a filled
                // basin still slopes towards its outlet.
                let v = h[k].max(z + 1);
                height[k] = v;
                heap.push(Reverse((v, seq, k as u32)));
                seq += 1;
            }
        }
    }
    Flooded { height, order }
}

/// Flow accumulation: each tile's steepest way down (`-1` for tiles on the
/// edge, where water leaves the map), and how many tiles drain through
/// each tile.
fn flow(filled: &[i32], order: &[u32], n: usize, rain: &[bool]) -> (Vec<u32>, Vec<i32>) {
    let nn = n * n;
    let mut rec = vec![-1i32; nn];
    for y in 1..n - 1 {
        if y == n / 2 - 1 || y == n / 2 {
            continue; // the middle rows are outlets
        }
        for x in 1..n - 1 {
            let i = y * n + x;
            let z = filled[i];
            let mut best = 0i64;
            for (ox, oy) in N8 {
                let k = ((y as i32 + oy) as usize) * n + (x as i32 + ox) as usize;
                if filled[k] < z {
                    let run = if ox != 0 && oy != 0 { 1414 } else { 1000 };
                    let slope = i64::from(z - filled[k]) * 1_000_000 / run;
                    if slope > best {
                        best = slope;
                        rec[i] = k as i32;
                    }
                }
            }
        }
    }
    let mut acc: Vec<u32> = rain.iter().map(|&r| u32::from(r)).collect();
    for &i in order.iter().rev() {
        let r = rec[i as usize];
        if r >= 0 {
            acc[r as usize] += acc[i as usize];
        }
    }
    for a in &mut acc {
        *a = (*a).max(1);
    }
    (acc, rec)
}

/// Rivers as smooth, meandering lines down the drainage. Each starts at a
/// source (a river tile nothing river-sized drains into), highest first,
/// and runs down until it meets a river already traced.
fn river_lines(
    h: &Grid,
    filled: &[i32],
    acc: &[u32],
    rec: &[i32],
    thresh: u32,
    phase: &Grid,
    meander: &Grid,
) -> Vec<Line> {
    let n = h.n;
    let nn = n * n;
    let is_river = |i: usize| acc[i] > thresh;
    let mut has_donor = vec![false; nn];
    for (i, &r) in rec.iter().enumerate() {
        if r >= 0 && is_river(i) && is_river(r as usize) {
            has_donor[r as usize] = true;
        }
    }
    let mut heads: Vec<usize> = (0..nn).filter(|&i| is_river(i) && !has_donor[i]).collect();
    heads.sort_by_key(|&i| (Reverse(filled[i]), i));

    const NONE: (i32, i32) = (i32::MIN, 0);
    let mut shifted = vec![NONE; nn];
    let mut lines = Vec::new();
    for hd in heads {
        let mut path = Vec::new();
        let mut i = hd as i32;
        while i >= 0 && shifted[i as usize] == NONE {
            path.push(i as usize);
            i = rec[i as usize];
            if path.len() > 4 * n {
                break;
            }
        }
        let join = (i >= 0).then_some(i as usize);
        if path.len() < MIN_RIVER && join.is_some() {
            continue; // a short stub: not worth a river
        }
        let m = path.len();
        // Position and width at each tile of the path.
        let pos: Vec<(i32, i32)> = path
            .iter()
            .map(|&c| ((c % n) as i32 * ONE, (c / n) as i32 * ONE))
            .collect();
        let width: Vec<i32> = path
            .iter()
            .map(|&c| {
                let ratio =
                    ((i64::from(acc[c]) << 16) / i64::from(thresh)).min(i64::from(16 * ONE));
                (MIN_WIDTH + 2 * sqrt(ratio as i32)).min(MAX_WIDTH)
            })
            .collect();
        let mut xy = pos.clone();
        const SMOOTH: i64 = 12;
        if m > 4 {
            // Smoothed line, its sideways direction at each point.
            let sm: Vec<(i64, i64)> = (0..m as i64)
                .map(|k| {
                    let (mut sx, mut sy) = (0i64, 0i64);
                    // Wide enough to iron out the staircase a drop's way
                    // takes across filled, flat ground.
                    for d in -SMOOTH..=SMOOTH {
                        let p = pos[(k + d).clamp(0, m as i64 - 1) as usize];
                        sx += i64::from(p.0);
                        sy += i64::from(p.1);
                    }
                    let (sx, sy) = (sx / (2 * SMOOTH + 1), sy / (2 * SMOOTH + 1));
                    // Where it leaves the map the smoothing must not
                    // pull the river back from the edge.
                    if join.is_none() {
                        let t = ((m as i64 - 1 - k) * 65536 / SMOOTH).clamp(0, 65536);
                        let p = pos[k as usize];
                        (
                            i64::from(p.0) + (((sx - i64::from(p.0)) * t) >> 16),
                            i64::from(p.1) + (((sy - i64::from(p.1)) * t) >> 16),
                        )
                    } else {
                        (sx, sy)
                    }
                })
                .collect();
            let mut wave = 0i64; // the wandering wave's angle, 65536 to a turn
            for k in 0..m {
                let (a, b) = (k.saturating_sub(1), (k + 1).min(m - 1));
                let (tx, ty) = (
                    (sm[b].0 - sm[a].0) / (b - a) as i64,
                    (sm[b].1 - sm[a].1) / (b - a) as i64,
                );
                let len = i64::from(hypot(tx as i32, ty as i32));
                let (nx, ny) = if len > 0 {
                    (-(ty << 16) / len, (tx << 16) / len)
                } else {
                    (0, 0)
                };
                let c = path[k];
                let wd = i64::from(width[k]);
                let ph = i64::from(phase.v[c]);
                // How much the river wanders: more on flat, low land,
                // more where the noise says so.
                let flat = ((WATER_LINE + ONE * 55 / 10 - h.v[c]) / 4).clamp(ONE / 4, ONE);
                let wild = ONE * 4 / 10 + mul(ONE * 12 / 10, meander.v[c]);
                let amp = mul(
                    mul(ONE * 3 / 10 + mul(ONE * 11 / 10, wd as i32), flat),
                    wild,
                );
                let wav = mul(
                    ONE * 22 + 7 * wd as i32,
                    ONE * 7 / 10 + mul(ONE * 6 / 10, ph as i32),
                );
                // Never so tight for its length that the bends fold into
                // a zigzag: the wave's steepest is about one across to one along.
                let amp = amp.min(wav / 9);
                wave += (65536 * i64::from(ONE)) / i64::from(wav.max(ONE));
                let ang = wave + (ph / 8).max(0);
                let s1 = Angle(ang as u16).sin().raw();
                let s2 = Angle((ang * 23 / 10 + 17_731) as u16).sin().raw();
                let mut off = mul(amp, s1 + mul(ONE / 5, s2));
                let ramp_in = ((k as i64 * i64::from(ONE)) / 10).clamp(0, i64::from(ONE)) as i32;
                off = mul(off, ramp_in);
                // Come back onto the river it joins, or onto the edge it
                // leaves the map by.
                let ramp_out =
                    (((m - 1 - k) as i64 * i64::from(ONE)) / 10).clamp(0, i64::from(ONE)) as i32;
                off = mul(off, ramp_out);
                xy[k] = (
                    (sm[k].0 + ((nx * i64::from(off)) >> 16)) as i32,
                    (sm[k].1 + ((ny * i64::from(off)) >> 16)) as i32,
                );
            }
            xy[0] = pos[0];
            if join.is_none() {
                xy[m - 1] = pos[m - 1];
            }
        }
        for (k, &c) in path.iter().enumerate() {
            shifted[c] = xy[k];
        }
        let mut ws = width;
        if let Some(j) = join {
            xy.push(shifted[j]);
            ws.push(*ws.last().unwrap_or(&MIN_WIDTH));
        }
        let smooth = chaikin(&xy, 3);
        // Widths along the smoothed line, by arc position.
        let total = smooth.len();
        let last = ws.len() - 1;
        let width_at = |k: usize| -> i32 {
            if total < 2 || last == 0 {
                return ws[0];
            }
            let num = k * last;
            let (i0, frac) = (num / (total - 1), num % (total - 1));
            if i0 >= last {
                return ws[last];
            }
            let t = ((frac as i64) << 16) / (total as i64 - 1);
            lerp(ws[i0], ws[i0 + 1], t as i32)
        };
        // Keep a point every few: enough to lay the river down.
        let mut pts = Vec::new();
        let mut wid = Vec::new();
        for k in (0..total).step_by(4) {
            pts.push(smooth[k]);
            wid.push(width_at(k));
        }
        if !(total - 1).is_multiple_of(4) {
            pts.push(smooth[total - 1]);
            wid.push(width_at(total - 1));
        }
        lines.push(Line {
            pts,
            width: wid,
            outlet: join.is_none(),
        });
    }
    lines
}

/// Round off a line's corners: each round replaces every corner with two
/// points a quarter of the way in (the ends stay put).
fn chaikin(pts: &[(i32, i32)], rounds: usize) -> Vec<(i32, i32)> {
    let mut pts = pts.to_vec();
    for _ in 0..rounds {
        if pts.len() < 3 {
            return pts;
        }
        let mut out = Vec::with_capacity(pts.len() * 2);
        out.push(pts[0]);
        for (k, w) in pts.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let q = ((3 * a.0 + b.0) >> 2, (3 * a.1 + b.1) >> 2);
            let r = ((a.0 + 3 * b.0) >> 2, (a.1 + 3 * b.1) >> 2);
            if k > 0 {
                out.push(q);
            }
            if k + 2 < pts.len() {
                out.push(r);
            }
        }
        out.push(pts[pts.len() - 1]);
        pts = out;
    }
    pts
}

/// The tiles under the rivers: a disc as wide as the river at every point
/// of every line.
fn rasterize(lines: &[Line], n: usize) -> Vec<bool> {
    let mut out = vec![false; n * n];
    let ni = n as i64;
    for line in lines {
        for (&(px, py), &w) in line.pts.iter().zip(&line.width) {
            let r = i64::from(w) / 2 + i64::from(ONE) / 8;
            let r2 = r * r;
            let reach = ((r * 3 / 2) >> 16) + 1;
            let (cx, cy) = (i64::from(px) >> 16, i64::from(py) >> 16);
            for y in (cy - reach).max(0)..(cy + reach + 2).min(ni) {
                for x in (cx - reach).max(0)..(cx + reach + 2).min(ni) {
                    let (dx, dy) = (
                        x * i64::from(ONE) - i64::from(px),
                        y * i64::from(ONE) - i64::from(py),
                    );
                    if dx * dx + dy * dy <= r2 {
                        out[(y * ni + x) as usize] = true;
                    }
                }
            }
        }
    }
    out
}

/// Shrink a mask by a tile (a tile stays only if it and its four
/// neighbours are in it; outside the map counts as out).
fn erode_cross(m: &[bool], n: usize) -> Vec<bool> {
    let mut out = vec![false; n * n];
    for y in 1..n.saturating_sub(1) {
        for x in 1..n - 1 {
            let i = y * n + x;
            out[i] = m[i] && m[i - 1] && m[i + 1] && m[i - n] && m[i + n];
        }
    }
    out
}

/// Grow a mask by a tile (a tile is in if it or a neighbour was).
fn dilate_cross(m: &[bool], n: usize) -> Vec<bool> {
    let mut out = m.to_vec();
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            if m[i] {
                continue;
            }
            out[i] = (x > 0 && m[i - 1])
                || (x + 1 < n && m[i + 1])
                || (y > 0 && m[i - n])
                || (y + 1 < n && m[i + n]);
        }
    }
    out
}

/// Clear every connected patch of the mask no bigger than `min` tiles.
fn keep_big(m: &mut [bool], n: usize, min: usize) {
    for patch in patches(m, n) {
        if patch.len() <= min {
            for i in patch {
                m[i] = false;
            }
        }
    }
}

/// A lake bigger than `max` tiles keeps only its deepest `max` tiles (a
/// big flat basin would otherwise drown a whole region).
fn trim_lakes(m: &mut [bool], depth: &[i32], n: usize, max: usize) {
    for patch in patches(m, n) {
        if patch.len() <= max {
            continue;
        }
        let mut d: Vec<i32> = patch.iter().map(|&i| depth[i]).collect();
        d.sort_unstable_by(|a, b| b.cmp(a));
        let floor = d[max];
        for i in patch {
            if depth[i] <= floor {
                m[i] = false;
            }
        }
    }
}

/// Cut the valleys, channels and lake floors into `h`, then lay shallows
/// across the long rivers. Tiles in `keep` are never touched. Returns the
/// middle of each ford.
fn carve(
    h: &mut Grid,
    seeds: &mut Seeds,
    keep: &[bool],
    off_keep: &Grid,
    river: &[bool],
    lake: &[bool],
    lines: &[Line],
) -> Vec<(i32, i32)> {
    let n = h.n;
    let nn = n * n;
    let wet: Vec<bool> = (0..nn).map(|i| river[i] || lake[i]).collect();
    let dist = distance(&wet, n, 42);
    let dry: Vec<bool> = wet.iter().map(|&w| !w).collect();
    let inner = distance(&dry, n, 4);
    // How far the valley reaches varies from place to place: 8 to 42 tiles.
    let reach = {
        let f = warp(
            &fbm(n, seeds.next(), n as i32 * ONE / 6, 4),
            seeds,
            30 * n as i32 * ONE / 512,
            n as i32 * ONE / 5,
        );
        f.map(|v| 8 * ONE + mul(34 * ONE, v))
    };
    for i in 0..nn {
        if keep[i] {
            continue;
        }
        let d = dist.v[i];
        let z = h.v[i];
        let s = smoothstep(reach.v[i], 0, d);
        let valley = mul(mul(s, sqrt(s)), smoothstep(0, AVOID * ONE, off_keep.v[i]));
        if valley > 0 {
            // A gentle bank a little above the water, steeper right at
            // the shore so the bank never makes a cliff of the channel's
            // edge.
            let rise =
                (ONE / 2 + mul(d, ONE * 6 / 100)).min(ONE * 12 / 100 + mul(d, ONE * 30 / 100));
            let target = z.min(WATER_LINE + rise);
            h.v[i] = lerp(z, target, valley);
        }
        if wet[i] {
            let bed = WATER_LINE - ONE * 45 / 100 - mul(inner.v[i].min(4 * ONE), ONE * 2 / 10);
            h.v[i] = h.v[i].min(bed);
        }
    }
    fords(h, seeds, keep, lines)
}

/// Shallows where each long river is narrowest: the bed is raised to just
/// under the water line, in a disc as wide as the river, fading out into
/// the deep. Returns the middle of each.
fn fords(h: &mut Grid, seeds: &mut Seeds, keep: &[bool], lines: &[Line]) -> Vec<(i32, i32)> {
    let n = h.n as i32;
    let mut out = Vec::new();
    for line in lines {
        let m = line.pts.len();
        // Length of the river in whole tiles.
        let mut cum = Vec::with_capacity(m);
        let mut total = 0i64;
        cum.push(0i64);
        for w in line.pts.windows(2) {
            total += i64::from(hypot(w[1].0 - w[0].0, w[1].1 - w[0].1));
            cum.push(total);
        }
        let tiles = (total >> 16) as i32;
        let widest = line.width.iter().copied().max().unwrap_or(0);
        if tiles < 40 || widest < ONE * 35 / 10 {
            continue;
        }
        // Wide rivers are crossed more often: they are the ones that cut
        // the land in two.
        let spacing = if widest >= WIDE_RIVER {
            FORD_SPACING * 2 / 3
        } else {
            FORD_SPACING
        };
        let count = (tiles / spacing).max(1);
        for j in 0..count {
            // The middle of its stretch (a river is narrowest at its
            // source, which is no place for a ford).
            let a = total * i64::from(j) / i64::from(count);
            let b = total * i64::from(j + 1) / i64::from(count);
            let lo = (a + (b - a) / 5).max(3 * i64::from(ONE));
            let hi = (b - (b - a) / 5).min(total - 3 * i64::from(ONE));
            let mut best: Option<(i32, usize)> = None;
            for k in (0..m).step_by(3) {
                if cum[k] < lo || cum[k] >= hi {
                    continue;
                }
                let (x, y) = (line.pts[k].0 >> 16, line.pts[k].1 >> 16);
                // (only in the north half, clear of the middle: the south is its
                // turned copy)
                if x < 8 || y < 8 || x >= n - 8 || y >= n / 2 - 14 {
                    continue;
                }
                let score = line.width[k] + 2 * seeds.unit();
                if best.is_none_or(|(s, _)| score < s) {
                    best = Some((score, k));
                }
            }
            let Some((_, k)) = best else { continue };
            let (fx, fy) = line.pts[k];
            // Wide enough to cross the whole river, fading out beyond.
            let inner = (3 * ONE).max(line.width[k] / 2 + ONE);
            let outer = inner + 3 * ONE;
            let reach = (outer >> 16) + 1;
            let (cx, cy) = (fx >> 16, fy >> 16);
            for y in (cy - reach).max(0)..(cy + reach + 2).min(n) {
                for x in (cx - reach).max(0)..(cx + reach + 2).min(n) {
                    let i = (y * n + x) as usize;
                    if keep[i] {
                        continue;
                    }
                    let d = hypot(x * ONE - fx, y * ONE - fy);
                    let f = smoothstep(outer, inner, d);
                    if f > 0 {
                        let shallow = WATER_LINE - ONE / 5;
                        h.v[i] = h.v[i].max(lerp(h.v[i], shallow, f));
                    }
                }
            }
            out.push((cx, cy));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// Water deeper than this below the line can't be waded (the nav
    /// rules' ford depth).
    const DEEP: i32 = ONE * 35 / 100;
    use super::super::{Dials, erode, shape};
    use super::*;
    use crate::generate::plateaus;
    use crate::wilds::Wildness;

    /// Land and clearings as `grow` makes them just before the water.
    fn land(n: usize, seed: u64, wild: Wildness) -> (Grid, Vec<bool>, Seeds, Dials) {
        let dials = Dials::of(wild);
        let mut seeds = Seeds::new(seed);
        let starts = plateaus(n as u32);
        let near = Grid::from_fn(n, |x, y| {
            starts
                .iter()
                .map(|&(sx, sy)| hypot((x as i32 - sx) * ONE, (y as i32 - sy) * ONE))
                .min()
                .unwrap()
        });
        let s = |t: i32| (i64::from(t) * n as i64 / 512) as i32;
        let calm = near.map(|d| smoothstep(s(95) * ONE, s(55) * ONE, d));
        let (raw, parts) = shape::raw_land(n, &mut seeds, &dials, &calm);
        let benched = shape::terrace(&raw, &mut seeds, &dials);
        let mut h = shape::massifs(&benched, &parts, &mut seeds, &dials);
        erode::erode(&mut h, seeds.next(), 70_000 * n * n / (512 * 512));
        erode::thermal(&mut h, ONE * 18 / 10, 4);
        for &st in &starts {
            let shape = super::super::clearing_shape(&mut seeds);
            super::super::flatten_clearing(&mut h, st, shape);
        }
        let open = s(40);
        let keep = near.v.iter().map(|&d| d <= open * ONE).collect();
        (h, keep, seeds, dials)
    }

    fn share(v: &[bool]) -> usize {
        v.iter().filter(|&&b| b).count() * 1000 / v.len()
    }

    #[test]
    fn deterministic_and_keeps_clearings() {
        for (seed, wild) in [
            (3, Wildness::Wild),
            (9, Wildness::Plains),
            (21, Wildness::Mixed),
        ] {
            let n = 320;
            let (h0, keep, mut s1, dials) = land(n, seed, wild);
            let mut a = h0.clone();
            let wa = water(&mut a, &mut s1, &dials, &keep);
            let (_, _, mut s2, _) = land(n, seed, wild);
            let mut b = h0.clone();
            let wb = water(&mut b, &mut s2, &dials, &keep);
            assert_eq!(a, b);
            assert_eq!(wa.river, wb.river);
            assert_eq!(wa.lake, wb.lake);
            assert_eq!(wa.acc, wb.acc);
            for (i, &kept) in keep.iter().enumerate() {
                if kept {
                    assert_eq!(a.v[i], h0.v[i], "keep tile {i} changed");
                    assert!(!wa.river[i] && !wa.lake[i]);
                }
            }
            let shallow = (0..n * n)
                .filter(|&i| (wa.river[i] || wa.lake[i]) && a.v[i] > WATER_LINE - DEEP)
                .count();
            println!("shallow wet tiles {shallow}");
            let (r, l) = (share(&wa.river), share(&wa.lake));
            println!(
                "{wild:?} seed {seed} n {n}: river {r}/1000 lake {l}/1000 fords {}",
                wa.fords.len()
            );
            assert!(r < 130 && l < 60, "river {r} lake {l}");
        }
    }

    #[test]
    fn shares_at_512() {
        for (seed, wild) in [
            (3, Wildness::Wild),
            (5, Wildness::Mixed),
            (7, Wildness::Plains),
            (11, Wildness::Wild),
        ] {
            let (mut h, keep, mut seeds, dials) = land(512, seed, wild);
            let w = water(&mut h, &mut seeds, &dials, &keep);
            let (r, l) = (share(&w.river), share(&w.lake));
            println!(
                "{wild:?} seed {seed}: river {r}/1000 lake {l}/1000 fords {}",
                w.fords.len()
            );
            assert!(r < 100 && l < 60, "river {r} lake {l}");
        }
    }

    #[test]
    fn timing_at_768() {
        let (mut h, keep, mut seeds, dials) = land(768, 3, Wildness::Wild);
        let t = std::time::Instant::now();
        let w = water(&mut h, &mut seeds, &dials, &keep);
        println!(
            "water at 768: {:?}, river {}/1000, lake {}/1000, fords {}",
            t.elapsed(),
            share(&w.river),
            share(&w.lake),
            w.fords.len()
        );
    }
}
