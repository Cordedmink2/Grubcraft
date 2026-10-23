//! Shape and rock: the broad land, mountain country with branching
//! crests, strata benches whose edges are cliffs or ramps, and crags on
//! the highest cores (docs/27, "How a map grows", steps 1 and 2).

use super::field::{Grid, ONE, Seeds, div, fbm, mul, ridged, smoothstep, warp};
use super::{Dials, WATER_LINE};

/// What the later layers need from the shape: where mountain country is
/// (0 to [`ONE`]) and its crests.
pub(super) struct Parts {
    pub(super) mask: Grid,
    pub(super) ridges: Grid,
}

/// `t` tiles as a fixed-point length scaled from a 512-tile map to one
/// `n` across.
fn scaled(n: usize, t: i32) -> i32 {
    (i64::from(t) * i64::from(ONE) * n as i64 / 512) as i32
}

/// Broad lowlands and uplands, mountain country where a warped mask
/// allows (ridged crests), rolling hills everywhere. `calm` is 0 to
/// [`ONE`] per tile: how far mountain country is kept off (the starts).
///
/// The broad land is worked out at half the map's resolution and drawn
/// up to it: its finest swells are a few tiles across, and the erosion
/// adds the fine detail. That halves the map's making time.
pub(super) fn raw_land(n: usize, seeds: &mut Seeds, dials: &Dials, calm: &Grid) -> (Grid, Parts) {
    // An odd side works on one more tile (the last row and column
    // repeated) and drops it again.
    let even = n.div_ceil(2) * 2;
    let pad = |g: &Grid| Grid::from_fn(even, |x, y| g.get(x as i64, y as i64));
    let crop = |g: Grid| {
        if g.n == n {
            g
        } else {
            Grid::from_fn(n, |x, y| g.get(x as i64, y as i64))
        }
    };
    let (h, parts) = raw_land_at(even / 2, seeds, dials, &pad(calm).downsample(2));
    let parts = Parts {
        mask: crop(parts.mask.upsample(2)),
        ridges: crop(parts.ridges.upsample(2)),
    };
    (crop(h.upsample(2)), parts)
}

/// [`raw_land`] at `n` tiles a side, with lengths still as on a map
/// twice as big.
fn raw_land_at(n: usize, seeds: &mut Seeds, dials: &Dials, calm: &Grid) -> (Grid, Parts) {
    // Lengths on the map being made, in this grid's tiles.
    let scaled = |t: i32| scaled(n * 2, t) / 2;
    let broad = warp(
        &fbm(n, seeds.next(), (n as i32) * ONE / 2, 5),
        seeds,
        scaled(92),
        (n as i32) * ONE * 2 / 5,
    );
    let hills = warp(
        &fbm(n, seeds.next(), (n as i32) * ONE / 8, 5),
        seeds,
        scaled(30),
        (n as i32) * ONE / 6,
    );
    let mask_raw = warp(
        &fbm(n, seeds.next(), (n as i32) * ONE * 10 / 22, 4),
        seeds,
        scaled(128),
        (n as i32) * ONE / 3,
    );
    // How much of the map is mountain country: more on wild maps.
    let cut = mask_raw.percentile(1000 - dials.mountains);
    let mask = Grid {
        n,
        v: mask_raw
            .v
            .iter()
            .zip(&calm.v)
            .map(|(&m, &c)| mul(smoothstep(cut - ONE * 3 / 100, cut + ONE / 10, m), ONE - c))
            .collect(),
    };
    let ridges = ridged(n, seeds, (n as i32) * ONE / 5, 6, scaled(36));
    let h = Grid::from_fn(n, |x, y| {
        let i = y * n + x;
        let r = ridges.v[i];
        // The crests to the power 1.5 (the prototype's 1.4), so they
        // sharpen.
        let r15 = mul(r, super::field::sqrt(r));
        WATER_LINE
            + ONE * 6 / 10
            + 2 * broad.v[i]
            + mul(ONE * 12 / 10, hills.v[i])
            + mul(mask.v[i], ONE * 8 / 10 + mul(dials.peaks, r15))
    });
    (h, Parts { mask, ridges })
}

/// Strata: above a height the ground climbs in benches 1.6 tiles high. A
/// noise field sets how sharp each bench's edge is: sharp edges are
/// cliffs, soft ones ramps, so cliff lines follow the land's contours and
/// there are ways up between them.
pub(super) fn terrace(h: &Grid, seeds: &mut Seeds, dials: &Dials) -> Grid {
    let n = h.n;
    let start = WATER_LINE + ONE * 27 / 10;
    let step = ONE * 16 / 10;
    let field = warp(
        &fbm(n, seeds.next(), (n as i32) * ONE / 6, 4),
        seeds,
        30 * ONE,
        (n as i32) * ONE / 5,
    );
    Grid::from_fn(n, |x, y| {
        let i = y * n + x;
        let v = h.v[i];
        // How sharp: 1 (a plain slope) to 6 + 4 * wild.
        let sharp = ONE
            + mul(
                dials.sharpness,
                smoothstep(ONE * 35 / 100, ONE * 7 / 10, field.v[i]),
            );
        let f = div(v - start, step);
        let (whole, frac) = (f.div_euclid(ONE), f.rem_euclid(ONE));
        let half_width = div(ONE / 2, sharp);
        let t = smoothstep(ONE / 2 - half_width, ONE / 2 + half_width, frac);
        let benched = start + mul(whole * ONE + t, step);
        let w = smoothstep(start - ONE / 2, start + ONE * 8 / 10, v);
        v + mul(benched - v, w)
    })
}

/// The highest cores of mountain country become crags: a sudden wall of
/// rock along a jagged contour, so each massif has its own outline and the
/// gaps between them are the natural passes.
pub(super) fn massifs(h: &Grid, parts: &Parts, seeds: &mut Seeds, dials: &Dials) -> Grid {
    let n = h.n;
    let inside: Vec<i32> = (h.v.iter().zip(&parts.mask.v))
        .filter(|&(_, &m)| m > ONE / 2)
        .map(|(&v, _)| v)
        .collect();
    let jag = warp(
        &fbm(n, seeds.next(), (n as i32) * ONE / 10, 4),
        seeds,
        20 * ONE,
        (n as i32) * ONE / 8,
    );
    if inside.is_empty() {
        return h.clone();
    }
    let line = super::field::percentile(inside, dials.crag_line);
    // Where the wall is sheer and where it leans back into a climbable
    // slope: the rise happens over 0.35 to 3 tiles of height.
    let lean = warp(
        &fbm(n, seeds.next(), (n as i32) * ONE / 12, 3),
        seeds,
        16 * ONE,
        (n as i32) * ONE / 10,
    );
    Grid::from_fn(n, |x, y| {
        let i = y * n + x;
        let hh = h.v[i] + mul(ONE * 12 / 10, jag.v[i] - ONE / 2);
        let soft = smoothstep(ONE * 55 / 100, ONE * 85 / 100, lean.v[i]);
        let width = ONE * 35 / 100 + mul(ONE * 27 / 10, soft);
        let core = smoothstep(line, line + width, hh);
        h.v[i] + mul(core, dials.crags + mul(ONE * 15 / 10, parts.ridges.v[i]))
    })
}
