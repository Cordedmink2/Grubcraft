//! Placeholder map generator for M0: rolling hide, broken rib-ridges, flat
//! start plateaus and a shallow central bowl where the Titan's Heart will sit.
//!
//! Everything the simulation sees is made with whole-number maths from the
//! seed: the heights (fixed point, 1.0 = 65,536, with `gc_fixed`'s sine
//! table), then the walkable-ground grid, the tile heights and the cover.
//! Floats differ between CPUs and maths libraries, so a map made with them
//! could differ between a Mac and a Windows PC in a LAN game; this one is
//! the same everywhere (a test pins its hash, and CI runs it on all three
//! systems). Floats are used only to draw it.

use gc_fixed::{Angle, isqrt_u64};
use glam::Vec2;

/// Centres of the four start plateaus: north-west, north-east, south-west,
/// south-east (north is towards smaller z, where the camera looks).
pub fn start_positions(tiles: u32) -> [Vec2; 4] {
    start_tiles(tiles).map(|(x, y)| Vec2::new(x as f32, y as f32))
}

/// [`start_positions`] in whole tiles, for map making.
fn start_tiles(tiles: u32) -> [(i64, i64); 4] {
    let (q1, q3) = (i64::from(tiles) / 4, i64::from(tiles) * 3 / 4);
    [(q1, q1), (q3, q1), (q1, q3), (q3, q3)]
}

pub struct Heightmap {
    pub samples_per_tile: u32,
    /// Samples per side: `tiles * samples_per_tile + 1`.
    pub size: u32,
    pub heights: Vec<u16>,
    /// The simulation's tile heights and water line ([`Heightmap::sim_ground`]).
    sim_heights: Vec<u16>,
    sim_water: u16,
    /// Woods and tall grass, one [`gc_sim::Cover`] byte per tile, row by
    /// row (see [`Heightmap::grow_cover`]).
    pub cover: Vec<u8>,
}

impl Heightmap {
    pub fn generate(seed: u64, tiles: u32) -> Heightmap {
        let samples_per_tile = SAMPLES_PER_TILE as u32;
        let size = tiles * samples_per_tile + 1;
        let noise = Noise { seed: seed as u32 };
        let n = i64::from(tiles);
        // In samples: the map's centre and the start plateaus' centres.
        let centre = n * SAMPLES_PER_TILE / 2;
        let starts = start_tiles(tiles).map(|(x, z)| (x * SAMPLES_PER_TILE, z * SAMPLES_PER_TILE));
        // Squared distance in samples, and as a distance in tiles in fixed point.
        let dist2 = |dx: i64, dz: i64| dx * dx + dz * dz;
        let tiles_apart = |d2: i64| isqrt_u64((d2 as u64) << 32) as i64 / SAMPLES_PER_TILE;
        // Beyond these the bowl and the plateaus leave the height as it is,
        // so their square roots are skipped (the bowl's with a sample to spare).
        let bowl_reach2 = (72 * n / 100 + 1).pow(2);
        let plateau_reach2 = (30 * SAMPLES_PER_TILE).pow(2);
        let last = n * SAMPLES_PER_TILE;

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

                // Flat, buildable plateaus at the four start positions.
                for (x, z) in starts {
                    let d2 = dist2(sx - x, sz - z);
                    if d2 < plateau_reach2 {
                        let t = smoothstep(18 * ONE, 30 * ONE, tiles_apart(d2));
                        h = (h * t + ONE * 36 / 100 * (ONE - t)) / ONE;
                    }
                }

                // Edges fall away so the map reads as an island of hide.
                let edge = sx.min(sz).min(last - sx).min(last - sz) * ONE / SAMPLES_PER_TILE;
                h = h * (smoothstep(0, 10 * ONE, edge) * 85 / 100 + ONE * 15 / 100) / ONE;

                heights.push((h.clamp(0, ONE) * i64::from(u16::MAX) / ONE) as u16);
            }
        }
        let mut map = Heightmap {
            samples_per_tile,
            size,
            heights,
            sim_heights: Vec::new(),
            sim_water: 0,
            cover: Vec::new(),
        };
        (map.sim_heights, map.sim_water) = map.sim_ground();
        map.cover = map.grow_cover(seed);
        map
    }

    /// The ground the simulation plays on: tile heights, the water line,
    /// and the woods and tall grass.
    pub fn ground(&self) -> gc_sim::Ground<'_> {
        gc_sim::Ground {
            heights: &self.sim_heights,
            water_level: self.sim_water,
            cover: &self.cover,
        }
    }

    /// The highest ground, in tiles, for drawing.
    pub fn max_height(&self) -> f32 {
        MAX_HEIGHT as f32
    }

    /// The water line, in tiles, for drawing and the computer players.
    pub fn water_level(&self) -> f32 {
        WATER as f32 * self.max_height() / f32::from(u16::MAX)
    }

    fn sample(&self, x: i32, z: i32) -> f32 {
        let max = self.size as i32 - 1;
        let i = (z.clamp(0, max) * self.size as i32 + x.clamp(0, max)) as usize;
        f32::from(self.heights[i]) * (self.max_height() / f32::from(u16::MAX))
    }

    /// Bilinear height in tiles at a map position in tiles (matches the shader).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let sx = x * self.samples_per_tile as f32;
        let sz = z * self.samples_per_tile as f32;
        let (ix, iz) = (sx.floor() as i32, sz.floor() as i32);
        let (fx, fz) = (sx - ix as f32, sz - iz as f32);
        let h00 = self.sample(ix, iz);
        let h10 = self.sample(ix + 1, iz);
        let h01 = self.sample(ix, iz + 1);
        let h11 = self.sample(ix + 1, iz + 1);
        let top = h00 + (h10 - h00) * fx;
        let bottom = h01 + (h11 - h01) * fx;
        top + (bottom - top) * fz
    }
}

/// Fixed point for map making: 1.0.
const ONE: i64 = 1 << 16;
const SAMPLES_PER_TILE: i64 = 4;
/// The highest ground, in tiles: a stored height of 65,535.
const MAX_HEIGHT: i64 = 14;
/// The water line as a stored height: 0.17 of the highest ground.
const WATER: i64 = u16::MAX as i64 * 17 / 100;
/// Ground steeper than this many hundredths of a tile of rise per tile is
/// a cliff no regiment climbs; over half of it is slow going that routes
/// avoid.
const CLIFF_RISE_PER_TILE: i64 = 120;
/// Water deeper than this (hundredths of a tile) can't be waded;
/// shallower water is a ford.
const FORD_DEPTH: i64 = 35;
/// Nav costs: open ground, steep ground, fords (see `gc_sim::nav`).
const OPEN: u8 = 1;
const STEEP: u8 = 2;
const FORD: u8 = 3;

impl Heightmap {
    /// The ground cost of each half-tile cell, row by row, for the
    /// simulation's pathfinding: cliffs and deep water are blocked, steep
    /// slopes and fords cost more. Integer rules over the stored heights,
    /// so the same heights give the same grid everywhere.
    pub fn nav_costs(&self) -> Vec<u8> {
        let cells_per_tile = gc_sim::nav::CELLS_PER_TILE as u32;
        let step = self.samples_per_tile / cells_per_tile; // samples per cell
        let side = (self.size - 1) / step;
        // Stored heights per hundredth of a tile is u16::MAX / (MAX_HEIGHT * 100).
        let hundredths = |n: i64| (n * i64::from(u16::MAX) / (MAX_HEIGHT * 100)) as u32;
        let cliff = hundredths(CLIFF_RISE_PER_TILE) / cells_per_tile;
        let steep = cliff / 2;
        let water = WATER as u32;
        let deep = water.saturating_sub(hundredths(FORD_DEPTH));
        let mut cells = Vec::with_capacity((side * side) as usize);
        for cy in 0..side {
            for cx in 0..side {
                // The (step + 1)^2 samples covering the cell.
                let (mut lo, mut hi, mut sum, mut n) = (u32::MAX, 0, 0, 0);
                for sz in cy * step..=(cy + 1) * step {
                    for sx in cx * step..=(cx + 1) * step {
                        let h = u32::from(self.heights[(sz * self.size + sx) as usize]);
                        lo = lo.min(h);
                        hi = hi.max(h);
                        sum += h;
                        n += 1;
                    }
                }
                let mean = sum / n;
                let rise = hi - lo;
                cells.push(if mean < deep || rise > cliff {
                    gc_sim::nav::BLOCKED
                } else if mean < water {
                    FORD
                } else if rise > steep {
                    STEEP
                } else {
                    OPEN
                });
            }
        }
        cells
    }

    /// The heights the simulation's terrain rules use (see
    /// [`gc_sim::terrain`]): one per tile, the sample at the tile's centre,
    /// in [`gc_sim::terrain::HEIGHT_UNITS`] per tile, and the water line in
    /// the same units. The same tile-centre heights the minds read
    /// ([`Heightmap::ai_terrain`]), so both agree on what is high and wet.
    fn sim_ground(&self) -> (Vec<u16>, u16) {
        let tiles = (self.size - 1) / self.samples_per_tile;
        let half = self.samples_per_tile / 2;
        // Stored samples to sim units: an integer rule, so every machine
        // agrees given the same samples.
        let scale = MAX_HEIGHT as u64 * u64::from(gc_sim::terrain::HEIGHT_UNITS);
        let mut heights = Vec::with_capacity((tiles * tiles) as usize);
        for y in 0..tiles {
            for x in 0..tiles {
                let s = (y * self.samples_per_tile + half) * self.size
                    + x * self.samples_per_tile
                    + half;
                let h = u64::from(self.heights[s as usize]) * scale / u64::from(u16::MAX);
                heights.push(h as u16);
            }
        }
        (heights, (WATER as u64 * scale / u64::from(u16::MAX)) as u16)
    }

    /// The ground as the computer players read it: the walkable-ground
    /// grid plus each tile's height and where the water is, so they can
    /// pick a line beside a cliff, on a rise or behind a river.
    pub fn ai_terrain(&self, nav: &[u8]) -> gc_ai::Terrain {
        let tiles = (self.size - 1) / self.samples_per_tile;
        let mut heights = Vec::with_capacity((tiles * tiles) as usize);
        for y in 0..tiles {
            for x in 0..tiles {
                heights.push(self.height_at(x as f32 + 0.5, y as f32 + 0.5));
            }
        }
        gc_ai::Terrain::new(nav, tiles)
            .with_heights(heights, self.water_level())
            .with_cover(self.cover.clone())
    }
}

/// Woods and tall grass stay this far from the start plateaus' centres
/// (tiles), so armies deploy in the open and see each other.
const COVER_CLEAR_OF_STARTS: i64 = 34;
/// ... and this far from the map's edge.
const COVER_CLEAR_OF_EDGE: i64 = 10;
/// Ground steeper than this (height units per tile) grows no wood or grass.
const COVER_STEEPEST: i64 = 150;
/// Noise levels (0 to 65,535) above which woods grow, below that by the
/// fringe tall grass rings them, and above which grass fills a meadow.
const WOOD_LEVEL: i64 = 42_000;
const GRASS_FRINGE: i64 = 2_200;
const MEADOW_LEVEL: i64 = 46_500;

impl Heightmap {
    /// Woods and tall grass, one [`gc_sim::Cover`] byte per tile, row by
    /// row (docs/20-ambushes.md). Integer rules over the simulation's tile
    /// heights and the seed, so every machine that agrees on the heights
    /// agrees on the cover. Woods grow in clumps on dry, gentle ground, ringed
    /// by tall grass; more grass fills a few meadows. The start plateaus and
    /// the map's edge stay open.
    fn grow_cover(&self, seed: u64) -> Vec<u8> {
        let (heights, water) = (&self.sim_heights, self.sim_water);
        let n = i64::from((self.size - 1) / self.samples_per_tile);
        let at = |x: i64, y: i64| {
            i64::from(heights[(y.clamp(0, n - 1) * n + x.clamp(0, n - 1)) as usize])
        };
        let starts = start_tiles(n as u32);
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
struct Noise {
    seed: u32,
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
    fn fbm(&self, x: i64, y: i64, octaves: usize) -> i64 {
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
        let hm = Heightmap::generate(1, 64);
        assert_eq!(hm.size, 257);
        assert_eq!(hm.heights.len(), 257 * 257);
        // Start plateaus are flat.
        let a = hm.height_at(16.0, 16.0);
        let b = hm.height_at(18.0, 17.0);
        assert!((a - b).abs() < 0.05, "{a} vs {b}");
    }

    #[test]
    fn sim_ground_matches_the_heights_the_minds_read() {
        let hm = Heightmap::generate(1, 64);
        let (heights, water) = hm.sim_ground();
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
            let hm = Heightmap::generate(seed, 256);
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
            for (x, y) in [(64, 64), (192, 64), (64, 192), (192, 192)] {
                assert_eq!(cover[y * 256 + x], gc_sim::Cover::Open as u8);
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
        let fingerprint = |seed| {
            let hm = Heightmap::generate(seed, 256);
            let heights = crate::online::fnv1a(hm.heights.iter().flat_map(|v| v.to_le_bytes()));
            (
                heights,
                crate::online::map_hash(&hm.nav_costs(), &hm.ground()),
            )
        };
        assert_eq!(
            fingerprint(1),
            (0xfa16_1374_5be8_6d96, 0x10dc_5a39_1b6e_0613)
        );
        assert_eq!(
            fingerprint(42),
            (0xed6f_3282_3a3d_1a97, 0xebdd_c7ee_8ab8_9d84)
        );
    }

    #[test]
    fn nav_grid_blocks_cliffs_and_deep_water() {
        let hm = Heightmap::generate(1, 256);
        let nav = hm.nav_costs();
        assert_eq!(nav.len(), 512 * 512);
        let count = |c: u8| nav.iter().filter(|&&v| v == c).count() * 100 / nav.len();
        let (blocked, open, steep, ford) = (count(0), count(OPEN), count(STEEP), count(FORD));
        println!("blocked {blocked}%, open {open}%, steep {steep}%, ford {ford}%");
        // Most of the map is walkable, the edges are not.
        assert!(open > 50, "{open}% open");
        assert_eq!(nav[0], 0);
        // Start plateaus are open ground.
        assert_eq!(nav[(128 * 512 + 128) as usize], OPEN);
    }
}
