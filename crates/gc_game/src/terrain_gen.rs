//! Placeholder map generator for M0: rolling hide, broken rib-ridges, flat
//! start plateaus and a shallow central bowl where the Titan's Heart will sit.
//!
//! This is presentation-only for now. When pathfinding arrives (M1) the map
//! will come from a baked map file and the simulation will read heights in
//! fixed point; floats are fine here because nothing deterministic uses them.

use glam::Vec2;

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
        let starts = [
            Vec2::new(n * 0.25, n * 0.25),
            Vec2::new(n * 0.75, n * 0.25),
            Vec2::new(n * 0.25, n * 0.75),
            Vec2::new(n * 0.75, n * 0.75),
        ];
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
}
