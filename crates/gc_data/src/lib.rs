//! Grubcraft game data as the game reads it: fixed-size records in the asset
//! pack, viewed in place. The human-edited source is RON under `data/`, which
//! `gc_bake` validates and compiles into these tables.
//!
//! Every player must run identical data, so the pack carries a BLAKE3 hash of
//! all data tables; the lobby (M2) compares it before a match starts.

#![no_std]

use core::fmt;

use bytemuck::{Pod, Zeroable};
use gc_assets::{Kind, Pack, PackError, TableError, read_table};
use gc_fixed::Fx;

/// Pack path of the unit table.
pub const UNITS_PATH: &str = "data/units";
/// Pack path of the data manifest (the data hash).
pub const MANIFEST_PATH: &str = "data/manifest";
/// Bump whenever [`UnitRecord`]'s layout or meaning changes.
pub const UNIT_SCHEMA: u32 = 1;
/// Longest unit name, in UTF-8 bytes.
pub const NAME_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Faction {
    EmberClans = 0,
    Brood = 1,
    Choir = 2,
    CarrionConcord = 3,
}

impl Faction {
    pub const ALL: [Faction; 4] = [
        Faction::EmberClans,
        Faction::Brood,
        Faction::Choir,
        Faction::CarrionConcord,
    ];

    pub fn from_u8(v: u8) -> Option<Faction> {
        Faction::ALL.get(v as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Faction::EmberClans => "Ember Clans",
            Faction::Brood => "Brood",
            Faction::Choir => "Choir",
            Faction::CarrionConcord => "Carrion Concord",
        }
    }
}

/// One regiment type. Lengths and speeds are `Fx` raw values (Q16.16), so the
/// simulation reads them without any floating point.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct UnitRecord {
    /// UTF-8, zero-padded.
    pub name: [u8; NAME_BYTES],
    /// Tiles per second.
    pub speed: i32,
    /// Width of the regiment's block in tiles.
    pub frontage: i32,
    /// Depth of the regiment's block in tiles.
    pub depth: i32,
    /// Soldiers in a full-strength regiment.
    pub models: u16,
    pub hp_per_model: u16,
    /// A [`Faction`] as `u8`.
    pub faction: u8,
    pub _pad: [u8; 3],
}

impl UnitRecord {
    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
        core::str::from_utf8(&self.name[..len]).unwrap_or("?")
    }

    pub fn speed(&self) -> Fx {
        Fx::from_raw(self.speed)
    }

    pub fn frontage(&self) -> Fx {
        Fx::from_raw(self.frontage)
    }

    pub fn depth(&self) -> Fx {
        Fx::from_raw(self.depth)
    }

    pub fn faction(&self) -> Faction {
        // Validated when the table is loaded.
        Faction::from_u8(self.faction).unwrap_or(Faction::EmberClans)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataError {
    Pack(PackError),
    Missing(&'static str),
    WrongKind(&'static str),
    Table(&'static str, TableError),
    BadManifest,
    /// A unit record failed validation; the index is its row.
    BadUnit(usize, &'static str),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Pack(e) => write!(f, "{e}"),
            DataError::Missing(path) => write!(f, "game data is missing {path}"),
            DataError::WrongKind(path) => write!(f, "{path} has the wrong kind"),
            DataError::Table(path, e) => write!(f, "{path}: {e}"),
            DataError::BadManifest => write!(f, "game data manifest is malformed"),
            DataError::BadUnit(i, why) => write!(f, "unit {i}: {why}"),
        }
    }
}

impl core::error::Error for DataError {}

impl From<PackError> for DataError {
    fn from(e: PackError) -> Self {
        DataError::Pack(e)
    }
}

/// All game data, borrowed from a pack.
#[derive(Clone, Copy, Debug)]
pub struct GameData<'a> {
    pub units: &'a [UnitRecord],
    /// BLAKE3 hash of every data table, as computed by the baker.
    pub hash: [u8; 32],
}

impl<'a> GameData<'a> {
    pub fn from_pack(pack: &Pack<'a>) -> Result<GameData<'a>, DataError> {
        let units_entry = pack
            .get_path(UNITS_PATH)
            .ok_or(DataError::Missing(UNITS_PATH))?;
        if units_entry.kind != Kind::Table {
            return Err(DataError::WrongKind(UNITS_PATH));
        }
        let units = read_table::<UnitRecord>(UNIT_SCHEMA, units_entry.bytes)
            .map_err(|e| DataError::Table(UNITS_PATH, e))?;
        for (i, u) in units.iter().enumerate() {
            validate_unit(u).map_err(|why| DataError::BadUnit(i, why))?;
        }

        let manifest = pack
            .get_path(MANIFEST_PATH)
            .ok_or(DataError::Missing(MANIFEST_PATH))?;
        let hash: [u8; 32] = manifest
            .bytes
            .try_into()
            .map_err(|_| DataError::BadManifest)?;
        Ok(GameData { units, hash })
    }

    pub fn unit_by_name(&self, name: &str) -> Option<(u16, &'a UnitRecord)> {
        self.units
            .iter()
            .enumerate()
            .find(|(_, u)| u.name() == name)
            .map(|(i, u)| (i as u16, u))
    }

    /// The first 8 bytes of the data hash as hex-friendly `u64`, for display.
    pub fn short_hash(&self) -> u64 {
        u64::from_be_bytes(self.hash[..8].try_into().unwrap())
    }
}

/// The rules every unit record must satisfy. Shared by the baker (which
/// reports them against the RON source) and the loader.
pub fn validate_unit(u: &UnitRecord) -> Result<(), &'static str> {
    let len = u.name.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
    if len == 0 {
        return Err("name is empty");
    }
    if core::str::from_utf8(&u.name[..len]).is_err() {
        return Err("name is not UTF-8");
    }
    if u.name[len..].iter().any(|&b| b != 0) {
        return Err("name has bytes after its terminator");
    }
    if Faction::from_u8(u.faction).is_none() {
        return Err("unknown faction");
    }
    if u.speed <= 0 || u.speed > Fx::from_int(20).raw() {
        return Err("speed must be above 0 and at most 20 tiles per second");
    }
    if u.frontage <= 0
        || u.depth <= 0
        || u.frontage > Fx::from_int(32).raw()
        || u.depth > Fx::from_int(32).raw()
    {
        return Err("frontage and depth must be above 0 and at most 32 tiles");
    }
    if u.models == 0 {
        return Err("a regiment needs at least one model");
    }
    if u.hp_per_model == 0 {
        return Err("hp per model must be above 0");
    }
    if u._pad != [0; 3] {
        return Err("padding must be zero");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_layout_is_stable() {
        // Changing this means bumping UNIT_SCHEMA.
        assert_eq!(size_of::<UnitRecord>(), 52);
        assert_eq!(align_of::<UnitRecord>(), 4);
    }
}
