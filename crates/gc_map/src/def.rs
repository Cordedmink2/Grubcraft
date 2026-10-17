//! The map description: what a map author writes in `data/maps/*.ron`.
//!
//! A map is a few dozen shapes and objects, not a picture: the terrain is a
//! base height with shapes stamped on it, so a Warfield map is a few
//! kilobytes of text and anyone can read why a pass is where it is. Heights
//! are in sixteenths of a tile (16 is one tile of height). Shapes use point
//! coordinates (a tile's corner grid); objects name the tile they stand on.
//!
//! With a symmetry set, the author describes the whole map but only needs to
//! write the shapes and objects of one start's quarter (or half): the loader
//! adds the rotated copies, with owners moved on, so the result is exactly
//! symmetric by construction.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum SizeClass {
    /// 128 × 128 tiles, 2 players.
    Duel,
    /// 192 × 192 tiles, 2 to 3 players.
    Skirmish,
    /// 256 × 256 tiles, 4 players.
    Warfield,
    /// 320 × 320 tiles, 5 to 6 players.
    Grand,
    /// A Field Battle map of this many tiles a side, for 2 to 6 players:
    /// armies and their starts, no economy.
    Field(i32),
}

/// Rules a Conquest map's economy must meet, which the checker enforces
/// and the recipes write to: a start's own seeps and vein lie within
/// `BASE_AREA` tiles of it and its natural expansion beyond that, out to
/// `EXPANSION_AREA`; a map has at least `MIN_BLOODSEEPS` Bloodseep sites
/// and `FRONTIER_SLOTS` frontier slots.
pub const BASE_AREA: i32 = 24;
pub const EXPANSION_AREA: i32 = 60;
pub const MIN_BLOODSEEPS: usize = 3;
pub const FRONTIER_SLOTS: std::ops::RangeInclusive<usize> = 4..=8;

/// The sides a Field map may have, in tiles.
pub const FIELD_TILES: std::ops::RangeInclusive<i32> = 64..=1024;

/// Field Battle maps this many tiles a side and more are the big ones:
/// their starts stand further in (see [`crate::plateaus`]) and the Wilds
/// have room for a middle band and spurs on them.
pub const BIG_MAP_TILES: i32 = 512;

impl SizeClass {
    pub fn tiles(self) -> i32 {
        match self {
            SizeClass::Duel => 128,
            SizeClass::Skirmish => 192,
            SizeClass::Warfield => 256,
            SizeClass::Grand => 320,
            SizeClass::Field(tiles) => tiles,
        }
    }

    pub fn players(self) -> std::ops::RangeInclusive<u8> {
        match self {
            SizeClass::Duel => 2..=2,
            SizeClass::Skirmish => 2..=3,
            SizeClass::Warfield => 4..=4,
            SizeClass::Grand => 5..=6,
            SizeClass::Field(_) => 2..=6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Symmetry {
    None,
    /// Turned half a circle about the centre; players 0 and 1 swap places.
    Rot2,
    /// Turned a quarter circle about the centre; players 0 to 3 follow each other round.
    Rot4,
}

impl Symmetry {
    pub fn copies(self) -> usize {
        match self {
            Symmetry::None => 1,
            Symmetry::Rot2 => 2,
            Symmetry::Rot4 => 4,
        }
    }
}

/// A point or tile as (x, y), x to the east and y to the south.
pub type Pos = (i32, i32);

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Shape {
    /// Everything within `r` tiles of a point.
    Disc { at: Pos, r: i32 },
    /// Everything within `half_width` tiles of the line from `a` to `b`.
    Line { a: Pos, b: Pos, half_width: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Mode {
    /// Pull the ground to the height.
    Set,
    /// Raise the ground to at least the height.
    Raise,
    /// Lower the ground to at most the height.
    Lower,
}

/// One stamp on the height field.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct TerrainOp {
    pub shape: Shape,
    pub mode: Mode,
    /// In sixteenths of a tile.
    pub height: i32,
    /// Tiles over which the stamp fades into the ground around it; 0 is a hard edge.
    #[serde(default)]
    pub edge: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum CoverKind {
    Grass,
    Wood,
    /// Clears cover again.
    Open,
    /// Woods too dense to march through: blocked to every regiment, as a
    /// cliff is, so a belt of it turns armies to its gaps. Woods to the
    /// simulation's eyes (it hides what stands behind it).
    Thicket,
}

/// What the ground looks like: the land a region of the map belongs to.
/// Drawn (the ground's colour, the minimap) and not a rule: what can be
/// walked, seen or hidden in comes from the heights and the cover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum Land {
    /// Open grassland, the ground everywhere until painted.
    #[default]
    Meadow,
    /// Old forest: dark loam under the trees.
    Forest,
    /// Wet low ground, mud and reeds.
    Fen,
    /// Raised rocky ground and heather, round the mountains.
    Highland,
    /// Pale, bare bone waste.
    Waste,
    /// A river's green valley.
    Valley,
}

impl Land {
    pub const ALL: [Land; 6] = [
        Land::Meadow,
        Land::Forest,
        Land::Fen,
        Land::Highland,
        Land::Waste,
        Land::Valley,
    ];

    /// The land a tile's byte stands for (Meadow for anything unknown).
    pub fn from_u8(b: u8) -> Land {
        Land::ALL.get(usize::from(b)).copied().unwrap_or_default()
    }

    /// The colour of open ground in this land, sRGB: what the renderer
    /// tints the grass with and the maps' pictures draw.
    pub fn colour(self) -> [u8; 3] {
        match self {
            Land::Meadow => [112, 150, 70],
            Land::Forest => [72, 98, 52],
            Land::Fen => [104, 112, 66],
            Land::Highland => [128, 120, 104],
            Land::Waste => [196, 184, 150],
            Land::Valley => [92, 156, 76],
        }
    }
}

/// Paint the land within a shape (the last paint wins).
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct LandOp {
    pub shape: Shape,
    pub land: Land,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct CoverOp {
    pub shape: Shape,
    pub kind: CoverKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Kind {
    /// A player's base centre.
    Start,
    GrubSeep,
    MarrowVein,
    Village,
    Town,
    Bazaar,
    CreepCamp,
    /// The Warfield boss.
    OldMouth,
    Shrine,
    /// The Titan's Heart.
    Heart,
    /// A place where a Tremor can open a new seep.
    Bloodseep,
    /// A slot the match seed fills with a frontier site.
    FrontierSlot,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Object {
    pub kind: Kind,
    pub at: Pos,
    /// The player whose area this belongs to (a start, its own seeps and
    /// vein, its natural expansion). Moved on for each rotated copy.
    #[serde(default)]
    pub owner: Option<u8>,
    /// Free text: which creeps live in a camp, what a Shrine is called.
    #[serde(default)]
    pub label: String,
    /// Placed once, not turned into copies: the Heart in the middle.
    #[serde(default)]
    pub once: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum SetPieceKind {
    /// A hill or ridge a defending line can hold.
    High,
    /// A shallow crossing.
    Ford,
}

/// A place the author says makes a good Field Battle ([07](../../docs/07-world-and-maps.md)).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetPiece {
    pub kind: SetPieceKind,
    pub at: Pos,
    pub label: String,
}

/// Rolling ground under the stamps: smooth integer noise, made symmetric
/// by averaging it over the map's rotations.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Noise {
    pub seed: u32,
    /// From the lowest to the highest ground it adds, in sixteenths of a tile.
    pub amplitude: i32,
    /// Tiles across one swell of the coarsest layer.
    pub scale: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MapDef {
    pub name: String,
    pub size: SizeClass,
    pub symmetry: Symmetry,
    pub players: u8,
    /// Ground height everywhere before any stamp, in sixteenths of a tile.
    pub base: i32,
    /// The water line, in sixteenths of a tile.
    pub water: i32,
    #[serde(default)]
    pub noise: Option<Noise>,
    #[serde(default)]
    pub terrain: Vec<TerrainOp>,
    #[serde(default)]
    pub cover: Vec<CoverOp>,
    /// The look of the ground, region by region; Meadow where unpainted.
    #[serde(default)]
    pub land: Vec<LandOp>,
    pub objects: Vec<Object>,
    #[serde(default)]
    pub set_pieces: Vec<SetPiece>,
}

impl MapDef {
    pub fn parse(text: &str) -> Result<MapDef, String> {
        let def: MapDef = ron::from_str(text).map_err(|e| e.to_string())?;
        match def.size {
            SizeClass::Field(tiles) if !FIELD_TILES.contains(&tiles) => Err(format!(
                "a Field map is {} to {} tiles a side, not {tiles}",
                FIELD_TILES.start(),
                FIELD_TILES.end()
            )),
            _ => Ok(def),
        }
    }

    /// The description as RON text, as an author would save it.
    pub fn to_ron(&self) -> String {
        let config = ron::ser::PrettyConfig::new().depth_limit(3);
        // Plain data with no maps or floats: it always serialises.
        ron::ser::to_string_pretty(self, config).unwrap_or_default()
    }

    /// Read and parse a map file; the error names the file.
    pub fn load(path: &std::path::Path) -> Result<MapDef, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        MapDef::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}
