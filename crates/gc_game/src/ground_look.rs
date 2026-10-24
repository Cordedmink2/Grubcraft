//! How each habitat's ground looks (docs/27-living-land.md, docs/36-habitat-ground.md):
//! a colour for the grass and a pattern over it, one entry a tile for the
//! renderer. Presentation only, and made once from the map when it is loaded.

use gc_map::Map;
use gc_map::grown::Habitat;
use gc_render::{
    GROUND_BOG, GROUND_BONE, GROUND_CRUST, GROUND_DAMP, GROUND_FLOWERS, GROUND_GRAVEL,
    GROUND_HEATHER, GROUND_LITTER, GROUND_MOSS, GROUND_MUD, GROUND_RIPPLE, GROUND_STRAW,
};

/// What the grass colour is multiplied by (128 leaves it) and the pattern
/// (0 for none), for a habitat.
fn look_of(habitat: Habitat) -> [u8; 4] {
    use Habitat::*;
    match habitat {
        FlowerMeadow => [140, 134, 122, GROUND_FLOWERS],
        Grassland => [128, 128, 128, 0],
        ScrubEdge => [122, 118, 105, 0],
        WetMeadow => [116, 128, 122, GROUND_DAMP],
        OldGrowth => [100, 102, 90, GROUND_LITTER],
        BirchFringe => [138, 138, 122, 0],
        FernHollow => [102, 125, 102, GROUND_MOSS],
        Glade => [141, 143, 122, 0],
        SedgeTussock => [128, 125, 96, 0],
        AlderCarr => [102, 108, 90, GROUND_MUD],
        PeatBog => [90, 90, 83, GROUND_BOG],
        HeatherMoor => [122, 109, 109, GROUND_HEATHER],
        Scree => [128, 128, 128, GROUND_GRAVEL],
        TreeLine => [109, 109, 100, GROUND_LITTER],
        BareRock => [128, 128, 128, GROUND_GRAVEL],
        CrackedFlat => [128, 128, 128, GROUND_CRUST],
        BoneDrift => [128, 128, 128, GROUND_RIPPLE],
        RibField => [128, 128, 128, GROUND_BONE],
        DyingGrass => [147, 134, 102, GROUND_STRAW],
    }
}

/// The ground's look for every tile of `map`, row by row; empty for a map
/// with no habitats, which keeps the plain ground.
pub fn ground_look(map: &Map) -> Vec<[u8; 4]> {
    let tiles = (map.tiles * map.tiles) as usize;
    if map.habitat.len() != tiles {
        return Vec::new();
    }
    map.habitat
        .iter()
        .map(|&b| look_of(Habitat::from_u8(b)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_without_habitats_has_no_look() {
        assert!(ground_look(&Map::generate(1, 64)).is_empty());
    }

    #[test]
    fn the_bog_and_carr_read_wet_and_sedge_stays_plain() {
        assert_eq!(look_of(Habitat::PeatBog)[3], GROUND_BOG);
        assert_eq!(look_of(Habitat::AlderCarr)[3], GROUND_MUD);
        // Sedge stays firm and plain: it must not read as wet as the soft ground.
        assert_eq!(look_of(Habitat::SedgeTussock)[3], 0);
    }

    #[test]
    fn a_grown_map_gets_a_look_a_tile() {
        let mut map = Map::generate(1, 64);
        map.habitat = (0..64 * 64).map(|i| (i % 19) as u8).collect();
        let look = ground_look(&map);
        assert_eq!(look.len(), 64 * 64);
        assert_eq!(look[1], look_of(Habitat::from_u8(1)));
    }
}
