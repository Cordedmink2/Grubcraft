//! The number fields grown land is made of: square grids of whole numbers,
//! one value per tile, and the few things done to them (noise, warping,
//! blurring, distances, percentiles). Fixed point throughout: [`ONE`] is
//! 1.0, so a height of `3 * ONE` is three tiles and a field "from 0 to 1"
//! runs from 0 to `ONE`.
//!
//! Everything is integer maths with a fixed order of work, so the same
//! seed makes the same fields on every machine.

use gc_fixed::{Fx, Pcg32, isqrt_u64};

/// Fixed point: 1.0, and one tile of height or distance.
pub(crate) const ONE: i32 = 1 << 16;

/// `a * b` in fixed point.
pub(crate) fn mul(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 16) as i32
}

/// `a / b` in fixed point (0 when `b` is 0).
pub(crate) fn div(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        ((i64::from(a) << 16) / i64::from(b)) as i32
    }
}

/// `a` to `b` by `t` (0 to [`ONE`]).
pub(crate) fn lerp(a: i32, b: i32, t: i32) -> i32 {
    a + ((i64::from(b - a) * i64::from(t)) >> 16) as i32
}

/// 0 at `e0`, [`ONE`] at `e1`, a smooth S between (either way round).
pub(crate) fn smoothstep(e0: i32, e1: i32, x: i32) -> i32 {
    if e0 == e1 {
        return if x < e0 { 0 } else { ONE };
    }
    let t = (i64::from(x - e0) * i64::from(ONE) / i64::from(e1 - e0)).clamp(0, i64::from(ONE));
    (t * t / i64::from(ONE) * (3 * i64::from(ONE) - 2 * t) / i64::from(ONE)) as i32
}

/// The square root of a fixed-point number, in fixed point.
pub(crate) fn sqrt(x: i32) -> i32 {
    Fx::from_raw(x).sqrt().raw()
}

/// The length of (dx, dy), in the same fixed point.
pub(crate) fn hypot(dx: i32, dy: i32) -> i32 {
    let (dx, dy) = (i64::from(dx), i64::from(dy));
    isqrt_u64((dx * dx + dy * dy) as u64) as i32
}

/// A square grid of fixed-point values, one per tile, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Grid {
    pub(crate) n: usize,
    pub(crate) v: Vec<i32>,
}

impl Grid {
    #[cfg(test)]
    pub(crate) fn new(n: usize, fill: i32) -> Grid {
        Grid {
            n,
            v: vec![fill; n * n],
        }
    }

    /// A grid with `f(x, y)` at each tile.
    pub(crate) fn from_fn(n: usize, mut f: impl FnMut(usize, usize) -> i32) -> Grid {
        let mut v = Vec::with_capacity(n * n);
        for y in 0..n {
            for x in 0..n {
                v.push(f(x, y));
            }
        }
        Grid { n, v }
    }

    /// The value at (x, y), clamped to the grid's edge.
    pub(crate) fn get(&self, x: i64, y: i64) -> i32 {
        let last = self.n as i64 - 1;
        self.v[(y.clamp(0, last) as usize) * self.n + x.clamp(0, last) as usize]
    }

    /// The value between tiles at a fixed-point position (tile centres at
    /// whole numbers), bilinear, clamped to the edge.
    pub(crate) fn sample(&self, x: i64, y: i64) -> i32 {
        let one = i64::from(ONE);
        let (ix, iy) = (x.div_euclid(one), y.div_euclid(one));
        let (fx, fy) = (x.rem_euclid(one), y.rem_euclid(one));
        let a = i64::from(self.get(ix, iy));
        let b = i64::from(self.get(ix + 1, iy));
        let c = i64::from(self.get(ix, iy + 1));
        let d = i64::from(self.get(ix + 1, iy + 1));
        let top = a * (one - fx) + b * fx;
        let bottom = c * (one - fx) + d * fx;
        ((top * (one - fy) + bottom * fy) >> 32) as i32
    }

    /// The grid drawn `k` times bigger, bilinear between its tile centres.
    pub(crate) fn upsample(&self, k: usize) -> Grid {
        let one = i64::from(ONE);
        let kk = k as i64;
        // The centre of fine tile x lies at (x + 1/2) / k - 1/2 coarse tiles.
        let at = |x: usize| ((2 * x as i64 + 1) * one / (2 * kk)) - one / 2;
        Grid::from_fn(self.n * k, |x, y| self.sample(at(x), at(y)))
    }

    /// The grid shrunk `k` times, each tile the mean of the `k` by `k`
    /// tiles it covers (the side must divide by `k`).
    pub(crate) fn downsample(&self, k: usize) -> Grid {
        let m = self.n / k;
        Grid::from_fn(m, |x, y| {
            let sum: i64 = (0..k)
                .flat_map(|dy| (0..k).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| i64::from(self.v[(y * k + dy) * self.n + x * k + dx]))
                .sum();
            (sum / (k * k) as i64) as i32
        })
    }

    /// Each value through `f`.
    pub(crate) fn map(&self, f: impl Fn(i32) -> i32) -> Grid {
        Grid {
            n: self.n,
            v: self.v.iter().map(|&v| f(v)).collect(),
        }
    }

    /// The change in height to the east and to the south at each tile
    /// (central differences, one-sided at the edge), in fixed point per
    /// tile.
    pub(crate) fn gradient(&self) -> (Grid, Grid) {
        let n = self.n as i64;
        let d = |a: i64, b: i64, span: i64| ((b - a) / span) as i32;
        let gx = Grid::from_fn(self.n, |x, y| {
            let (x, y) = (x as i64, y as i64);
            let (l, r) = ((x - 1).max(0), (x + 1).min(n - 1));
            d(
                i64::from(self.get(l, y)),
                i64::from(self.get(r, y)),
                (r - l).max(1),
            )
        });
        let gy = Grid::from_fn(self.n, |x, y| {
            let (x, y) = (x as i64, y as i64);
            let (u, b) = ((y - 1).max(0), (y + 1).min(n - 1));
            d(
                i64::from(self.get(x, u)),
                i64::from(self.get(x, b)),
                (b - u).max(1),
            )
        });
        (gx, gy)
    }

    /// The steepness at each tile: the length of the gradient.
    #[cfg(test)]
    pub(crate) fn slope(&self) -> Grid {
        let (gx, gy) = self.gradient();
        Grid {
            n: self.n,
            v: gx.v.iter().zip(&gy.v).map(|(&a, &b)| hypot(a, b)).collect(),
        }
    }

    /// The value below which `per_mille` thousandths of the grid lie,
    /// near enough: read from an even spread of at most 2^16 of its tiles.
    pub(crate) fn percentile(&self, per_mille: i32) -> i32 {
        let every = (self.v.len() >> 16).max(1) | 1;
        percentile(self.v.iter().copied().step_by(every).collect(), per_mille)
    }

    /// The grid stretched so its `lo` and `hi` per-mille values become 0
    /// and [`ONE`], clamped.
    pub(crate) fn stretch(&self, lo: i32, hi: i32) -> Grid {
        let (a, b) = (self.percentile(lo), self.percentile(hi));
        self.map(|v| div(v - a, (b - a).max(1)).clamp(0, ONE))
    }

    /// A box blur `r` tiles each way, `passes` times (three passes is
    /// close to a Gaussian of about `r` tiles). Edges are clamped.
    pub(crate) fn blur(&self, r: usize, passes: usize) -> Grid {
        if r == 0 {
            return self.clone();
        }
        let n = self.n;
        let mut cur = self.v.clone();
        let mut tmp = vec![0i32; n * n];
        let width = (2 * r + 1) as i64;
        let blur_line = |get: &dyn Fn(i64) -> i32, out: &mut dyn FnMut(usize, i32)| {
            let last = n as i64 - 1;
            let mut sum: i64 = (-(r as i64)..=r as i64)
                .map(|k| i64::from(get(k.clamp(0, last))))
                .sum();
            for i in 0..n as i64 {
                out(i as usize, (sum / width) as i32);
                let add = (i + r as i64 + 1).clamp(0, last);
                let drop = (i - r as i64).clamp(0, last);
                sum += i64::from(get(add)) - i64::from(get(drop));
            }
        };
        for _ in 0..passes {
            for y in 0..n {
                let row = &cur[y * n..(y + 1) * n];
                blur_line(&|x| row[x as usize], &mut |x, v| tmp[y * n + x] = v);
            }
            for x in 0..n {
                let src = &tmp;
                blur_line(&|y| src[y as usize * n + x], &mut |y, v| cur[y * n + x] = v);
            }
        }
        Grid { n, v: cur }
    }
}

/// The value below which `per_mille` thousandths of `values` lie.
pub(crate) fn percentile(mut values: Vec<i32>, per_mille: i32) -> i32 {
    if values.is_empty() {
        return 0;
    }
    let k = ((values.len() - 1) * per_mille.clamp(0, 1000) as usize) / 1000;
    *values.select_nth_unstable(k).1
}

/// A bool grid's distance field: for each tile, how far (fixed point,
/// tiles) it is to the nearest tile where `mask` is true, up to `cap`
/// tiles (and `cap` where there is none). A two-pass chamfer with knight
/// steps, within about 2% of the straight-line distance.
pub(crate) fn distance(mask: &[bool], n: usize, cap: i32) -> Grid {
    debug_assert!(cap <= 4096, "the cap is in whole tiles");
    // Steps of 5 (straight), 7 (diagonal) and 11 (knight's move) are 1,
    // 1.4 and 2.2 tiles.
    const STEPS: [(i64, i64, i32); 8] = [
        (-1, 0, 5),
        (0, -1, 5),
        (-1, -1, 7),
        (1, -1, 7),
        (-1, -2, 11),
        (1, -2, 11),
        (-2, -1, 11),
        (2, -1, 11),
    ];
    let far = cap.saturating_mul(5);
    let mut d: Vec<i32> = mask.iter().map(|&m| if m { 0 } else { far }).collect();
    let ni = n as i64;
    let relax = |d: &mut Vec<i32>, x: i64, y: i64, sign: i64| {
        let i = (y * ni + x) as usize;
        let mut best = d[i];
        for &(dx, dy, c) in &STEPS {
            let (px, py) = (x + sign * dx, y + sign * dy);
            if (0..ni).contains(&px) && (0..ni).contains(&py) {
                best = best.min(d[(py * ni + px) as usize] + c);
            }
        }
        d[i] = best;
    };
    for y in 0..ni {
        for x in 0..ni {
            relax(&mut d, x, y, 1);
        }
    }
    for y in (0..ni).rev() {
        for x in (0..ni).rev() {
            relax(&mut d, x, y, -1);
        }
    }
    Grid {
        n,
        v: d.iter().map(|&v| (v.min(far) * ONE) / 5).collect(),
    }
}

/// The four tiles beside tile `i` on an `n`-wide grid, in the order
/// left, right, up, down (searches rely on the order for their ties).
pub(crate) fn n4(i: usize, n: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % n, i / n);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < n).then(|| i + 1),
        (y > 0).then(|| i - n),
        (y + 1 < n).then(|| i + n),
    ]
    .into_iter()
    .flatten()
}

/// Every connected patch (four ways) of the mask, each as its tile
/// indices, in the order their first tiles come.
pub(crate) fn patches(m: &[bool], n: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; n * n];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for s in 0..n * n {
        if !m[s] || seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let mut patch = Vec::new();
        while let Some(i) = stack.pop() {
            patch.push(i);
            for k in n4(i, n) {
                if m[k] && !seen[k] {
                    seen[k] = true;
                    stack.push(k);
                }
            }
        }
        out.push(patch);
    }
    out
}

/// Where the seeds of a map's fields come from: one stream, drawn in a
/// fixed order.
pub(crate) struct Seeds(pub(crate) Pcg32);

impl Seeds {
    pub(crate) fn new(seed: u64) -> Seeds {
        Seeds(Pcg32::new(seed, 0x0067_726f_776e))
    }

    pub(crate) fn next(&mut self) -> u32 {
        self.0.next_u32()
    }

    /// A fixed-point fraction from 0 to [`ONE`].
    pub(crate) fn unit(&mut self) -> i32 {
        self.0.unit().raw()
    }
}

/// Smooth value noise, 0 to [`ONE`], on a lattice one unit apart at a
/// fixed-point position.
pub(crate) fn value_noise(seed: u32, x: i64, y: i64) -> i32 {
    let hash = |x: i64, y: i64| -> i64 {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ seed.wrapping_mul(0xcb1a_b31f);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        i64::from(h & 0xffff)
    };
    let one = i64::from(ONE);
    let (ix, iy) = (x.div_euclid(one), y.div_euclid(one));
    let ease = |f: i64| f * f / one * (3 * one - 2 * f) / one;
    let (u, v) = (ease(x.rem_euclid(one)), ease(y.rem_euclid(one)));
    let (a, b) = (hash(ix, iy), hash(ix + 1, iy));
    let (c, d) = (hash(ix, iy + 1), hash(ix + 1, iy + 1));
    let top = a * one + (b - a) * u;
    let bottom = c * one + (d - c) * u;
    ((top * one + (bottom - top) * v) / (one * one)) as i32
}

/// Fractal noise: `octaves` layers of [`value_noise`], the first with
/// swells `cell` tiles across (fixed point), each finer layer half the
/// size and half as strong (`gain` 0.5), stopping below 1.5 tiles. Raw,
/// about 0.2 to 0.8; [`fbm`] stretches it.
pub(crate) fn fbm_raw(n: usize, seed: u32, cell: i32, octaves: usize) -> Grid {
    let mut layers = Vec::new();
    let mut c = i64::from(cell.max(ONE));
    for k in 0..octaves {
        if c < i64::from(ONE) * 3 / 2 {
            break;
        }
        layers.push((
            seed.wrapping_add(0x9e37_79b9_u32.wrapping_mul(k as u32 + 1)),
            c,
        ));
        c /= 2;
    }
    let norm: i64 = (0..layers.len())
        .map(|k| 1i64 << (16 - k))
        .sum::<i64>()
        .max(1);
    let mut sum = vec![0i64; n * n];
    for (k, &(s, c)) in layers.iter().enumerate() {
        // Each layer is moved by its own offset so the lattices don't line up.
        let off = i64::from(s & 0xffff) * 37;
        let noise = |x: usize, y: usize| {
            let px = (x as i64 * i64::from(ONE) * i64::from(ONE)) / c + off;
            let py = (y as i64 * i64::from(ONE) * i64::from(ONE)) / c + off / 3;
            i64::from(value_noise(s, px, py))
        };
        // A broad layer is worked out every `step` tiles and drawn
        // straight between (a swell eight or more steps across barely
        // bends between them), which saves most of the work.
        let step = (c / (8 * i64::from(ONE))).clamp(1, 8);
        let step = 1usize << (63 - step.leading_zeros());
        let weight = 16 - k as u32;
        if step == 1 {
            for y in 0..n {
                for x in 0..n {
                    sum[y * n + x] += noise(x, y) << weight;
                }
            }
            continue;
        }
        let m = n.div_ceil(step) + 1;
        let coarse: Vec<i64> = (0..m * m)
            .map(|i| noise((i % m) * step, (i / m) * step))
            .collect();
        let st = step as i64;
        for y in 0..n {
            let (j, fy) = (y / step, (y % step) as i64);
            for x in 0..n {
                let (i, fx) = (x / step, (x % step) as i64);
                let a = coarse[j * m + i];
                let b = coarse[j * m + i + 1];
                let c2 = coarse[(j + 1) * m + i];
                let d = coarse[(j + 1) * m + i + 1];
                let top = a * (st - fx) + b * fx;
                let bottom = c2 * (st - fx) + d * fx;
                let v = (top * (st - fy) + bottom * fy) / (st * st);
                sum[y * n + x] += v << weight;
            }
        }
    }
    Grid {
        n,
        v: sum.iter().map(|&v| (v / norm) as i32).collect(),
    }
}

/// [`fbm_raw`] stretched to 0 to [`ONE`] between its 0.5th and 99.5th
/// percentiles.
pub(crate) fn fbm(n: usize, seed: u32, cell: i32, octaves: usize) -> Grid {
    fbm_raw(n, seed, cell, octaves).stretch(5, 995)
}

/// `field` bent by noise: each tile reads the field up to `strength` tiles
/// (fixed point) away, in a direction set by two fractal noises with
/// swells `scale` tiles across.
pub(crate) fn warp(field: &Grid, seeds: &mut Seeds, strength: i32, scale: i32) -> Grid {
    let n = field.n;
    let wx = fbm(n, seeds.next(), scale, 4);
    let wy = fbm(n, seeds.next(), scale, 4);
    let half = ONE / 2;
    Grid::from_fn(n, |x, y| {
        let i = y * n + x;
        let dx = i64::from(mul(strength, wx.v[i] - half));
        let dy = i64::from(mul(strength, wy.v[i] - half));
        field.sample(
            x as i64 * i64::from(ONE) + dx,
            y as i64 * i64::from(ONE) + dy,
        )
    })
}

/// Ridged multifractal noise, 0 to [`ONE`]: sharp, branching crests, each
/// layer weighted by the one before so detail gathers on the ridges, then
/// warped by `bend` tiles.
pub(crate) fn ridged(n: usize, seeds: &mut Seeds, cell: i32, octaves: usize, bend: i32) -> Grid {
    let mut out = vec![0i64; n * n];
    let mut weight = vec![ONE; n * n];
    let (mut c, mut total) = (cell, 0i64);
    for k in 0..octaves {
        if c < 2 * ONE {
            break;
        }
        let o = fbm(n, seeds.next(), c, 1);
        let amp = 1i64 << (16 - k);
        for i in 0..n * n {
            let s = ONE - (2 * o.v[i] - ONE).abs();
            let s = mul(mul(s, s), weight[i]);
            weight[i] = (s * 2).clamp(0, ONE);
            out[i] += i64::from(s) * amp;
        }
        total += amp;
        c /= 2;
    }
    let raw = Grid {
        n,
        v: out.iter().map(|&v| (v / total.max(1)) as i32).collect(),
    };
    let lo = raw.v.iter().copied().min().unwrap_or(0);
    let hi = raw.v.iter().copied().max().unwrap_or(ONE);
    let norm = raw.map(|v| div(v - lo, (hi - lo).max(1)));
    warp(&norm, seeds, bend, cell * 3 / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_point_basics() {
        assert_eq!(mul(3 * ONE, ONE / 2), 3 * ONE / 2);
        assert_eq!(div(ONE, 4 * ONE), ONE / 4);
        assert_eq!(smoothstep(0, ONE, ONE / 2), ONE / 2);
        assert_eq!(smoothstep(ONE, 0, ONE / 4), smoothstep(0, ONE, 3 * ONE / 4));
        assert_eq!(sqrt(4 * ONE), 2 * ONE);
        assert_eq!(hypot(3 * ONE, 4 * ONE), 5 * ONE);
    }

    #[test]
    fn fbm_spans_zero_to_one_and_is_smooth() {
        let g = fbm(128, 7, 32 * ONE, 5);
        assert_eq!(*g.v.iter().min().unwrap(), 0);
        assert_eq!(*g.v.iter().max().unwrap(), ONE);
        // Neighbours differ by a small step.
        let worst = (1..128).map(|x| (g.v[x] - g.v[x - 1]).abs()).max().unwrap();
        assert!(worst < ONE / 6, "{worst}");
    }

    #[test]
    fn distances_are_close_to_straight_lines() {
        let n = 64;
        let mut mask = vec![false; n * n];
        mask[32 * n + 32] = true;
        let d = distance(&mask, n, 100);
        for (x, y) in [(40, 32), (32, 50), (50, 50), (36, 60)] {
            let want = ((x as f64 - 32.0).powi(2) + (y as f64 - 32.0).powi(2)).sqrt();
            let got = f64::from(d.v[y * n + x]) / f64::from(ONE);
            assert!(
                (got - want).abs() <= want * 0.03 + 0.01,
                "{x},{y}: {got} vs {want}"
            );
        }
    }

    #[test]
    fn blur_keeps_a_flat_field_flat() {
        let g = Grid::new(32, 5 * ONE).blur(4, 3);
        assert!(g.v.iter().all(|&v| v == 5 * ONE));
    }
}
