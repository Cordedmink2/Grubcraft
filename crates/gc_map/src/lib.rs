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

pub mod battle;
pub mod conquest;
pub mod def;
mod generate;
pub mod grown;
mod map;
pub mod recipe;
mod stamp;
pub mod view;
pub mod wilds;

pub use def::{Kind, MapDef, Pos, SizeClass, Symmetry};
pub use generate::{START_PLATEAUS, plateaus};
pub use map::{
    CLIFF_RISE_PER_TILE, Ground, MAX_HEIGHT, Map, Placed, SAMPLES_PER_TILE, fnv1a, map_hash,
};

/// The maps a Field Battle can be played on, in the lobby's order: the
/// seeded map, a random map from each recipe, a random recipe, the Classic
/// Wilds as wild as the seed picks and at each [`wilds::Wildness`], then
/// grown land (Living Land, [`grown`]) the same way. Each group was added
/// after the last, so older replays keep their choices.
pub const FIELD_MAPS: [&str; 14] = [
    "Proving Ground",
    "Random: Pass",
    "Random: Ford",
    "Random: Hill",
    "Random: Open",
    "Random: Any",
    "Classic: Random",
    "Classic: Plains",
    "Classic: Mixed",
    "Classic: Wild",
    "Land: Random",
    "Land: Plains",
    "Land: Mixed",
    "Land: Wild",
];

/// The first Classic Wilds choice in [`FIELD_MAPS`], as wild as the seed
/// picks. Each [`wilds::Wildness`] follows it.
pub const WILDS: usize = 6;

/// The first grown land choice in [`FIELD_MAPS`], as wild as the seed
/// picks: the one a new lobby starts on. Each [`wilds::Wildness`] follows
/// it.
pub const GROWN: usize = WILDS + 1 + wilds::Wildness::ALL.len();

/// A Field Battle map: the lobby's choice (an index into [`FIELD_MAPS`]),
/// its size and its seed. The same three make the same map on every
/// machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldMap {
    pub choice: usize,
    pub tiles: u32,
    pub seed: u64,
}

impl FieldMap {
    /// Make the map. A random map whose starts aren't all joined by
    /// walkable ground is made again from the next seed (none of the seeds
    /// the tests sweep need it), so nobody is shown a broken one.
    pub fn make(self) -> Map {
        let FieldMap {
            choice,
            tiles,
            seed,
        } = self;
        enum Maker {
            Recipe(recipe::Recipe),
            Wilds(wilds::Wildness),
            Grown(wilds::Wildness),
        }
        let (recipes, lands) = (recipe::Recipe::ALL, wilds::Wildness::ALL);
        let pick = |count: usize| (seed % count as u64) as usize;
        let maker = match choice {
            0 => return Map::generate(seed, tiles),
            c if c <= recipes.len() => Maker::Recipe(recipes[c - 1]),
            c if c == recipes.len() + 1 => Maker::Recipe(recipes[pick(recipes.len())]),
            WILDS => Maker::Wilds(lands[pick(lands.len())]),
            c if c <= WILDS + lands.len() => Maker::Wilds(lands[c - WILDS - 1]),
            GROWN => Maker::Grown(lands[pick(lands.len())]),
            c if c <= GROWN + lands.len() => Maker::Grown(lands[c - GROWN - 1]),
            _ => return Map::generate(seed, tiles),
        };
        let make = |k| {
            let seed = seed.wrapping_add(k);
            Map::from_def(&match maker {
                Maker::Recipe(recipe) => recipe::describe(recipe, tiles as i32, seed),
                Maker::Wilds(wild) => wilds::describe(tiles as i32, seed, wild),
                Maker::Grown(wild) => grown::describe(tiles as i32, seed, wild),
            })
        };
        let first = make(0);
        if first.starts_joined() {
            return first;
        }
        (1..8).map(make).find(Map::starts_joined).unwrap_or(first)
    }
}

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
        assert_eq!(hash("ribcage_pass"), 0x3dd0_e864_31a5_88c4);
        assert_eq!(hash("four_teeth"), 0x44a9_affb_53d9_a5a6);
    }

    /// Choice 0 is the seeded map, as it has always been.
    #[test]
    fn choice_0_is_the_seeded_map() {
        for seed in [1, 7] {
            assert_eq!(
                FieldMap {
                    choice: 0,
                    tiles: 256,
                    seed
                }
                .make()
                .hash(),
                Map::generate(seed, 256).hash(),
                "seed {seed}"
            );
        }
    }

    /// After the recipes (see `recipe_names_are_the_lobby_names`), the
    /// lobby names the Classic Wilds and grown land by how wild.
    #[test]
    fn the_wilds_choices_are_named_for_how_wild() {
        for (first, kind) in [(WILDS, "Classic"), (GROWN, "Land")] {
            assert_eq!(FIELD_MAPS[first], format!("{kind}: Random"));
            for (k, wild) in wilds::Wildness::ALL.iter().enumerate() {
                let name = format!("{kind}: {}", wild.name());
                assert_eq!(FIELD_MAPS[first + 1 + k], name);
            }
        }
        assert_eq!(FIELD_MAPS.len(), GROWN + 1 + wilds::Wildness::ALL.len());
    }

    /// Every lobby choice, at both Small sizes, makes a map whose starts
    /// can all walk to each other and that is the size asked for (the big
    /// sizes are in `gc_mapcheck`'s tests, which run them side by side).
    #[test]
    fn every_field_map_joins_its_starts() {
        for choice in 0..FIELD_MAPS.len() {
            for tiles in [256, 320] {
                for seed in 1..=4 {
                    let map = FieldMap {
                        choice,
                        tiles,
                        seed,
                    }
                    .make();
                    let what = format!("choice {choice}, {tiles} tiles, seed {seed}");
                    assert!(map.starts_joined(), "{what}");
                    assert_eq!(map.tiles, tiles, "{what}");
                }
            }
        }
    }

    /// Like the shipped maps' fingerprints: the random Field Battle maps
    /// build the same bytes on every machine. The recipes and the stamping
    /// set these numbers, so a change to either changes every random map.
    /// Update the numbers only when that is meant.
    #[test]
    fn random_field_maps_are_the_same_on_every_machine() {
        let hash = |choice| {
            FieldMap {
                choice,
                tiles: 256,
                seed: 7,
            }
            .make()
            .hash()
        };
        assert_eq!(hash(1), 0xa2f8_4316_7491_1a36);
        assert_eq!(hash(2), 0x5359_e1ae_9a42_f5a4);
        assert_eq!(hash(3), 0x659c_f795_05bd_b656);
        assert_eq!(hash(4), 0xc44a_2da5_5aeb_eb5a);
        // The Small size for five or six players too.
        let at_320 = |choice| {
            FieldMap {
                choice,
                tiles: 320,
                seed: 1,
            }
            .make()
            .hash()
        };
        assert_eq!(at_320(1), 0x1175_1bfe_378b_818e);
        assert_eq!(at_320(4), 0x4810_c8ad_25ff_bc7c);
        // Grown land: erosion and the river lines are the parts most
        // likely to drift, and each of the dials.
        assert_eq!(hash(GROWN + 1), 0x0354_4072_643d_ed6c);
        assert_eq!(hash(GROWN + 2), 0xe996_ed2f_78a8_fc0e);
        assert_eq!(hash(GROWN + 3), 0xe3de_77b9_2aa8_6372);
    }
}
