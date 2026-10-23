//! The seeded map: rolling hide, broken rib-ridges, flat start plateaus and
//! a shallow central bowl where the Titan's Heart will sit. Every Field
//! Battle so far has been played on one of these.
//!
//! Everything is made with whole-number maths from the seed: the heights
//! (fixed point, 1.0 = 65,536, with `gc_fixed`'s sine table) and the cover.
//! Floats differ between CPUs and maths libraries, so a map made with them
//! could differ between a Mac and a Windows PC in a network game; this one
//! is the same everywhere (a test pins its hash, and CI runs it on all
//! three systems).

use gc_fixed::{Angle, isqrt_u64};

use crate::def::{BIG_MAP_TILES, Kind, Pos};
use crate::map::{Map, Placed, SAMPLES_PER_TILE};

/// How many start plateaus a generated map has: one for each lobby slot.
pub const START_PLATEAUS: usize = 6;

/// Centres of the six start plateaus in whole tiles: north-west,
/// north-east, south-west, south-east, then north and south in the middle
/// of their edges. The first four keep their places; the last two were
/// added for six-player battles. On maps up to 320 tiles they stand a
/// quarter of the map in from the edges; on the big maps (512 and more) a
/// sixth, so the armies start far apart and not only the edges.
pub fn plateaus(tiles: u32) -> [Pos; START_PLATEAUS] {
    let n = tiles as i32;
    let inset = if n >= BIG_MAP_TILES { n / 6 } else { n / 4 };
    let (near, mid, far) = (inset, n / 2, n - inset);
    [
        (near, near),
        (far, near),
        (near, far),
        (far, far),
        (mid, near),
        (mid, far),
    ]
}

/// [`plateaus`] for map making's wider integers.
fn start_tiles(tiles: u32) -> [(i64, i64); START_PLATEAUS] {
    plateaus(tiles).map(|(x, y)| (i64::from(x), i64::from(y)))
}

/// The water line as a sample: 0.17 of the highest ground.
const WATER: u16 = (u16::MAX as u32 * 17 / 100) as u16;

impl Map {
    /// The seeded map, `tiles` on a side, with a start for each of six
    /// players on the plateaus.
    pub fn generate(seed: u64, tiles: u32) -> Map {
        let samples_per_tile = i64::from(SAMPLES_PER_TILE);
        let size = tiles * SAMPLES_PER_TILE + 1;
        let noise = Noise { seed: seed as u32 };
        let n = i64::from(tiles);
        // In samples: the map's centre and the start plateaus' centres.
        let centre = n * samples_per_tile / 2;
        let starts = start_tiles(tiles).map(|(x, z)| (x * samples_per_tile, z * samples_per_tile));
        // Squared distance in samples, and as a distance in tiles in fixed point.
        let dist2 = |dx: i64, dz: i64| dx * dx + dz * dz;
        let tiles_apart = |d2: i64| isqrt_u64((d2 as u64) << 32) as i64 / samples_per_tile;
        // Beyond these the bowl and the plateaus leave the height as it is,
        // so their square roots are skipped (the bowl's with a sample to spare).
        let bowl_reach2 = (72 * n / 100 + 1).pow(2);
        let plateau_reach2 = (30 * samples_per_tile).pow(2);
        let last = n * samples_per_tile;
        let mut heights = Vec::with_capacity((size * size) as usize);
        for sz in 0..i64::from(size) {
            for sx in 0..i64::from(size) {
                let mut h =
                    ONE * 28 / 100 + 42 * noise.fbm(sx * ONE / 224, sz * ONE / 224, 5) / 100;

                // The Titan's ribs: long diagonal ridges, broken into passes.
                // sin((0.8 x + 0.35 z) / 21) for x, z in tiles, as an angle
                // in 65,536ths of a turn.
                let turn = (80 * sx + 35 * sz) * 124_171 / 100_000;
                let rib = ONE - i64::from(Angle(turn as u16).sin().raw().abs());
                let rib2 = rib * rib / ONE;
                let rib6 = rib2 * rib2 / ONE * rib2 / ONE;
                if rib6 > 0 {
                    let (gx, gz) = (sx * ONE / 120 + 17 * ONE, sz * ONE / 120 + 17 * ONE);
                    let gaps = smoothstep(ONE * 42 / 100, ONE * 6 / 10, noise.fbm(gx, gz, 3));
                    h += rib6 * 45 / 100 * gaps / ONE;
                }

                // Shallow bowl around the Heart, reaching 0.18 of the map out.
                let d2 = dist2(sx - centre, sz - centre);
                if d2 < bowl_reach2 {
                    let dc = tiles_apart(d2) * 100 / (18 * n);
                    h -= (ONE - smoothstep(0, ONE, dc)) / 10;
                }

                // Flat, buildable plateaus at the six start positions.
                for (x, z) in starts {
                    let d2 = dist2(sx - x, sz - z);
                    if d2 < plateau_reach2 {
                        let t = smoothstep(18 * ONE, 30 * ONE, tiles_apart(d2));
                        h = (h * t + ONE * 36 / 100 * (ONE - t)) / ONE;
                    }
                }

                // Edges fall away so the map reads as an island of hide.
                let edge = sx.min(sz).min(last - sx).min(last - sz) * ONE / samples_per_tile;
                h = h * (smoothstep(0, 10 * ONE, edge) * 85 / 100 + ONE * 15 / 100) / ONE;

                heights.push((h.clamp(0, ONE) * i64::from(u16::MAX) / ONE) as u16);
            }
        }
        let mut map = Map::from_samples(tiles, heights, WATER);
        map.name = format!("Seed {seed}");
        map.players = START_PLATEAUS as u8;
        map.cover = map.grow_cover(seed);
        map.settle_growth();
        map.objects = (0..)
            .zip(plateaus(tiles))
            .map(|(owner, at)| Placed {
                kind: Kind::Start,
                at,
                owner: Some(owner),
                label: String::new(),
            })
            .collect();
        map
    }
}

/// Fixed point for map making: 1.0.
pub(crate) const ONE: i64 = 1 << 16;

/// Woods and tall grass stay this far from the start plateaus' centres
/// (tiles), so armies deploy in the open and see each other.
pub(crate) const COVER_CLEAR_OF_STARTS: i64 = 34;
/// ... and this far from the map's edge.
const COVER_CLEAR_OF_EDGE: i64 = 10;
/// Ground steeper than this (height units per tile) grows no wood or grass.
const COVER_STEEPEST: i64 = 150;
/// Noise levels (0 to 65,535) above which woods grow, below that by the
/// fringe tall grass rings them, and above which grass fills a meadow.
const WOOD_LEVEL: i64 = 42_000;
const GRASS_FRINGE: i64 = 2_200;
const MEADOW_LEVEL: i64 = 46_500;

impl Map {
    /// Woods and tall grass, one [`gc_sim::Cover`] byte per tile, row by
    /// row (docs/20-ambushes.md). Integer rules over the simulation's tile
    /// heights and the seed, so every machine that agrees on the heights
    /// agrees on the cover. Woods grow in clumps on dry, gentle ground, ringed
    /// by tall grass; more grass fills a few meadows. The start plateaus and
    /// the map's edge stay open.
    fn grow_cover(&self, seed: u64) -> Vec<u8> {
        let (heights, water) = (&self.heights, self.water);
        let n = i64::from(self.tiles);
        let at = |x: i64, y: i64| {
            i64::from(heights[(y.clamp(0, n - 1) * n + x.clamp(0, n - 1)) as usize])
        };
        let starts = start_tiles(self.tiles);
        let mut cover = Vec::with_capacity((n * n) as usize);
        for y in 0..n {
            for x in 0..n {
                let h = at(x, y);
                let slope = (at(x + 1, y) - at(x - 1, y))
                    .abs()
                    .max((at(x, y + 1) - at(x, y - 1)).abs())
                    / 2;
                let edge = x.min(y).min(n - 1 - x).min(n - 1 - y);
                let near_start = starts.iter().any(|&(sx, sy)| {
                    (x - sx) * (x - sx) + (y - sy) * (y - sy)
                        < COVER_CLEAR_OF_STARTS * COVER_CLEAR_OF_STARTS
                });
                let kind = if h < i64::from(water)
                    || slope > COVER_STEEPEST
                    || edge < COVER_CLEAR_OF_EDGE
                    || near_start
                {
                    gc_sim::Cover::Open
                } else {
                    let wood = cover_noise(seed, 0, x, y);
                    if wood > WOOD_LEVEL {
                        gc_sim::Cover::Wood
                    } else if wood > WOOD_LEVEL - GRASS_FRINGE
                        || cover_noise(seed, 1, x, y) > MEADOW_LEVEL
                    {
                        gc_sim::Cover::Grass
                    } else {
                        gc_sim::Cover::Open
                    }
                };
                cover.push(kind as u8);
            }
        }
        cover
    }
}

/// Smooth integer value noise, 0 to 65,535: two octaves (28 and 11 tiles).
fn cover_noise(seed: u64, layer: u64, x: i64, y: i64) -> i64 {
    (2 * value_noise(seed, layer, x, y, 28) + value_noise(seed, layer + 16, x, y, 11)) / 3
}

/// Value noise on a lattice `cell` tiles apart, smoothly interpolated, all
/// in integers.
fn value_noise(seed: u64, layer: u64, x: i64, y: i64, cell: i64) -> i64 {
    let lattice = |cx: i64, cy: i64| -> i64 {
        let mut h = (cx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (cy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9)
            ^ layer.wrapping_mul(0x27D4_EB2F_1656_67C5);
        h ^= h >> 31;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 29;
        (h & 0xFFFF) as i64
    };
    const STEP: i64 = 4096;
    let ease = |f: i64| {
        let t = f * STEP / cell;
        t * t * (3 * STEP - 2 * t) / (STEP * STEP)
    };
    let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
    let (u, v) = (ease(x.rem_euclid(cell)), ease(y.rem_euclid(cell)));
    let top = lattice(cx, cy) * (STEP - u) + lattice(cx + 1, cy) * u;
    let bottom = lattice(cx, cy + 1) * (STEP - u) + lattice(cx + 1, cy + 1) * u;
    (top * (STEP - v) + bottom * v) / (STEP * STEP)
}

/// 0 below `e0`, 1 above `e1` and a smooth S between, in fixed point.
fn smoothstep(e0: i64, e1: i64, x: i64) -> i64 {
    let t = ((x - e0) * ONE / (e1 - e0)).clamp(0, ONE);
    t * t / ONE * (3 * ONE - 2 * t) / ONE
}

/// Each octave of [`Noise::fbm`] is 2.03 times finer than the last.
const OCTAVE_FREQ: [i64; 5] = [65_536, 133_038, 270_067, 548_237, 1_112_920];

/// Smooth value noise in fixed point, 0 to 1.
pub(crate) struct Noise {
    pub(crate) seed: u32,
}

impl Noise {
    fn hash(&self, x: i32, y: i32) -> i64 {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ self.seed.wrapping_mul(0xcb1a_b31f);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        i64::from(h & 0xffff)
    }

    /// Noise at a fixed-point position on a lattice one unit apart.
    fn value(&self, x: i64, y: i64) -> i64 {
        let (ix, iy) = ((x >> 16) as i32, (y >> 16) as i32);
        let ease = |f: i64| smoothstep(0, ONE, f);
        let (u, v) = (ease(x & (ONE - 1)), ease(y & (ONE - 1)));
        let a = self.hash(ix, iy);
        let b = self.hash(ix + 1, iy);
        let c = self.hash(ix, iy + 1);
        let d = self.hash(ix + 1, iy + 1);
        let top = a + (b - a) * u / ONE;
        let bottom = c + (d - c) * u / ONE;
        top + (bottom - top) * v / ONE
    }

    /// `octaves` layers of noise, each finer and half as strong.
    pub(crate) fn fbm(&self, x: i64, y: i64, octaves: usize) -> i64 {
        debug_assert!(octaves <= OCTAVE_FREQ.len(), "add octave frequencies");
        let (mut sum, mut norm) = (0, 0);
        for (i, freq) in OCTAVE_FREQ[..octaves].iter().enumerate() {
            let amp = ONE >> (i + 1);
            sum += self.value(x * freq / ONE, y * freq / ONE) * amp;
            norm += amp;
        }
        sum / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_plateaus() {
        let hm = Map::generate(1, 64);
        assert_eq!(hm.side(), 257);
        assert_eq!(hm.samples.len(), 257 * 257);
        // Start plateaus are flat, all six of them.
        for (x, y) in start_tiles(64) {
            let (x, y) = (x as f32, y as f32);
            let a = hm.height_at(x, y);
            let b = hm.height_at(x + 2.0, y + 1.0);
            assert!((a - b).abs() < 0.05, "{x},{y}: {a} vs {b}");
        }
    }

    #[test]
    fn the_first_four_plateaus_keep_their_places() {
        assert_eq!(
            start_tiles(256),
            [
                (64, 64),
                (192, 64),
                (64, 192),
                (192, 192),
                (128, 64),
                (128, 192)
            ]
        );
        let starts = Map::generate(1, 256).starts();
        assert_eq!(starts.len(), START_PLATEAUS);
        assert_eq!(starts[4], (128, 64));
    }

    #[test]
    fn sim_ground_matches_the_heights_the_minds_read() {
        let hm = Map::generate(1, 64);
        let (heights, water) = (&hm.heights, hm.water);
        assert_eq!(heights.len(), 64 * 64);
        assert_eq!(water, (hm.water_level() * 256.0) as u16);
        for (x, y) in [(3, 5), (16, 16), (40, 22), (63, 63)] {
            let want = hm.height_at(x as f32 + 0.5, y as f32 + 0.5) * 256.0;
            let got = f32::from(heights[y * 64 + x]);
            assert!((got - want).abs() <= 1.0, "tile {x},{y}: {got} vs {want}");
        }
    }

    #[test]
    fn woods_and_grass_grow_in_sensible_amounts() {
        for seed in [1, 2, 3, 42, 777] {
            let hm = Map::generate(seed, 256);
            let cover = &hm.cover;
            assert_eq!(cover.len(), 256 * 256);
            let share = |c: gc_sim::Cover| {
                cover.iter().filter(|&&v| v == c as u8).count() as f32 * 100.0 / cover.len() as f32
            };
            let (wood, grass) = (share(gc_sim::Cover::Wood), share(gc_sim::Cover::Grass));
            println!("seed {seed}: wood {wood:.1}%, grass {grass:.1}%");
            assert!((4.0..18.0).contains(&wood), "seed {seed}: {wood}% wood");
            assert!((3.0..14.0).contains(&grass), "seed {seed}: {grass}% grass");
            // The start plateaus are open.
            for (x, y) in start_tiles(256) {
                assert_eq!(cover[(y * 256 + x) as usize], gc_sim::Cover::Open as u8);
            }
        }
    }

    /// The map is made with whole-number maths only, so the same seed gives
    /// the same map on every machine. CI runs this on macOS, Windows and
    /// Linux; if it fails on one of them, something in map making depends
    /// on the CPU or the maths library. If map making changes on purpose,
    /// update the numbers (golden replays keep their own maps, so they
    /// don't move).
    #[test]
    fn the_map_is_the_same_on_every_machine() {
        let fingerprint = |seed, tiles| {
            let hm = Map::generate(seed, tiles);
            let heights = crate::fnv1a(hm.samples.iter().flat_map(|v| v.to_le_bytes()));
            (heights, hm.hash())
        };
        assert_eq!(
            fingerprint(1, 256),
            (0x7dca_be37_90bd_1f7b, 0xfc37_4683_efa8_a9fa)
        );
        assert_eq!(
            fingerprint(42, 256),
            (0x8f2d_3f4f_0515_4bee, 0x5dac_62a7_2065_0a9f)
        );
        // The size the field-8000pt-2v2 golden replay plays on.
        assert_eq!(
            fingerprint(1, 384),
            (0x276a_c467_503d_c5b9, 0x0c94_b228_d3e8_0ad6)
        );
    }

    #[test]
    fn nav_grid_blocks_cliffs_and_deep_water() {
        let hm = Map::generate(1, 256);
        let nav = hm.nav_costs();
        assert_eq!(nav.len(), 512 * 512);
        let count = |c: u8| nav.iter().filter(|&&v| v == c).count() * 100 / nav.len();
        let (blocked, open, steep, ford) = (count(0), count(1), count(2), count(3));
        println!("blocked {blocked}%, open {open}%, steep {steep}%, ford {ford}%");
        // Most of the map is walkable, the edges are not.
        assert!(open > 50, "{open}% open");
        assert_eq!(nav[0], 0);
        // Start plateaus are open ground (two nav cells to a tile).
        for (x, y) in start_tiles(256) {
            assert_eq!(nav[(y * 2 * 512 + x * 2) as usize], 1, "{x},{y}");
        }
    }
}
