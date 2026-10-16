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

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum SizeClass {
    /// 128 × 128 tiles, 2 players.
    Duel,
    /// 192 × 192 tiles, 2 to 3 players.
    Skirmish,
    /// 256 × 256 tiles, 4 players.
    Warfield,
}

impl SizeClass {
    pub fn tiles(self) -> i32 {
        match self {
            SizeClass::Duel => 128,
            SizeClass::Skirmish => 192,
            SizeClass::Warfield => 256,
        }
    }

    pub fn players(self) -> std::ops::RangeInclusive<u8> {
        match self {
            SizeClass::Duel => 2..=2,
            SizeClass::Skirmish => 2..=3,
            SizeClass::Warfield => 4..=4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
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

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum Shape {
    /// Everything within `r` tiles of a point.
    Disc { at: Pos, r: i32 },
    /// Everything within `half_width` tiles of the line from `a` to `b`.
    Line { a: Pos, b: Pos, half_width: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Mode {
    /// Pull the ground to the height.
    Set,
    /// Raise the ground to at least the height.
    Raise,
    /// Lower the ground to at most the height.
    Lower,
}

/// One stamp on the height field.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct TerrainOp {
    pub shape: Shape,
    pub mode: Mode,
    /// In sixteenths of a tile.
    pub height: i32,
    /// Tiles over which the stamp fades into the ground around it; 0 is a hard edge.
    #[serde(default)]
    pub edge: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum CoverKind {
    Grass,
    Wood,
    /// Clears cover again.
    Open,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct CoverOp {
    pub shape: Shape,
    pub kind: CoverKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum SetPieceKind {
    /// A hill or ridge a defending line can hold.
    High,
    /// A shallow crossing.
    Ford,
}

/// A place the author says makes a good Field Battle ([07](../../docs/07-world-and-maps.md)).
#[derive(Clone, Debug, Deserialize)]
pub struct SetPiece {
    pub kind: SetPieceKind,
    pub at: Pos,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
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
    pub terrain: Vec<TerrainOp>,
    #[serde(default)]
    pub cover: Vec<CoverOp>,
    pub objects: Vec<Object>,
    #[serde(default)]
    pub set_pieces: Vec<SetPiece>,
}

impl MapDef {
    pub fn parse(text: &str) -> Result<MapDef, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }

    /// Read and parse a map file; the error names the file.
    pub fn load(path: &std::path::Path) -> Result<MapDef, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        MapDef::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}
