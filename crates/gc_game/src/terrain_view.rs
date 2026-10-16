//! The map as the game's float-side code wants it: start positions for
//! deployment and the ground as the computer players read it. Only the
//! host runs the computer players; the map itself is made in `gc_map`
//! without floats, so every machine makes the same one.

use gc_map::{Map, START_PLATEAUS};
use glam::Vec2;

/// Centres of a seeded map's six start plateaus: north-west, north-east,
/// south-west, south-east, north and south (north is towards smaller z,
/// where the camera looks). Field Battles deploy on these; a described
/// map's own starts are `Map::start_positions`.
pub fn plateau_positions(tiles: u32) -> [Vec2; START_PLATEAUS] {
    gc_map::plateaus(tiles).map(|(x, y)| Vec2::new(x as f32, y as f32))
}

/// The ground as the computer players read it: the walkable-ground grid
/// plus each tile's height and where the water is, so they can pick a line
/// beside a cliff, on a rise or behind a river.
pub fn ai_terrain(map: &Map, nav: &[u8]) -> gc_ai::Terrain {
    let tiles = map.tiles;
    let mut heights = Vec::with_capacity((tiles * tiles) as usize);
    for y in 0..tiles {
        for x in 0..tiles {
            heights.push(map.height_at(x as f32 + 0.5, y as f32 + 0.5));
        }
    }
    gc_ai::Terrain::new(nav, tiles)
        .with_heights(heights, map.water_level())
        .with_cover(map.cover.clone())
}
