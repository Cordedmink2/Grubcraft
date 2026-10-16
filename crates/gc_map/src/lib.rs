//! Grubcraft's maps: where a battlefield comes from, and the one set of
//! terrain rules that turns it into what the simulation plays on.
//!
//! A map is made one of two ways, and both give the same [`Map`]:
//!
//! - [`Map::generate`]: the seeded noise map every Field Battle has played
//!   on so far (rolling hide, the Titan's rib-ridges, four start plateaus).
//! - [`Map::from_def`]: a map description, the RON format of `data/maps`
//!   ([`MapDef`]: a base height, terrain and cover stamps, objects, with
//!   rotational symmetry added by the loader). [`shipped`] lists the maps
//!   built into the game; [`MapDef::load`] reads one from a file.
//!
//! From a [`Map`], everything else is derived with the same integer rules
//! whichever way it was made: [`Map::nav_costs`] (the half-tile
//! walkable-ground grid), [`Map::ground`] (tile heights, water and cover for
//! `gc_sim::MatchSetup`), [`Map::tile_ground`] (blocked, open, steep or ford
//! per tile, for the map checker and object placement), [`Map::starts`]
//! and [`map_hash`] (the fingerprint machines compare before a network
//! match). The float views for drawing and the computer players are in
//! [`view`].
//!
//! Map making uses no floats (outside [`view`]), so every machine builds
//! the same bytes from the same seed or file; `gc_sim`'s `no_floats` test
//! and the lint below hold it to that.
//!
//! See `docs/07-world-and-maps.md` for the format and `docs/10-architecture.md`
//! for where this crate sits.

// The lint also catches float maths through inferred types or literals,
// which the token check in `gc_sim/tests/no_floats.rs` can miss. Test
// modules and the drawing views may use floats.
#![cfg_attr(not(test), deny(clippy::float_arithmetic))]

pub mod def;
mod generate;
mod map;
mod stamp;
pub mod view;

pub use def::{Kind, MapDef, Pos, SizeClass, Symmetry};
pub use generate::{START_PLATEAUS, plateaus};
pub use map::{Ground, MAX_HEIGHT, Map, Placed, SAMPLES_PER_TILE, fnv1a, map_hash};

/// The maps built into the game, by file name without `.ron`, in the
/// order the lobby lists them.
pub const SHIPPED: &[(&str, &str)] = &[
    (
        "ribcage_pass",
        include_str!("../../../data/maps/ribcage_pass.ron"),
    ),
    (
        "four_teeth",
        include_str!("../../../data/maps/four_teeth.ron"),
    ),
];

/// A shipped map's description, by file name without `.ron`
/// (`"ribcage_pass"`).
pub fn shipped(name: &str) -> Option<MapDef> {
    let (_, text) = SHIPPED.iter().find(|(n, _)| *n == name)?;
    // The shipped maps parse: a test checks every one.
    MapDef::parse(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_map_in_data_maps_is_shipped_and_parses() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/maps");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let path = e.unwrap().path();
                (path.extension()? == "ron")
                    .then(|| path.file_stem()?.to_str().map(String::from))?
            })
            .collect();
        files.sort();
        let mut shipped: Vec<String> = SHIPPED.iter().map(|(n, _)| (*n).to_string()).collect();
        shipped.sort();
        assert_eq!(files, shipped, "add new maps to SHIPPED");
        for (name, text) in SHIPPED {
            let def = MapDef::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            let map = Map::from_def(&def);
            assert_eq!(map.starts().len(), usize::from(def.players), "{name}");
        }
    }

    /// Like the seeded maps' fingerprints: a described map builds the same
    /// bytes on every machine. If a map file or the stamping changes on
    /// purpose, update the numbers.
    #[test]
    fn shipped_maps_are_the_same_on_every_machine() {
        let hash = |name| Map::from_def(&shipped(name).unwrap()).hash();
        assert_eq!(hash("ribcage_pass"), 0x70a5_cdda_6c76_7fb4);
        assert_eq!(hash("four_teeth"), 0x7628_f079_c3f5_f126);
    }
}
