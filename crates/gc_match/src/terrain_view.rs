//! The map as the game's float-side code wants it: start positions for
//! deployment, the ground as the computer players read it, and the grass's
//! colour by land for the renderer. Only the
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

/// Tiles a side of one texel of the ground tint.
const TINT_TILES: u32 = 4;

/// The grass's colour by the map's lands, for the renderer: one texel per
/// [`TINT_TILES`] square, the average of its tiles' land colours, and the
/// texels a side. Empty for a map without lands, which keeps the plain
/// palette.
pub fn ground_tint(map: &Map) -> (u32, Vec<[u8; 4]>) {
    let tiles = map.tiles;
    if map.land.len() != (tiles * tiles) as usize {
        return (0, Vec::new());
    }
    let side = tiles.div_ceil(TINT_TILES);
    let mut tint = Vec::with_capacity((side * side) as usize);
    for ty in 0..side {
        for tx in 0..side {
            let (mut sum, mut count) = ([0u32; 3], 0);
            for y in ty * TINT_TILES..((ty + 1) * TINT_TILES).min(tiles) {
                for x in tx * TINT_TILES..((tx + 1) * TINT_TILES).min(tiles) {
                    let land = map.land_at((y * tiles + x) as usize);
                    let c = land.unwrap_or_default().colour();
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += u32::from(v);
                    }
                    count += 1;
                }
            }
            let [r, g, b] = sum.map(|s| (s / count) as u8);
            tint.push([r, g, b, 255]);
        }
    }
    (side, tint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_map::def::Land;

    #[test]
    fn the_tint_averages_the_lands_and_a_map_without_them_has_none() {
        let mut map = Map::generate(1, 64);
        assert_eq!(ground_tint(&map).1.len(), 0);
        // West half forest, east half waste.
        map.land = (0..64 * 64)
            .map(|i| if i % 64 < 32 { Land::Forest } else { Land::Waste } as u8)
            .collect();
        let (side, tint) = ground_tint(&map);
        assert_eq!(side, 16);
        let rgb = |c: [u8; 3]| [c[0], c[1], c[2], 255];
        assert_eq!(tint[0], rgb(Land::Forest.colour()));
        assert_eq!(tint[15], rgb(Land::Waste.colour()));
    }
}
