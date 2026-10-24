//! Biomes, habitats and canopy (docs/27, step 5, "Biome depth").
//!
//! Wetness (nearness to water, plus noise) and height choose one of five
//! biomes; the same few things nature uses (wetness, hollows, which way a
//! slope faces, rock, the age of the woods) then split each biome into
//! habitats; and each habitat says how much of it stands under trees or
//! tall grass. The result is the tile cover the simulation reads (open,
//! tall grass, woods, and thicket in the deepest old growth), the biome
//! `Land` byte, and the habitat byte the renderer dresses.
//!
//! Ported from the prototype's `terrain.biomes`, `habitats.habitats` and
//! `habitats.canopy`, in whole-number maths. Heights are measured from the
//! water line (the prototype's water line was 2.0 tiles, ours is
//! [`WATER_LINE`]), so a height `x` there is `WATER_LINE + (x - 2.0)` here.

use super::field::{Grid, ONE, Seeds, distance, div, fbm, hypot, mul, warp};
use super::water::Water;
use super::{CLIFF, Dials, Habitat, WATER_LINE};
use crate::def::Land;
use gc_fixed::Angle;
use gc_sim::Cover;

/// The dressing of every tile, row by row.
pub(super) struct Dress {
    /// One `gc_sim::Cover` byte per tile.
    pub(super) cover: Vec<u8>,
    /// Impassable thicket: cover Wood too, only in the deepest old growth.
    pub(super) thicket: Vec<bool>,
    /// One `crate::def::Land` byte per tile.
    pub(super) land: Vec<u8>,
    /// One `super::Habitat` byte per tile.
    pub(super) habitat: Vec<u8>,
}

/// `p` hundredths as fixed point.
const fn pct(p: i32) -> i32 {
    p * ONE / 100
}

/// `t` tiles as fixed point.
const fn tiles(t: i32) -> i32 {
    t * ONE
}

/// The biomes, before they are split into habitats.
const MEADOW: u8 = 0;
const FOREST: u8 = 1;
const FEN: u8 = 2;
const HIGHLAND: u8 = 3;
const WASTE: u8 = 4;

/// How far from a start's clearing thicket may not come, in tiles.
const THICKET_KEEP_OFF: i32 = 10;
/// A thicket knot wider or taller than this is dropped whole, so thicket
/// never forms a wall an army must walk round.
const THICKET_MAX_SIDE: i32 = 11;

/// How much of a habitat stands under trees, and how much of it is tall
/// grass, in thousandths (the prototype's `CANOPY` and `TALL`), by
/// `Habitat as usize`.
const CANOPY: [(i32, i32); 19] = [
    (20, 150),  // FlowerMeadow
    (50, 120),  // Grassland
    (220, 200), // ScrubEdge (and all the rest of it is tall grass, see `dress`)
    (60, 550),  // WetMeadow
    (930, 0),   // OldGrowth
    (620, 0),   // BirchFringe
    (800, 0),   // FernHollow
    (40, 0),    // Glade
    (50, 600),  // SedgeTussock
    (700, 0),   // AlderCarr
    (0, 200),   // PeatBog
    (30, 0),    // HeatherMoor
    (0, 0),     // Scree
    (140, 0),   // TreeLine
    (0, 0),     // BareRock
    (0, 0),     // CrackedFlat
    (0, 0),     // BoneDrift
    (0, 0),     // RibField
    (20, 50),   // DyingGrass
];

/// `exp(-d / scale)` for `d` in quarter tiles, by repeated multiplication
/// of the per-quarter-tile factor `step` (fixed point).
fn decay_table(step: i32, len: usize) -> Vec<i32> {
    let mut t = Vec::with_capacity(len);
    let mut v = ONE;
    for _ in 0..len {
        t.push(v);
        v = mul(v, step);
    }
    t
}

/// `table` at a fixed-point distance in tiles.
fn decay_at(table: &[i32], d: i32) -> i32 {
    table[((d.max(0) >> 14) as usize).min(table.len() - 1)]
}

/// `log2(1 + x)` in fixed point, close enough to compare drainage by.
fn log2p1(x: u32) -> i32 {
    let v = u64::from(x) + 1;
    let e = 63 - v.leading_zeros();
    let f = (((v << 16) >> e) as i32) - ONE; // mantissa - 1, 0 to ONE
    // log2(1 + f) is close to f * (1.3466 - 0.3466 f).
    let frac = mul(f, 88_247 - mul(22_714, f));
    (e as i32) * ONE + frac
}

/// Makes a noise field its own turned copy, smoothly (see
/// `super::Seam::blend`), so habitats chosen from it match across the
/// map's halves without a seam.
type Sym<'a> = &'a dyn Fn(Grid) -> Grid;

/// A grid grown on `k` times fewer tiles a side, read back at full size.
fn upsample(g: &Grid, n: usize, k: usize) -> Grid {
    let at = |t: usize| ((2 * t as i64 + 1 - k as i64) * i64::from(ONE)) / (2 * k as i64);
    Grid::from_fn(n, |x, y| g.sample(at(x), at(y)))
}

/// A smooth noise field `cell` tiles across, then bent by up to `bend`
/// tiles over swells `swell` tiles across, as the prototype's
/// `warp(fbm(..))`. It is grown on `k` times fewer tiles a side (these
/// fields have no detail finer than a few tiles), which is `k * k` times
/// faster.
fn bent(
    n: usize,
    seeds: &mut Seeds,
    k: usize,
    cell: i32,
    octaves: usize,
    bend: i32,
    swell: i32,
) -> Grid {
    let (small, kk) = (n.div_ceil(k), k as i32);
    let base = fbm(small, seeds.next(), cell / kk, octaves);
    upsample(&warp(&base, seeds, bend / kk, swell / kk), n, k)
}

/// Like [`bent`] but with the field itself at full detail (woods edges
/// need the fine octaves); only the bending is read from coarser noise.
fn bent_fine(
    n: usize,
    seeds: &mut Seeds,
    cell: i32,
    octaves: usize,
    bend: i32,
    swell: i32,
) -> Grid {
    const K: usize = 4;
    let base = fbm(n, seeds.next(), cell, octaves);
    let small = n.div_ceil(K);
    let wx = fbm(small, seeds.next(), swell / K as i32, 4);
    let wy = fbm(small, seeds.next(), swell / K as i32, 4);
    let at = |t: usize| ((2 * t as i64 + 1 - K as i64) * i64::from(ONE)) / (2 * K as i64);
    Grid::from_fn(n, |x, y| {
        let (cx, cy) = (at(x), at(y));
        let dx = i64::from(mul(bend, wx.sample(cx, cy) - ONE / 2));
        let dy = i64::from(mul(bend, wy.sample(cx, cy) - ONE / 2));
        base.sample(
            x as i64 * i64::from(ONE) + dx,
            y as i64 * i64::from(ONE) + dy,
        )
    })
}

/// How far (fixed-point tiles) each tile is from the nearest `true` tile,
/// up to `cap` tiles. Worked out on half the tiles a side, so it can be
/// off by a tile; these distances only set soft habitat edges.
fn far_from(mask: &[bool], n: usize, cap: i32) -> Grid {
    let half = n.div_ceil(2);
    let mut small = vec![false; half * half];
    for (i, &m) in mask.iter().enumerate() {
        if m {
            small[(i / n / 2) * half + (i % n) / 2] = true;
        }
    }
    let d = distance(&small, half, cap / 2 + 1);
    upsample(&d.map(|v| v * 2), n, 2)
}

/// The bool grid turned inside out.
fn not(m: &[bool]) -> Vec<bool> {
    m.iter().map(|&x| !x).collect()
}

/// The fields the habitats read, all per tile.
struct Fields {
    slope: Vec<i32>,
    /// Facing the sun (south-west) is positive, shade negative.
    sunny: Vec<i32>,
    dist_w: Grid,
    hollow: Vec<i32>,
    wet: Vec<i32>,
    age: Grid,
    patch: Grid,
}

fn fields(h: &Grid, water_mask: &[bool], acc: &[u32], seeds: &mut Seeds, sym: Sym) -> Fields {
    let n = h.n;
    let side = tiles(n as i32);
    let (gx, gy) = h.gradient();
    let slope: Vec<i32> = gx.v.iter().zip(&gy.v).map(|(&a, &b)| hypot(a, b)).collect();
    // The side a slope faces: positive towards the sun (south-west, the
    // map's bottom left), negative in shade. The sun is at (-0.45, 0.89).
    let sunny: Vec<i32> = (0..n * n)
        .map(|i| {
            let num = (i64::from(gx.v[i]) * 29_491 - i64::from(gy.v[i]) * 58_327) >> 16;
            let face = div(num as i32, slope[i].max(66)).clamp(-ONE, ONE);
            mul(face, div(slope[i], pct(25)).clamp(0, ONE))
        })
        .collect();
    let dist_w = far_from(water_mask, n, 150);
    // Hollows: where the land, smoothed, curves up round a dip.
    let soft = h.blur(3, 2);
    let lap = Grid::from_fn(n, |x, y| {
        let (x, y) = (x as i64, y as i64);
        soft.get(x - 1, y) + soft.get(x + 1, y) + soft.get(x, y - 1) + soft.get(x, y + 1)
            - 4 * soft.get(x, y)
    });
    let hollow = lap.blur(2, 2).v;
    let top = log2p1(acc.iter().copied().max().unwrap_or(1)).max(1);
    let near_water = decay_table(64_796, 4 * 150 + 1); // exp(-d / 22)
    let wet = (0..n * n)
        .map(|i| {
            let drain = div(log2p1(acc[i]), top);
            let w = decay_at(&near_water, dist_w.v[i])
                + mul(pct(140), hollow[i].clamp(0, pct(30)))
                + mul(pct(60), drain)
                - mul(pct(25), sunny[i].clamp(0, ONE));
            w.clamp(0, pct(150))
        })
        .collect();
    let age = sym(bent(n, seeds, 4, side / 5, 4, tiles(30), side / 4));
    let patch = sym(bent(n, seeds, 4, side / 14, 4, tiles(12), side / 10));
    Fields {
        slope,
        sunny,
        dist_w,
        hollow,
        wet,
        age,
        patch,
    }
}

/// The biome of every tile from moisture and height (the prototype's
/// `moisture_field` and `biomes`). The forest line moves with the dial, so
/// plains are more open and wild land more wooded.
fn biomes(h: &Grid, dist_w: &Grid, seeds: &mut Seeds, dials: &Dials, sym: Sym) -> Vec<u8> {
    let n = h.n;
    let side = tiles(n as i32);
    let near = decay_table(65_173, 4 * 150 + 1); // exp(-d / 45)
    let moist_noise = sym(upsample(
        &fbm(n.div_ceil(4), seeds.next(), side / 12, 4),
        n,
        4,
    ));
    let jitter = sym(bent(n, seeds, 4, side / 6, 4, tiles(40), side / 5));
    let shift = sym(bent(n, seeds, 4, side / 7, 4, tiles(40), side / 5));
    let forest_line = pct(62) - mul(dials.forest - ONE / 2, pct(45));
    (0..n * n)
        .map(|i| {
            let up = (h.v[i] - WATER_LINE - tiles(2)).clamp(0, tiles(10));
            let moist = mul(pct(60), decay_at(&near, dist_w.v[i])) + mul(pct(55), moist_noise.v[i])
                - mul(pct(6), up);
            let hh = h.v[i] - WATER_LINE + mul(pct(160), jitter.v[i] - ONE / 2);
            let m = moist + mul(pct(25), shift.v[i] - ONE / 2);
            let mut b = MEADOW;
            if m > forest_line {
                b = FOREST;
            }
            if m > pct(72) && hh < pct(140) {
                b = FEN;
            }
            if m < pct(28) {
                b = WASTE;
            }
            if hh > tiles(4) + pct(20) {
                b = HIGHLAND;
            }
            b
        })
        .collect()
}

/// Tiles down the hill from a cliff: the apron of fallen rock. It grows
/// seven tiles out, each step onto ground lower than the cliff it touches.
fn scree_apron(h: &Grid, cliff: &[bool]) -> Vec<bool> {
    let n = h.n;
    let ni = n as i64;
    let mut below = vec![false; n * n];
    let mut front: Vec<usize> = (0..n * n).filter(|&i| cliff[i]).collect();
    let mut in_front = vec![false; n * n];
    let mut seen = vec![0u8; n * n];
    for round in 1..=7u8 {
        for &i in &front {
            in_front[i] = true;
        }
        let mut next = Vec::new();
        for &i in &front {
            let (x, y) = ((i % n) as i64, (i / n) as i64);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (px, py) = (x + dx, y + dy);
                if !(0..ni).contains(&px) || !(0..ni).contains(&py) {
                    continue;
                }
                let p = (py * ni + px) as usize;
                if seen[p] == round || cliff[p] {
                    continue;
                }
                seen[p] = round;
                // The highest front tile in the 3x3 round p.
                let mut top = i32::MIN;
                for qy in (py - 1).max(0)..=(py + 1).min(ni - 1) {
                    for qx in (px - 1).max(0)..=(px + 1).min(ni - 1) {
                        let q = (qy * ni + qx) as usize;
                        if in_front[q] {
                            top = top.max(h.v[q]);
                        }
                    }
                }
                if h.v[p] < top {
                    next.push(p);
                }
            }
        }
        for &i in &front {
            in_front[i] = false;
        }
        for &p in &next {
            below[p] = true;
        }
        front = next;
    }
    below
}

/// The habitat of every tile (the prototype's `habitats`).
fn habitats(h: &Grid, biome: &[u8], f: &Fields, seeds: &mut Seeds, sym: Sym) -> Vec<Habitat> {
    let n = h.n;
    let forest: Vec<bool> = biome.iter().map(|&b| b == FOREST).collect();
    let waste: Vec<bool> = biome.iter().map(|&b| b == WASTE).collect();
    let near_forest = far_from(&forest, n, 20);
    let inner = far_from(&not(&forest), n, 20);
    let from_waste = far_from(&not(&waste), n, 20);
    let glade = sym(bent(n, seeds, 2, tiles(22), 3, tiles(10), tiles(30)));
    let carr = sym(bent(n, seeds, 4, tiles(40), 4, tiles(16), tiles(40)));
    let cliff: Vec<bool> = f.slope.iter().map(|&s| s > CLIFF).collect();
    let apron = scree_apron(h, &cliff);
    let mut out = vec![Habitat::Grassland; n * n];
    for i in 0..n * n {
        let (wet, sunny, slope) = (f.wet[i], f.sunny[i], f.slope[i]);
        let (patch, age, hollow) = (f.patch.v[i], f.age.v[i], f.hollow[i]);
        // Height as the prototype measured it, over its water line.
        let height = h.v[i] - WATER_LINE + tiles(2);
        out[i] = match biome[i] {
            MEADOW => {
                if near_forest.v[i]
                    < tiles(4) + mul(tiles(7), patch) + mul(tiles(6), glade.v[i] - ONE / 2)
                    || (slope > pct(35) && patch < pct(40))
                {
                    Habitat::ScrubEdge
                } else if wet < pct(55) && sunny > pct(5) && patch > pct(55) {
                    Habitat::FlowerMeadow
                } else if wet > pct(62) {
                    Habitat::WetMeadow
                } else {
                    Habitat::Grassland
                }
            }
            FOREST => {
                if glade.v[i] > pct(80) && inner.v[i] > tiles(4) {
                    Habitat::Glade
                } else if hollow > 983 && wet > pct(35) && age > pct(30) {
                    Habitat::FernHollow
                } else if inner.v[i] < tiles(3) + mul(tiles(6), patch) || age < pct(22) {
                    Habitat::BirchFringe
                } else {
                    Habitat::OldGrowth
                }
            }
            FEN => {
                if carr.v[i] > pct(60) && f.dist_w.v[i] > ONE {
                    Habitat::AlderCarr
                } else if hollow > 655 && patch > pct(50) {
                    Habitat::PeatBog
                } else {
                    Habitat::SedgeTussock
                }
            }
            HIGHLAND => {
                // The tree line: higher on sunny sides.
                let tree_line = tiles(9) + mul(pct(150), patch - ONE / 2) + mul(pct(80), sunny);
                if height > tree_line + pct(90) {
                    Habitat::BareRock
                } else if (height - tree_line).abs() < pct(90) {
                    Habitat::TreeLine
                } else {
                    Habitat::HeatherMoor
                }
            }
            _ => {
                // Bone dust combed by the wind into ripples on the lee side.
                let (x, y) = ((i % n) as i64, (i / n) as i64);
                let turn = x * 1669 + y * 939 + ((i64::from(patch) * 62_582) >> 16);
                let ripple = pct(50) + mul(pct(50), Angle(turn as u16).sin().raw());
                if from_waste.v[i] < tiles(7) + mul(tiles(10), patch) {
                    Habitat::DyingGrass
                } else if patch < pct(30) {
                    Habitat::RibField
                } else if ripple > pct(62) && age > pct(45) {
                    Habitat::BoneDrift
                } else {
                    Habitat::CrackedFlat
                }
            }
        };
        // Rock that fell lies in an apron below each cliff; the cliffs
        // themselves are bare rock.
        if biome[i] != WASTE {
            if apron[i] && patch > pct(25) && h.v[i] > WATER_LINE + pct(40) {
                out[i] = Habitat::Scree;
            }
            if cliff[i] && h.v[i] > WATER_LINE {
                out[i] = Habitat::BareRock;
            }
        }
    }
    out
}

/// Drop every knot of thicket wider or taller than [`THICKET_MAX_SIDE`]
/// tiles, so thicket is only ever small stands, never a wall.
pub(super) fn drop_long_thicket(thick: &mut [bool], n: usize) {
    let mut seen = vec![false; n * n];
    let mut stack = Vec::new();
    let mut knot = Vec::new();
    for start in 0..n * n {
        if !thick[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        knot.clear();
        let (mut x0, mut x1, mut y0, mut y1) = (n, 0, n, 0);
        while let Some(i) = stack.pop() {
            knot.push(i);
            let (x, y) = (i % n, i / n);
            (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
            let mut push = |j: usize| {
                if thick[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1);
            }
            if x + 1 < n {
                push(i + 1);
            }
            if y > 0 {
                push(i - n);
            }
            if y + 1 < n {
                push(i + n);
            }
        }
        if ((x1 - x0).max(y1 - y0) as i32) >= THICKET_MAX_SIDE {
            for &i in &knot {
                thick[i] = false;
            }
        }
    }
}

/// Dress the land: biomes, habitats, cover and thicket for every tile.
/// `keep` marks the flat open clearings round the starts: open, meadow,
/// and no thicket near them.
pub(super) fn dress(
    h: &Grid,
    water: &Water,
    seeds: &mut Seeds,
    dials: &Dials,
    keep: &[bool],
    sym: Sym,
) -> Dress {
    let n = h.n;
    let nn = n * n;
    let wet_tile: Vec<bool> = (0..nn)
        .map(|i| h.v[i] < WATER_LINE || water.river[i] || water.lake[i])
        .collect();
    let f = fields(h, &wet_tile, &water.acc, seeds, sym);
    let biome = biomes(h, &f.dist_w, seeds, dials, sym);
    let mut hab = habitats(h, &biome, &f, seeds, sym);
    // Clearings round the starts are plain open meadow.
    for i in 0..nn {
        if keep[i] {
            hab[i] = if f.patch.v[i] > pct(55) {
                Habitat::FlowerMeadow
            } else {
                Habitat::Grassland
            };
        }
    }

    // Canopy: each habitat's share of trees is met by thresholding one
    // warped field, so woods stay lobed with ragged edges. The dial scales
    // the shares: fewer trees on plains, more in the wild.
    let field = sym(bent_fine(n, seeds, tiles(28), 5, tiles(14), tiles(36)));
    let tall_f = sym(bent(n, seeds, 2, tiles(20), 4, tiles(10), tiles(30)));
    let knots = sym(upsample(
        &fbm(n.div_ceil(2), seeds.next(), tiles(5), 3),
        n,
        2,
    ));
    let scale = ONE + mul(dials.forest - ONE / 2, tiles(3));
    let share = Grid::from_fn(n, |x, y| {
        let i = y * n + x;
        let s = CANOPY[hab[i] as usize].0 * ONE / 1000;
        mul(s, scale).min(pct(97))
    })
    // Soften the share across habitat borders so woods thin out, not stop.
    .blur(3, 3);
    let woods: Vec<bool> = (0..nn)
        .map(|i| {
            !wet_tile[i] && field.v[i] < share.v[i] && f.slope[i] < mul(CLIFF, pct(80)) && !keep[i]
        })
        .collect();
    // Thicket: only deep in old growth, in small knots.
    let core = far_from(&not(&woods), n, 12);
    let near_keep = far_from(keep, n, THICKET_KEEP_OFF + 2);
    let mut thicket: Vec<bool> = (0..nn)
        .map(|i| {
            woods[i]
                && hab[i] == Habitat::OldGrowth
                && core.v[i] > tiles(6)
                && field.v[i] < share.v[i] - pct(35)
                && knots.v[i] > pct(66)
                && near_keep.v[i] >= tiles(THICKET_KEEP_OFF)
        })
        .collect();
    drop_long_thicket(&mut thicket, n);

    // Cover, and the biome's land byte (river valleys are Valley).
    let river_dist = far_from(&water.river, n, 6);
    let mut cover = vec![Cover::Open as u8; nn];
    let mut land = vec![0u8; nn];
    for i in 0..nn {
        let hb = hab[i];
        let (_, tall) = CANOPY[hb as usize];
        land[i] = hb.land() as u8;
        if keep[i] || wet_tile[i] {
            thicket[i] = false;
        } else if woods[i] {
            cover[i] = Cover::Wood as u8;
        } else if hb == Habitat::ScrubEdge || tall_f.v[i] > ONE - tall * ONE / 1000 {
            // Scrub is all light cover, which a growing base clears (idea
            // 22): woods where its canopy share says trees, tall grass
            // between.
            cover[i] = Cover::Grass as u8;
        }
        if thicket[i] {
            cover[i] = Cover::Wood as u8;
        }
        let valley = river_dist.v[i] <= tiles(4)
            && h.v[i] < WATER_LINE + tiles(2)
            && !keep[i]
            && matches!(hb.land(), Land::Meadow | Land::Waste);
        if valley {
            land[i] = Land::Valley as u8;
        }
    }
    Dress {
        cover,
        thicket,
        land,
        habitat: hab.iter().map(|&b| b as u8).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Grown, grow, open_reach};
    use super::*;
    use crate::wilds::Wildness;
    use std::time::Instant;

    /// Land from the real pipeline, with a made-up drainage: low ground
    /// gathers water, and a thin band just over the water line is river.
    fn setup(n: usize, seed: u64, w: Wildness) -> (Grid, Water, Vec<bool>, Dials) {
        let h = grow(
            n as u32,
            Grown {
                seed,
                wildness: w,
                fairness: super::super::Fairness::Mirrored,
            },
        )
        .height
        .clone();
        let acc =
            h.v.iter()
                .map(|&v| {
                    let low = ((WATER_LINE + tiles(6) - v).clamp(0, tiles(6)) / ONE) as u32;
                    1 + low * low * low * low * 50
                })
                .collect();
        // Stand-ins for lakes and rivers: the lowest ground, and a band
        // above it.
        let low = h.percentile(80);
        let high = h.percentile(110);
        let lake = h.v.iter().map(|&v| v <= low).collect();
        let river = h.v.iter().map(|&v| v > low && v <= high).collect();
        let starts = crate::generate::plateaus(n as u32);
        let open = open_reach(n);
        let keep = (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as i32, (i / n) as i32);
                starts
                    .iter()
                    .any(|&(sx, sy)| hypot((x - sx) * ONE, (y - sy) * ONE) <= open * ONE)
            })
            .collect();
        (
            h,
            Water {
                acc,
                river,
                lake,
                fords: Vec::new(),
            },
            keep,
            Dials::of(w),
        )
    }

    fn run(n: usize, seed: u64, w: Wildness) -> (Dress, Vec<bool>, Grid) {
        let (h, water, keep, dials) = setup(n, seed, w);
        let mut seeds = Seeds::new(seed ^ 0xb10b);
        (
            dress(&h, &water, &mut seeds, &dials, &keep, &|g| g),
            keep,
            h,
        )
    }

    fn share(d: &Dress, f: impl Fn(usize) -> bool) -> f64 {
        let k = (0..d.cover.len()).filter(|&i| f(i)).count();
        k as f64 / d.cover.len() as f64
    }

    #[test]
    fn dressing_is_deterministic() {
        let a = run(192, 4, Wildness::Mixed).0;
        let b = run(192, 4, Wildness::Mixed).0;
        assert_eq!(a.cover, b.cover);
        assert_eq!(a.thicket, b.thicket);
        assert_eq!(a.land, b.land);
        assert_eq!(a.habitat, b.habitat);
    }

    #[test]
    fn keep_is_open_and_thicket_stays_away() {
        for w in [Wildness::Plains, Wildness::Mixed, Wildness::Wild] {
            let n = 256;
            let (d, keep, _) = run(n, 9, w);
            let near = far_from(&keep, n, 20);
            #[allow(clippy::needless_range_loop)] // (four layers read by tile)
            for i in 0..n * n {
                if keep[i] {
                    assert_eq!(d.cover[i], Cover::Open as u8);
                    assert!(!d.thicket[i]);
                    let hb = Habitat::from_u8(d.habitat[i]);
                    assert!(matches!(hb, Habitat::Grassland | Habitat::FlowerMeadow));
                }
                if d.thicket[i] {
                    assert_eq!(d.cover[i], Cover::Wood as u8);
                    assert!(near.v[i] >= tiles(THICKET_KEEP_OFF), "thicket near a start");
                }
            }
        }
    }

    #[test]
    fn cover_shares_follow_the_dial() {
        let mut woods = Vec::new();
        for w in [Wildness::Plains, Wildness::Mixed, Wildness::Wild] {
            let (mut wd, mut th, mut gr, mut cnt) = (0.0, 0.0, 0.0, 0.0);
            for seed in [1, 2, 3, 4] {
                let (d, _, _) = run(256, seed, w);
                wd += share(&d, |i| d.cover[i] == Cover::Wood as u8);
                th += share(&d, |i| d.thicket[i]);
                gr += share(&d, |i| d.cover[i] == Cover::Grass as u8);
                cnt += 1.0;
            }
            println!(
                "{w:?}: woods {:.1}% thicket {:.2}% grass {:.1}%",
                100.0 * wd / cnt,
                100.0 * th / cnt,
                100.0 * gr / cnt
            );
            assert!(th / cnt < 0.03, "thicket {}", th / cnt);
            woods.push(wd / cnt);
        }
        assert!(woods[0] < woods[1] && woods[1] < woods[2], "{woods:?}");
        assert!(woods[1] > 0.12 && woods[1] < 0.40, "mixed {}", woods[1]);
    }

    #[test]
    fn habitats_and_land_agree_and_show_up() {
        let mut seen = [0usize; 19];
        let mut total = 0usize;
        for w in [Wildness::Plains, Wildness::Mixed, Wildness::Wild] {
            for seed in [1, 2, 3, 4, 5, 6] {
                let (d, _, _) = run(256, seed, w);
                for i in 0..d.habitat.len() {
                    seen[usize::from(d.habitat[i])] += 1;
                    total += 1;
                    let land = Habitat::from_u8(d.habitat[i]).land() as u8;
                    assert!(d.land[i] == land || d.land[i] == Land::Valley as u8);
                }
            }
        }
        for (k, hb) in Habitat::ALL.iter().enumerate() {
            println!("{hb:?}: {:.2}%", 100.0 * seen[k] as f64 / total as f64);
        }
        for (k, hb) in Habitat::ALL.iter().enumerate() {
            assert!(seen[k] > 0, "{hb:?} never appears");
        }
    }

    #[test]
    fn thicket_has_no_long_walls() {
        let n = 256;
        let (d, _, _) = run(n, 3, Wildness::Wild);
        let mut left = d.thicket.clone();
        drop_long_thicket(&mut left, n);
        assert_eq!(left, d.thicket);
    }

    #[test]
    fn timing_at_768() {
        let (h, water, keep, dials) = setup(768, 1, Wildness::Mixed);
        let mut seeds = Seeds::new(1);
        let t = Instant::now();
        let d = dress(&h, &water, &mut seeds, &dials, &keep, &|g| g);
        println!("dress at 768: {:?}", t.elapsed());
        assert_eq!(d.cover.len(), 768 * 768);
    }

    /// Writes a picture of a dressed map to the folder in `GC_BIOME_DUMP`
    /// (for looking at; does nothing when it is not set).
    #[test]
    fn dump_pictures() {
        let Ok(dir) = std::env::var("GC_BIOME_DUMP") else {
            return;
        };
        const RGB: [[i32; 3]; 19] = [
            [150, 175, 80],
            [118, 148, 74],
            [104, 128, 66],
            [98, 132, 100],
            [58, 82, 48],
            [104, 130, 72],
            [70, 104, 70],
            [140, 160, 80],
            [110, 126, 84],
            [72, 96, 90],
            [92, 88, 66],
            [122, 98, 124],
            [150, 144, 134],
            [112, 118, 86],
            [126, 120, 112],
            [198, 186, 152],
            [216, 206, 178],
            [176, 160, 128],
            [160, 156, 104],
        ];
        for (name, w) in [
            ("plains", Wildness::Plains),
            ("mixed", Wildness::Mixed),
            ("wild", Wildness::Wild),
        ] {
            for seed in [3u64, 7] {
                let n = 512;
                let (d, keep, h) = run(n, seed, w);
                let low = h.percentile(80);
                let mut out = format!("P6 {n} {n} 255\n").into_bytes();
                for i in 0..n * n {
                    let mut c = RGB[usize::from(d.habitat[i])];
                    let shade = (h.get((i % n) as i64 + 1, (i / n) as i64 - 1)
                        - h.get((i % n) as i64 - 1, (i / n) as i64 + 1))
                        / (ONE / 8);
                    for v in &mut c {
                        *v += shade;
                    }
                    if d.cover[i] == Cover::Wood as u8 {
                        c = [c[0] / 4, c[1] * 4 / 10, c[2] / 4];
                    } else if d.cover[i] == Cover::Grass as u8 {
                        c = [c[0] * 9 / 10, c[1] * 105 / 100, c[2] * 8 / 10];
                    }
                    if d.thicket[i] {
                        c = [10, 20, 10];
                    }
                    if h.v[i] <= low {
                        c = [50, 90, 160];
                    }
                    if keep[i] && i % 7 == 0 {
                        c[0] += 30;
                    }
                    out.extend(c.map(|v| v.clamp(0, 255) as u8));
                }
                std::fs::write(format!("{dir}/{name}{seed}.ppm"), out).unwrap();
            }
        }
    }
}
