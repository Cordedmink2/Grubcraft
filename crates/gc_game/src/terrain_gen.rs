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
        Heightmap {
            samples_per_tile,
            size,
            max_height,
            water_level: 0.17 * max_height,
            heights,
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
