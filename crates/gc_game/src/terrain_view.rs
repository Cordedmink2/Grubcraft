//! The float views of the map, for drawing and the computer players, which
//! only the host runs. The map itself is made in `terrain_gen.rs` without
//! floats, so every machine makes the same one.

use glam::Vec2;

use crate::terrain_gen::{Heightmap, MAX_HEIGHT, WATER, start_tiles};

/// Centres of the four start plateaus: north-west, north-east, south-west,
/// south-east (north is towards smaller z, where the camera looks).
pub fn start_positions(tiles: u32) -> [Vec2; 4] {
    start_tiles(tiles).map(|(x, y)| Vec2::new(x as f32, y as f32))
}

impl Heightmap {
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
