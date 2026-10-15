//! Placeholder map generator for M0: rolling hide, broken rib-ridges, flat
//! start plateaus and a shallow central bowl where the Titan's Heart will sit.
//!
//! The heights are made with floats, which is fine for one machine. The
//! simulation only sees the walkable-ground grid derived from them by integer
//! rules ([`Heightmap::nav_costs`]); for LAN play (M2) maps will be baked
//! files, so every machine reads the same heights.

use glam::Vec2;

/// Centres of the four start plateaus: north-west, north-east, south-west,
/// south-east (north is towards smaller z, where the camera looks).
pub fn start_positions(tiles: u32) -> [Vec2; 4] {
    let n = tiles as f32;
    [
        Vec2::new(n * 0.25, n * 0.25),
        Vec2::new(n * 0.75, n * 0.25),
        Vec2::new(n * 0.25, n * 0.75),
        Vec2::new(n * 0.75, n * 0.75),
    ]
}

pub struct Heightmap {
    pub samples_per_tile: u32,
    /// Samples per side: `tiles * samples_per_tile + 1`.
    pub size: u32,
    pub max_height: f32,
    pub water_level: f32,
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
        let samples_per_tile = 4;
        let size = tiles * samples_per_tile + 1;
        let max_height = 14.0;
        let noise = Noise { seed: seed as u32 };
        let n = tiles as f32;
        let starts = start_positions(tiles);
        let centre = Vec2::splat(n * 0.5);

        let mut heights = Vec::with_capacity((size * size) as usize);
        for sz in 0..size {
            for sx in 0..size {
                let p = Vec2::new(sx as f32, sz as f32) / samples_per_tile as f32;
                let mut h = 0.28 + 0.42 * noise.fbm(p / 56.0, 5);

                // The Titan's ribs: long diagonal ridges, broken into passes.
                let rib = 1.0 - ((p.x * 0.8 + p.y * 0.35) / 21.0).sin().abs();
                let gaps = smoothstep(0.42, 0.6, noise.fbm(p / 30.0 + 17.0, 3));
                h += rib.powi(6) * 0.45 * gaps;

                // Shallow bowl around the Heart.
                let dc = p.distance(centre) / (n * 0.18);
                h -= 0.1 * (1.0 - smoothstep(0.0, 1.0, dc));

                // Flat, buildable plateaus at the four start positions.
                for s in starts {
                    let t = smoothstep(18.0, 30.0, p.distance(s));
                    h = h * t + 0.36 * (1.0 - t);
                }

                // Edges fall away so the map reads as an island of hide.
                let edge = p.x.min(p.y).min(n - p.x).min(n - p.y);
                h *= smoothstep(0.0, 10.0, edge) * 0.85 + 0.15;

                heights.push((h.clamp(0.0, 1.0) * u16::MAX as f32) as u16);
            }
        }
        let mut map = Heightmap {
            samples_per_tile,
            size,
            max_height,
            water_level: 0.17 * max_height,
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

    fn sample(&self, x: i32, z: i32) -> f32 {
        let max = self.size as i32 - 1;
        let i = (z.clamp(0, max) * self.size as i32 + x.clamp(0, max)) as usize;
        self.heights[i] as f32 * (self.max_height / u16::MAX as f32)
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

/// Ground steeper than this many tiles of rise per tile is a cliff no
/// regiment climbs; over half of it is slow going that routes avoid.
const CLIFF_RISE_PER_TILE: f32 = 1.2;
/// Water deeper than this (tiles) can't be waded; shallower water is a ford.
const FORD_DEPTH: f32 = 0.35;
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
        let units = u16::MAX as f32 / self.max_height; // height units per tile
        let cliff = (CLIFF_RISE_PER_TILE * units / cells_per_tile as f32) as u32;
        let steep = cliff / 2;
        let water = (self.water_level * units) as u32;
        let deep = water.saturating_sub((FORD_DEPTH * units) as u32);
        let mut cells = Vec::with_capacity((side * side) as usize);
        for cy in 0..side {
            for cx in 0..side {
                // The (step + 1)^2 samples covering the cell.
                let (mut lo, mut hi, mut sum, mut n) = (u32::MAX, 0, 0, 0);
                for sz in cy * step..=(cy + 1) * step {
                    for sx in cx * step..=(cx + 1) * step {
                        let h = self.heights[(sz * self.size + sx) as usize] as u32;
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
        let per_tile = gc_sim::terrain::HEIGHT_UNITS as f32;
        // Stored samples to sim units: an integer rule, so every machine
        // agrees given the same samples.
        let scale = (self.max_height * per_tile) as u64;
        let mut heights = Vec::with_capacity((tiles * tiles) as usize);
        for y in 0..tiles {
            for x in 0..tiles {
                let s = (y * self.samples_per_tile + half) * self.size
                    + x * self.samples_per_tile
                    + half;
                let h = self.heights[s as usize] as u64 * scale / u16::MAX as u64;
                heights.push(h as u16);
            }
        }
        (heights, (self.water_level * per_tile) as u16)
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
            .with_heights(heights, self.water_level)
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
        let n = ((self.size - 1) / self.samples_per_tile) as i64;
        let at =
            |x: i64, y: i64| heights[(y.clamp(0, n - 1) * n + x.clamp(0, n - 1)) as usize] as i64;
        let starts = start_positions(n as u32).map(|s| (s.x as i64, s.y as i64));
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
                let kind = if h < water as i64
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
    const ONE: i64 = 4096;
    let ease = |f: i64| {
        let t = f * ONE / cell;
        t * t * (3 * ONE - 2 * t) / (ONE * ONE)
    };
    let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
    let (u, v) = (ease(x.rem_euclid(cell)), ease(y.rem_euclid(cell)));
    let top = lattice(cx, cy) * (ONE - u) + lattice(cx + 1, cy) * u;
    let bottom = lattice(cx, cy + 1) * (ONE - u) + lattice(cx + 1, cy + 1) * u;
    (top * (ONE - v) + bottom * v) / (ONE * ONE)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct Noise {
    seed: u32,
}

impl Noise {
    fn hash(&self, x: i32, y: i32) -> f32 {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ self.seed.wrapping_mul(0xcb1a_b31f);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 65535.0
    }

    fn value(&self, p: Vec2) -> f32 {
        let i = p.floor();
        let f = p - i;
        let u = f * f * (Vec2::splat(3.0) - 2.0 * f);
        let (x, y) = (i.x as i32, i.y as i32);
        let a = self.hash(x, y);
        let b = self.hash(x + 1, y);
        let c = self.hash(x, y + 1);
        let d = self.hash(x + 1, y + 1);
        let top = a + (b - a) * u.x;
        let bottom = c + (d - c) * u.x;
        top + (bottom - top) * u.y
    }

    fn fbm(&self, p: Vec2, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
        for _ in 0..octaves {
            sum += self.value(p * freq) * amp;
            norm += amp;
            amp *= 0.5;
            freq *= 2.03;
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
        assert_eq!(water, (hm.water_level * 256.0) as u16);
        for (x, y) in [(3, 5), (16, 16), (40, 22), (63, 63)] {
            let want = hm.height_at(x as f32 + 0.5, y as f32 + 0.5) * 256.0;
            let got = heights[y * 64 + x] as f32;
            assert!((got - want).abs() <= 1.0, "tile {x},{y}: {got} vs {want}");
        }
    }

    #[test]
    fn woods_and_grass_grow_in_sensible_amounts() {
        for seed in [1, 2, 3, 42, 777] {
            let hm = Heightmap::generate(seed, 256);
            let cover = &hm.cover;
            assert_eq!(cover.len(), 256 * 256);
            assert_eq!(
                *cover,
                Heightmap::generate(seed, 256).cover,
                "cover isn't repeatable"
            );
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
