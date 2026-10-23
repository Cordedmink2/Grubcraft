//! Weather: raindrops cutting gullies and laying fans (docs/27, step 3).
//!
//! Each raindrop is dropped on a random spot and rolls downhill. Fast water
//! on a steep slope can carry a lot of soil, so it picks soil up; where the
//! land flattens or climbs it can carry less, so it sets soil down. Over
//! many drops that cuts branching gullies and leaves smooth fans below them.
//!
//! All fixed point ([`ONE`] is 1.0). Positions, heights and amounts are
//! 16.16 numbers held in `i64` so the products cannot overflow. The drops
//! run one after another in a fixed order, so the same seed gives the same
//! land on every machine.

use super::field::{Grid, ONE};
use gc_fixed::Pcg32;

/// How long a raindrop lives, in steps of about one tile.
const STEPS: usize = 48;
/// How much of its old heading a drop keeps (0.08).
const INERTIA: i64 = 5243;
/// Soil carried per unit of speed, water and drop in height.
const CAPACITY: i64 = 4 * ONE as i64;
/// The least drop in height counted when working out what a drop can carry
/// (0.01), so a drop on flat ground still carries a little.
const MIN_SLOPE: i64 = 655;
/// The share of surplus soil set down each step (0.25).
const DEPOSIT: i64 = ONE as i64 / 4;
/// The share of spare capacity picked up each step (0.35).
const ERODE_RATE: i64 = 11469;
/// The share of water lost each step (0.03).
const EVAPORATE: i64 = 1966;
/// How much a drop speeds up per unit of height lost.
const GRAVITY: i64 = 8 * ONE as i64;
/// The slowest a drop's speed squared can get (0.01).
const MIN_SPEED_SQ: i64 = 655;

/// `a * b` for 16.16 numbers held in `i64`.
fn m(a: i64, b: i64) -> i64 {
    (a * b) >> 16
}

/// Particle erosion: `droplets` raindrops run downhill, picking up soil
/// on steep ground and dropping it where the land flattens.
pub(super) fn erode(h: &mut Grid, seed: u32, droplets: usize) {
    let n = h.n;
    if n < 4 {
        return;
    }
    let one = i64::from(ONE);
    let ni = n as i64;
    let mut rng = Pcg32::new(u64::from(seed), 0x0065_726f_6465);
    // The furthest a drop may go and still have a cell to its east and south.
    let limit = (ni - 1) * one;
    let span = ((ni - 2) * one) as u64;

    for _ in 0..droplets {
        let mut px = ((u64::from(rng.next_u32()) * span) >> 32) as i64;
        let mut py = ((u64::from(rng.next_u32()) * span) >> 32) as i64;
        let (mut dx, mut dy) = (0i64, 0i64);
        let (mut speed, mut water, mut sed) = (one, one, 0i64);
        let mut alive_at_end = true;

        for _ in 0..STEPS {
            // The cell the drop is in, and where in it.
            let (ix, iy) = ((px >> 16) as usize, (py >> 16) as usize);
            let (fx, fy) = (px & 0xffff, py & 0xffff);
            let i00 = iy * n + ix;
            let (h00, h10) = (i64::from(h.v[i00]), i64::from(h.v[i00 + 1]));
            let (h01, h11) = (i64::from(h.v[i00 + n]), i64::from(h.v[i00 + n + 1]));

            // Which way is down here, and the height under the drop.
            let gx = ((h10 - h00) * (one - fy) + (h11 - h01) * fy) >> 16;
            let gy = ((h01 - h00) * (one - fx) + (h11 - h10) * fx) >> 16;
            let hold = bilinear(h00, h10, h01, h11, fx, fy);

            // Steer: mostly downhill, a little of the old heading.
            dx = m(dx, INERTIA) - m(gx, one - INERTIA);
            dy = m(dy, INERTIA) - m(gy, one - INERTIA);
            let len = length(dx, dy);
            let moving = len > 0;
            let (nx, ny) = if moving {
                let inv = (1i64 << 32) / len;
                dx = (dx * inv) >> 16;
                dy = (dy * inv) >> 16;
                (px + dx, py + dy)
            } else {
                (px, py)
            };
            let alive = moving && nx >= 0 && nx < limit && ny >= 0 && ny < limit;

            // The change in height from where it was to where it goes.
            let dh = if alive {
                let j = (ny >> 16) as usize * n + (nx >> 16) as usize;
                bilinear(
                    i64::from(h.v[j]),
                    i64::from(h.v[j + 1]),
                    i64::from(h.v[j + n]),
                    i64::from(h.v[j + n + 1]),
                    nx & 0xffff,
                    ny & 0xffff,
                ) - hold
            } else {
                0
            };

            // What it can carry here, and whether to set soil down (positive
            // change) or pick some up (negative). A drop that dies sets down
            // everything it carries.
            let cap = m(m(m((-dh).max(MIN_SLOPE), speed), water), CAPACITY);
            let change = if !alive {
                sed
            } else if dh > 0 {
                dh.min(sed)
            } else if sed > cap {
                m(sed - cap, DEPOSIT)
            } else {
                -m(cap - sed, ERODE_RATE).min(-dh).max(0)
            };
            sed -= change;

            spread(h, i00, fx, fy, change);

            if !alive {
                alive_at_end = false;
                break;
            }
            // Downhill speeds it up, uphill slows it; water dries up.
            let sq = (m(speed, speed) - m(dh, GRAVITY)).max(MIN_SPEED_SQ);
            speed = root_near(sq << 16, speed);
            water = m(water, one - EVAPORATE);
            px = nx;
            py = ny;
        }
        // A drop that ran its course sets down what it still carries.
        if sed != 0 && alive_at_end {
            let i = (py >> 16) as usize * n + (px >> 16) as usize;
            spread(h, i, px & 0xffff, py & 0xffff, sed);
        }
    }
}

/// The length of (`dx`, `dy`): a rough guess sharpened by a Newton step,
/// which is within a third of a percent and much quicker than an exact root.
fn length(dx: i64, dy: i64) -> i64 {
    let (a, b) = (dx.abs().max(dy.abs()), dx.abs().min(dy.abs()));
    if a == 0 {
        return 0;
    }
    root_near(dx * dx + dy * dy, a + b * 3 / 8)
}

/// The square root of `s`, starting from the guess `g` (one Newton step).
fn root_near(s: i64, g: i64) -> i64 {
    let g = g.max(1);
    (g + s / g) >> 1
}

/// Add `change` to the four corners of the cell starting at `i`, split by
/// where (`fx`, `fy`) the drop is in it. The last corner takes the
/// remainder so nothing is lost to rounding.
fn spread(h: &mut Grid, i: usize, fx: i64, fy: i64, change: i64) {
    let one = i64::from(ONE);
    let n = h.n;
    let c00 = m(change, m(one - fx, one - fy));
    let c10 = m(change, m(fx, one - fy));
    let c01 = m(change, m(one - fx, fy));
    let c11 = change - c00 - c10 - c01;
    h.v[i] += c00 as i32;
    h.v[i + 1] += c10 as i32;
    h.v[i + n] += c01 as i32;
    h.v[i + n + 1] += c11 as i32;
}

/// Four corner heights blended at (`fx`, `fy`) within the cell.
fn bilinear(h00: i64, h10: i64, h01: i64, h11: i64, fx: i64, fy: i64) -> i64 {
    let one = i64::from(ONE);
    let top = h00 * (one - fx) + h10 * fx;
    let bottom = h01 * (one - fx) + h11 * fx;
    (top * (one - fy) + bottom * fy) >> 32
}

/// Crumble slopes steeper than `talus` (fixed-point tiles per tile),
/// `iters` times. Each round, for each of the four directions in turn,
/// every tile that stands more than `talus` above its neighbour sends a
/// tenth of the excess down to it. Edge tiles only trade with the
/// neighbours they have (the prototype wrapped round the map).
pub(super) fn thermal(h: &mut Grid, talus: i32, iters: usize) {
    /// A quarter of the 0.4 rate: each direction moves this share (0.1).
    const SHARE: i64 = 6554;
    let n = h.n;
    let mut moved = vec![0i32; n * n];
    for _ in 0..iters {
        // (dx, dy) is the neighbour a tile gives to.
        for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            // Work out every move first, so the order of tiles cannot matter.
            for y in 0..n {
                for x in 0..n {
                    let (tx, ty) = (x as isize + dx, y as isize + dy);
                    let i = y * n + x;
                    moved[i] = 0;
                    if tx < 0 || ty < 0 || tx >= n as isize || ty >= n as isize {
                        continue;
                    }
                    let d = i64::from(h.v[i]) - i64::from(h.v[ty as usize * n + tx as usize]);
                    if d > i64::from(talus) {
                        moved[i] = (((d - i64::from(talus)) * SHARE) >> 16) as i32;
                    }
                }
            }
            for y in 0..n {
                for x in 0..n {
                    let i = y * n + x;
                    let mv = moved[i];
                    if mv != 0 {
                        let t = (y as isize + dy) as usize * n + (x as isize + dx) as usize;
                        h.v[i] -= mv;
                        h.v[t] += mv;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::field::{fbm, mul};
    use super::*;
    use std::time::Instant;

    fn hills(n: usize) -> Grid {
        let f = fbm(n, 11, (n as i32 / 4) * ONE, 6);
        f.map(|v| ONE * 3 / 2 + mul(v, 12 * ONE))
    }

    fn droplets(n: usize) -> usize {
        70_000 * n * n / (512 * 512)
    }

    /// Mean absolute difference between neighbouring tiles.
    fn roughness(g: &Grid) -> i64 {
        let n = g.n;
        let mut s = 0i64;
        for y in 0..n - 1 {
            for x in 0..n - 1 {
                let i = y * n + x;
                s += i64::from((g.v[i] - g.v[i + 1]).abs() + (g.v[i] - g.v[i + n]).abs());
            }
        }
        s / (n * n) as i64
    }

    #[test]
    fn same_seed_same_land() {
        let mut a = hills(128);
        let mut b = a.clone();
        erode(&mut a, 9, droplets(128));
        erode(&mut b, 9, droplets(128));
        assert_eq!(a, b);
        let mut c = hills(128);
        erode(&mut c, 10, droplets(128));
        assert_ne!(a, c);
    }

    #[test]
    fn carves_and_keeps_the_soil() {
        let n = 256;
        let before = hills(n);
        let mut h = before.clone();
        erode(&mut h, 3, droplets(n));
        thermal(&mut h, ONE * 18 / 10, 4);
        let sum = |g: &Grid| g.v.iter().map(|&v| i64::from(v)).sum::<i64>();
        let drift = (sum(&h) - sum(&before)).abs() as f64 / sum(&before) as f64;
        assert!(drift < 0.01, "mean height moved by {drift}");
        assert!(h.v.iter().all(|&v| (0..14 * ONE).contains(&v)));
        let changed = h.v.iter().zip(&before.v).filter(|(a, b)| a != b).count();
        assert!(changed > n * n / 4, "only {changed} tiles changed");
        let (r0, r1) = (roughness(&before), roughness(&h));
        assert!(r1 > r0, "no gullies: roughness {r0} -> {r1}");
        let top = |g: &Grid| g.slope().v.iter().copied().max().unwrap();
        println!(
            "roughness {r0} -> {r1}, max slope {} -> {}",
            top(&before),
            top(&h)
        );
    }

    #[test]
    fn thermal_flattens_a_spike_and_keeps_the_total() {
        let mut g = Grid::new(32, 2 * ONE);
        g.v[16 * 32 + 16] = 9 * ONE;
        let total: i64 = g.v.iter().map(|&v| i64::from(v)).sum();
        thermal(&mut g, ONE * 18 / 10, 4);
        assert!(g.v[16 * 32 + 16] < 9 * ONE);
        assert_eq!(total, g.v.iter().map(|&v| i64::from(v)).sum::<i64>());
    }

    #[test]
    fn timing() {
        for n in [512, 768] {
            let mut h = hills(n);
            let t = Instant::now();
            erode(&mut h, 1, droplets(n));
            let e = t.elapsed();
            thermal(&mut h, ONE * 18 / 10, 4);
            println!("erode+thermal at {n}: {:?} (erode {:?})", t.elapsed(), e);
        }
    }
}
