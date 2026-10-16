//! The float views of a map, for drawing and the computer players, which
//! only the host runs. The map itself is made without floats, so every
//! machine makes the same one; nothing here feeds back into it.

#![allow(clippy::float_arithmetic)]

use glam::Vec2;

use crate::map::{MAX_HEIGHT, Map, SAMPLES_PER_TILE};

impl Map {
    /// The highest ground, in tiles, for drawing.
    pub fn max_height(&self) -> f32 {
        MAX_HEIGHT as f32
    }

    /// The water line, in tiles, for drawing and the computer players.
    pub fn water_level(&self) -> f32 {
        f32::from(self.sample_water) * self.max_height() / f32::from(u16::MAX)
    }

    fn sample(&self, x: i32, z: i32) -> f32 {
        let side = self.side() as i32;
        let max = side - 1;
        let i = (z.clamp(0, max) * side + x.clamp(0, max)) as usize;
        f32::from(self.samples[i]) * (self.max_height() / f32::from(u16::MAX))
    }

    /// Bilinear height in tiles at a map position in tiles (matches the shader).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let sx = x * SAMPLES_PER_TILE as f32;
        let sz = z * SAMPLES_PER_TILE as f32;
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

    /// Each player's start as a map position in tiles (see [`Map::starts`]).
    pub fn start_positions(&self) -> Vec<Vec2> {
        self.starts()
            .into_iter()
            .map(|(x, y)| Vec2::new(x as f32, y as f32))
            .collect()
    }
}
