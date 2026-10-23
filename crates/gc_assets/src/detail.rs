//! Shared detail textures: a small set of repeating surface textures (stone,
//! wood, cloth, chitin...) the game layers over the baked atlas of every
//! building and soldier, so close-up surfaces keep fine grain the atlas
//! itself is too coarse to hold (see `docs/31-surface-detail.md`).
//!
//! A detail pack holds one table, `detail/textures`: for each of the
//! [`CLASSES`] classes, [`LAYERS_PER_CLASS`] layers, each [`DETAIL_SIZE`]
//! square and BC7-compressed with its mips down to one block. Both layers
//! are linear:
//!
//! - [`DetailLayer::Albedo`]: rgb modulates the baked colour (0.5 leaves it
//!   as it is, the shader multiplies by twice the value); alpha is cavity
//!   (1 is open, lower darkens).
//! - [`DetailLayer::Relief`]: red and green are a tangent-space normal's x
//!   and y (0.5 is flat), blue is a roughness change (0.5 leaves it), alpha
//!   is height.
//!
//! A model's vertices say which class they wear and how finely it repeats
//! (see [`DetailTag`]), so a model costs no extra texture of its own.

use alloc::vec::Vec;

pub use crate::model::BLOCK_SIDE;
use crate::model::Block;
use crate::{Pack, TableError, read_table};

/// Bumped whenever the layout or meaning of the table changes.
pub const DETAIL_SCHEMA: u32 = 1;
pub const TEXTURES_PATH: &str = "detail/textures";

/// Side of a detail layer, in texels.
pub const DETAIL_SIZE: u32 = 256;
/// Mip levels per layer, down to a single block.
pub const DETAIL_MIPS: u32 = DETAIL_SIZE.ilog2() - BLOCK_SIDE.ilog2() + 1;
/// Blocks in one layer with all its mips.
pub const BLOCKS_PER_LAYER: usize = (4 * ((DETAIL_SIZE / BLOCK_SIDE) as usize).pow(2) - 1) / 3;
pub const LAYERS_PER_CLASS: usize = 2;
/// Classes in a pack: [`DetailClass`] 1 to 15 (0 is no detail).
pub const CLASSES: usize = 15;
/// Texture array layers a detail pack uploads.
pub const LAYERS: usize = CLASSES * LAYERS_PER_CLASS;

/// What a class's two layers hold, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailLayer {
    Albedo = 0,
    Relief = 1,
}

/// The surfaces a face can wear. The numbers are what a vertex stores (four
/// bits each); `tools/gc_artbake` and the surface table of the building
/// baker (`design/buildings/source/surfaces.py`) use the same ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DetailClass {
    Stone = 1,
    Brick = 2,
    Wood = 3,
    Planks = 4,
    Plaster = 5,
    Tile = 6,
    Cloth = 7,
    Leather = 8,
    Iron = 9,
    Bronze = 10,
    Bone = 11,
    Chitin = 12,
    Organic = 13,
    Crystal = 14,
    Earth = 15,
}

/// The classes' names, in number order.
const NAMES: [&str; CLASSES] = [
    "stone", "brick", "wood", "planks", "plaster", "tile", "cloth", "leather", "iron", "bronze",
    "bone", "chitin", "organic", "crystal", "earth",
];

impl DetailClass {
    pub const ALL: [DetailClass; CLASSES] = [
        DetailClass::Stone,
        DetailClass::Brick,
        DetailClass::Wood,
        DetailClass::Planks,
        DetailClass::Plaster,
        DetailClass::Tile,
        DetailClass::Cloth,
        DetailClass::Leather,
        DetailClass::Iron,
        DetailClass::Bronze,
        DetailClass::Bone,
        DetailClass::Chitin,
        DetailClass::Organic,
        DetailClass::Crystal,
        DetailClass::Earth,
    ];

    /// The name the class's source images and the baker's config use.
    pub fn name(self) -> &'static str {
        NAMES[self as usize - 1]
    }

    pub fn from_name(name: &str) -> Option<DetailClass> {
        DetailClass::ALL.into_iter().find(|c| c.name() == name)
    }

    /// The class a vertex's four bits name (0 is none).
    pub fn from_bits(bits: u8) -> Option<DetailClass> {
        DetailClass::ALL
            .get((bits as usize).checked_sub(1)?)
            .copied()
    }

    /// First texture array layer of this class.
    pub fn first_layer(self) -> usize {
        (self as usize - 1) * LAYERS_PER_CLASS
    }
}

/// Repeats across the whole texture atlas for each repeat code, 1 to 15: 2,
/// then half again more at each step (rounded to a tenth).
const TILES: [f32; 15] = [
    2.0, 3.0, 4.5, 6.8, 10.1, 15.2, 22.8, 34.2, 51.3, 76.9, 115.4, 173.1, 259.7, 389.6, 584.4,
];

/// How finely a face's detail repeats: the number of repeats across the
/// whole texture atlas, from a four-bit code. 0 means no detail; codes 1 to
/// 15 run from 2 repeats up by half again each step, to about 584.
pub fn tiles_of(code: u8) -> f32 {
    match code {
        0 => 0.0,
        c => TILES[usize::from(c.min(15)) - 1],
    }
}

/// The code whose repeats are closest (in ratio) to `tiles`; 0 for a
/// non-positive or non-finite number.
pub fn tiles_code(tiles: f32) -> u8 {
    if !tiles.is_finite() || tiles <= 0.0 {
        return 0;
    }
    let off = |t: f32| if t > tiles { t / tiles } else { tiles / t };
    (1..=15u8)
        .min_by(|&a, &b| off(tiles_of(a)).total_cmp(&off(tiles_of(b))))
        .unwrap_or(1)
}

/// What a vertex says about its surface: up to two classes, the share of the
/// second, and how finely the detail repeats. Stored in the two padding
/// bytes of a [`crate::model::ModelVertex`], so models baked before detail
/// existed (which hold 0 and 255) have none: see [`DetailTag::of`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DetailTag {
    pub class_a: u8,
    pub class_b: u8,
    /// Share of `class_b`, 0 to 15 (0 is all `class_a`).
    pub mix: u8,
    /// See [`tiles_of`].
    pub tiles: u8,
}

impl DetailTag {
    /// The padding byte of the normal: the two classes, four bits each.
    pub fn classes_byte(self) -> u8 {
        self.class_a & 15 | (self.class_b & 15) << 4
    }

    /// The padding byte of the colour: the mix and the repeats.
    pub fn params_byte(self) -> u8 {
        self.mix & 15 | (self.tiles & 15) << 4
    }

    pub fn from_bytes(classes: u8, params: u8) -> DetailTag {
        DetailTag {
            class_a: classes & 15,
            class_b: classes >> 4,
            mix: params & 15,
            tiles: params >> 4,
        }
    }

    /// Whether the surface wears any detail.
    pub fn is_none(self) -> bool {
        self.class_a == 0 || self.tiles == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailError {
    Missing,
    Table(TableError),
    /// The texture table is not [`CLASSES`] whole classes.
    BadTextures,
}

impl core::fmt::Display for DetailError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DetailError::Missing => write!(f, "detail pack has no {TEXTURES_PATH}"),
            DetailError::Table(e) => write!(f, "{TEXTURES_PATH}: {e}"),
            DetailError::BadTextures => write!(f, "detail pack is not {CLASSES} whole classes"),
        }
    }
}

impl core::error::Error for DetailError {}

/// The detail textures of a pack, borrowed in place.
#[derive(Clone, Copy, Debug)]
pub struct Detail<'a> {
    /// [`BLOCKS_PER_LAYER`] blocks per layer, mip by mip, layer after layer
    /// ([`LAYERS`] of them).
    pub textures: &'a [Block],
}

impl<'a> Detail<'a> {
    pub fn from_pack(pack: &Pack<'a>) -> Result<Detail<'a>, DetailError> {
        let entry = pack.get_path(TEXTURES_PATH).ok_or(DetailError::Missing)?;
        let textures: &[Block] =
            read_table(DETAIL_SCHEMA, entry.bytes).map_err(DetailError::Table)?;
        if textures.len() != LAYERS * BLOCKS_PER_LAYER {
            return Err(DetailError::BadTextures);
        }
        Ok(Detail { textures })
    }
}

/// The blocks of a detail pack, for writing one.
#[derive(Default)]
pub struct DetailTables {
    pub textures: Vec<Block>,
}

impl DetailTables {
    pub fn write(&self, pack: &mut crate::PackWriter) {
        pack.add(
            TEXTURES_PATH,
            crate::Kind::Table,
            &crate::table_bytes(DETAIL_SCHEMA, &self.textures),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackWriter, aligned_copy};

    #[test]
    fn a_layer_holds_every_mip() {
        let sum: usize = (0..DETAIL_MIPS)
            .map(|i| ((DETAIL_SIZE >> i) / BLOCK_SIDE).pow(2) as usize)
            .sum();
        assert_eq!(BLOCKS_PER_LAYER, sum);
        assert_eq!(DETAIL_SIZE >> (DETAIL_MIPS - 1), BLOCK_SIDE);
    }

    #[test]
    fn classes_are_numbered_from_one_in_order() {
        for (k, c) in DetailClass::ALL.into_iter().enumerate() {
            assert_eq!(c as usize, k + 1);
            assert_eq!(DetailClass::from_bits(c as u8), Some(c));
            assert_eq!(DetailClass::from_name(c.name()), Some(c));
            assert_eq!(c.first_layer(), k * LAYERS_PER_CLASS);
        }
        assert_eq!(DetailClass::from_bits(0), None);
        assert_eq!(DetailClass::from_bits(16), None);
    }

    #[test]
    fn repeat_codes_round_trip() {
        assert_eq!(tiles_of(0), 0.0);
        assert_eq!(tiles_of(1), 2.0);
        assert!(tiles_of(15) > 500.0 && tiles_of(15) < 650.0);
        for code in 1..=15 {
            assert_eq!(tiles_code(tiles_of(code)), code);
        }
        assert_eq!(tiles_code(0.0), 0);
        assert_eq!(tiles_code(f32::NAN), 0);
        assert_eq!(tiles_code(1.0e6), 15);
        assert_eq!(tiles_code(0.1), 1);
    }

    #[test]
    fn a_tag_survives_its_two_bytes() {
        let tag = DetailTag {
            class_a: 3,
            class_b: 12,
            mix: 7,
            tiles: 9,
        };
        assert_eq!(
            DetailTag::from_bytes(tag.classes_byte(), tag.params_byte()),
            tag
        );
        // Models baked before detail existed: zero and 255.
        assert!(DetailTag::from_bytes(0, 255).is_none());
        assert!(!tag.is_none());
    }

    fn read(tables: &DetailTables) -> Result<usize, DetailError> {
        let mut w = PackWriter::new();
        tables.write(&mut w);
        let bytes = w.finish();
        let store = aligned_copy(&bytes);
        let view = &bytemuck::cast_slice::<u128, u8>(&store)[..bytes.len()];
        Detail::from_pack(&Pack::parse(view).unwrap()).map(|d| d.textures.len())
    }

    #[test]
    fn a_detail_pack_reads_back_and_is_checked() {
        let whole = DetailTables {
            textures: alloc::vec![[3; 16]; LAYERS * BLOCKS_PER_LAYER],
        };
        assert_eq!(read(&whole), Ok(LAYERS * BLOCKS_PER_LAYER));
        let short = DetailTables {
            textures: alloc::vec![[3; 16]; BLOCKS_PER_LAYER],
        };
        assert_eq!(read(&short), Err(DetailError::BadTextures));
    }
}
