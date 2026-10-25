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
use serde::{Deserialize, Serialize};

pub mod conquest;

pub use conquest::{
    BUILDINGS_PATH, BuildingRecord, CONQUEST_UNITS_PATH, ConquestUnitRecord, POWERS_PATH,
    PowerRecord,
};

/// Pack path of the unit table.
pub const UNITS_PATH: &str = "data/units";
/// Pack path of the data manifest (the data hash).
pub const MANIFEST_PATH: &str = "data/manifest";
/// Bump whenever [`UnitRecord`]'s layout or meaning changes.
pub const UNIT_SCHEMA: u32 = 7;
/// Longest unit name, in UTF-8 bytes.
pub const NAME_BYTES: usize = 32;
/// Most soldiers in one regiment. Must equal `gc_sim::formation::MAX_MODELS`
/// (the game checks this at compile time).
pub const MAX_MODELS: u16 = 32;
/// Angle steps in a full turn (`gc_fixed::Angle`).
pub const ANGLE_STEPS_PER_TURN: u32 = 65_536;
/// Bits of [`UnitRecord::weapons`]: spears and pikes that fight from two
/// ranks, big swinging weapons that need room, shields, and the Clans'
/// formation discipline (more melee defence standing still in close order).
pub const WEAPON_REACH: u8 = 1;
pub const WEAPON_SWING_ROOM: u8 = 2;
pub const WEAPON_SHIELD: u8 = 4;
pub const WEAPON_DISCIPLINE: u8 = 8;
/// Heaviest [`UnitRecord::mass`]: 0 light, 1 medium, 2 heavy.
pub const MAX_MASS: u8 = 2;

/// The playable factions. This is the game's only faction type: the lobby,
/// the computer players, replays and the unit table all use it, and their
/// files and wire formats store [`Faction::index`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    /// The faction stored as `v`, or `None` for a number no faction has.
    pub fn from_u8(v: u8) -> Option<Faction> {
        Faction::ALL.get(v as usize).copied()
    }

    /// Its place in [`Faction::ALL`], which is also its stored number.
    pub fn index(self) -> usize {
        self as usize
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

/// One regiment type. Lengths and speeds are `Fx` raw values (Q16.16) and
/// turn rates are angle steps, so the simulation reads them without any
/// floating point.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct UnitRecord {
    /// UTF-8, zero-padded.
    pub name: [u8; NAME_BYTES],
    /// Marching speed, tiles per second.
    pub speed: i32,
    /// Distance between neighbouring soldiers in a rank, in tiles.
    pub file_spacing: i32,
    /// Distance between ranks, in tiles.
    pub rank_spacing: i32,
    /// Turning speed in angle steps per second ([`ANGLE_STEPS_PER_TURN`] per turn).
    pub turn_rate: u32,
    /// Soldiers in a full-strength regiment.
    pub models: u16,
    pub hp_per_model: u16,
    /// Soldiers per rank when the regiment is raised.
    pub files: u16,
    /// A [`Faction`] as `u8`.
    pub faction: u8,
    /// How heavy the soldiers are: 0 light (swarms), 1 medium, 2 heavy
    /// (cavalry, monsters).
    pub mass: u8,
    /// Shooting range in tiles (`Fx` raw); zero for regiments that don't shoot.
    pub range: i32,
    /// Hit points a landed melee blow takes.
    pub melee_damage: u16,
    /// Hit points a landed arrow takes.
    pub missile_damage: u16,
    /// Simulation ticks between volleys.
    pub reload_ticks: u16,
    /// Melee skill and defence, 0 to 100.
    pub melee_attack: u8,
    pub melee_defence: u8,
    /// Extra melee damage, percent, when a charge hits home.
    pub charge_bonus: u8,
    /// Percent of arrows that hit a block of normal density.
    pub accuracy: u8,
    /// Morale when fresh, 1 to 100; 0 means the regiment never routs.
    pub morale: u8,
    /// [`WEAPON_REACH`], [`WEAPON_SWING_ROOM`], [`WEAPON_SHIELD`] and
    /// [`WEAPON_DISCIPLINE`] bits.
    pub weapons: u8,
    /// What the regiment costs from a Field Battle army's points budget.
    pub points: u16,
    /// Zero; keeps the record free of padding.
    pub reserved: u16,
}

impl UnitRecord {
    pub fn name(&self) -> &str {
        name_of(&self.name)
    }

    pub fn speed(&self) -> Fx {
        Fx::from_raw(self.speed)
    }

    pub fn file_spacing(&self) -> Fx {
        Fx::from_raw(self.file_spacing)
    }

    pub fn rank_spacing(&self) -> Fx {
        Fx::from_raw(self.rank_spacing)
    }

    pub fn range(&self) -> Fx {
        Fx::from_raw(self.range)
    }

    pub fn faction(&self) -> Faction {
        // Validated when the table is loaded.
        Faction::from_u8(self.faction).unwrap_or(Faction::EmberClans)
    }

    pub fn reach(&self) -> bool {
        self.weapons & WEAPON_REACH != 0
    }

    pub fn swing_room(&self) -> bool {
        self.weapons & WEAPON_SWING_ROOM != 0
    }

    pub fn shield(&self) -> bool {
        self.weapons & WEAPON_SHIELD != 0
    }

    pub fn discipline(&self) -> bool {
        self.weapons & WEAPON_DISCIPLINE != 0
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
    /// A building record failed validation; the index is its row.
    BadBuilding(usize, &'static str),
    /// A Conquest unit record failed validation; the index is its row.
    BadConquestUnit(usize, &'static str),
    /// A faction power record failed validation; the index is its row.
    BadPower(usize, &'static str),
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
            DataError::BadBuilding(i, why) => write!(f, "building {i}: {why}"),
            DataError::BadConquestUnit(i, why) => write!(f, "Conquest unit {i}: {why}"),
            DataError::BadPower(i, why) => write!(f, "faction power {i}: {why}"),
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
    /// The Field Battle regiment types.
    pub units: &'a [UnitRecord],
    /// Every faction's buildings, for Conquest (see [`conquest`]).
    pub buildings: &'a [BuildingRecord],
    /// Every faction's trainable units, for Conquest (see [`conquest`]).
    pub conquest_units: &'a [ConquestUnitRecord],
    /// Every faction meter's abilities (see [`conquest`]).
    pub powers: &'a [PowerRecord],
    /// BLAKE3 hash of every data table, as computed by the baker.
    pub hash: [u8; 32],
}

impl<'a> GameData<'a> {
    pub fn from_pack(pack: &Pack<'a>) -> Result<GameData<'a>, DataError> {
        let units = table::<UnitRecord>(pack, UNITS_PATH, UNIT_SCHEMA)?;
        for (i, u) in units.iter().enumerate() {
            validate_unit(u).map_err(|why| DataError::BadUnit(i, why))?;
        }
        let buildings = table::<BuildingRecord>(pack, BUILDINGS_PATH, conquest::BUILDING_SCHEMA)?;
        for (i, b) in buildings.iter().enumerate() {
            conquest::validate_building(b, buildings.len())
                .map_err(|why| DataError::BadBuilding(i, why))?;
        }
        for i in 0..buildings.len() {
            conquest::validate_upgrade(buildings, i)
                .map_err(|why| DataError::BadBuilding(i, why))?;
        }
        let conquest_units =
            table::<ConquestUnitRecord>(pack, CONQUEST_UNITS_PATH, conquest::CONQUEST_UNIT_SCHEMA)?;
        for (i, u) in conquest_units.iter().enumerate() {
            conquest::validate_conquest_unit(u, buildings.len(), conquest_units.len())
                .map_err(|why| DataError::BadConquestUnit(i, why))?;
        }
        for (i, b) in buildings.iter().enumerate() {
            if b.trap == conquest::TRAP_NEST && usize::from(b.trap_unit) >= conquest_units.len() {
                return Err(DataError::BadBuilding(i, "its nest names no unit"));
            }
        }
        let powers = table::<PowerRecord>(pack, POWERS_PATH, conquest::POWER_SCHEMA)?;
        for (i, p) in powers.iter().enumerate() {
            conquest::validate_power(p, buildings.len(), conquest_units)
                .map_err(|why| DataError::BadPower(i, why))?;
        }

        let manifest = pack
            .get_path(MANIFEST_PATH)
            .ok_or(DataError::Missing(MANIFEST_PATH))?;
        let hash: [u8; 32] = manifest
            .bytes
            .try_into()
            .map_err(|_| DataError::BadManifest)?;
        Ok(GameData {
            units,
            buildings,
            conquest_units,
            powers,
            hash,
        })
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

/// The table at `path` in `pack`, of records written with `schema`.
fn table<'a, T: Pod>(
    pack: &Pack<'a>,
    path: &'static str,
    schema: u32,
) -> Result<&'a [T], DataError> {
    let entry = pack.get_path(path).ok_or(DataError::Missing(path))?;
    if entry.kind != Kind::Table {
        return Err(DataError::WrongKind(path));
    }
    read_table::<T>(schema, entry.bytes).map_err(|e| DataError::Table(path, e))
}

/// A record's name: the bytes before its terminator.
pub(crate) fn name_of(bytes: &[u8; NAME_BYTES]) -> &str {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
    core::str::from_utf8(&bytes[..len]).unwrap_or("?")
}

/// A record's name is UTF-8, not empty, and zero after its terminator.
pub(crate) fn validate_name(bytes: &[u8; NAME_BYTES]) -> Result<(), &'static str> {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
    if len == 0 {
        return Err("name is empty");
    }
    if core::str::from_utf8(&bytes[..len]).is_err() {
        return Err("name is not UTF-8");
    }
    if bytes[len..].iter().any(|&b| b != 0) {
        return Err("name has bytes after its terminator");
    }
    Ok(())
}

/// The rules every unit record must satisfy. Shared by the baker (which
/// reports them against the RON source) and the loader.
pub fn validate_unit(u: &UnitRecord) -> Result<(), &'static str> {
    validate_stats(u)?;
    if u.points == 0 {
        return Err("points must be above 0");
    }
    Ok(())
}

/// [`validate_unit`] but for the Field Battle points, which a Conquest
/// unit (`ConquestUnitRecord::stats`) need not have.
pub fn validate_stats(u: &UnitRecord) -> Result<(), &'static str> {
    validate_name(&u.name)?;
    if Faction::from_u8(u.faction).is_none() {
        return Err("unknown faction");
    }
    if u.speed <= 0 || u.speed > Fx::from_int(20).raw() {
        return Err("speed must be above 0 and at most 20 tiles per second");
    }
    let spacing = 1..=Fx::from_int(4).raw();
    if !spacing.contains(&u.file_spacing) || !spacing.contains(&u.rank_spacing) {
        return Err("file and rank spacing must be above 0 and at most 4 tiles");
    }
    if u.turn_rate == 0 || u.turn_rate > 2 * ANGLE_STEPS_PER_TURN {
        return Err("turn rate must be above 0 and at most 720 degrees per second");
    }
    if u.models == 0 || u.models > MAX_MODELS {
        return Err("a regiment needs between 1 and 32 models");
    }
    if u.files == 0 || u.files > u.models {
        return Err("files must be at least 1 and at most the number of models");
    }
    if u.hp_per_model == 0 {
        return Err("hp per model must be above 0");
    }
    if u.melee_attack > 100 || u.melee_defence > 100 || u.accuracy > 100 {
        return Err("melee attack, melee defence and accuracy are 0 to 100");
    }
    if u.melee_damage == 0 {
        return Err("melee damage must be above 0");
    }
    if u.range < 0 || u.range > Fx::from_int(100).raw() {
        return Err("range must be 0 to 100 tiles");
    }
    if u.range > 0 && (u.missile_damage == 0 || u.reload_ticks == 0 || u.accuracy == 0) {
        return Err("a regiment with a range needs missile damage, reload and accuracy");
    }
    if u.morale > 100 {
        return Err("morale is 0 (never routs) to 100");
    }
    if u.mass > MAX_MASS {
        return Err("mass is light, medium or heavy");
    }
    if u.weapons & !(WEAPON_REACH | WEAPON_SWING_ROOM | WEAPON_SHIELD | WEAPON_DISCIPLINE) != 0 {
        return Err("unknown weapon flags");
    }
    if u.reserved != 0 {
        return Err("reserved bytes must be zero");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_layout_is_stable() {
        // Changing this means bumping UNIT_SCHEMA.
        assert_eq!(size_of::<UnitRecord>(), 76);
        assert_eq!(align_of::<UnitRecord>(), 4);
    }

    #[test]
    fn faction_numbers_round_trip() {
        // Files, replays and the wire store these numbers.
        for (i, f) in Faction::ALL.into_iter().enumerate() {
            assert_eq!(f.index(), i);
            assert_eq!(Faction::from_u8(i as u8), Some(f));
        }
        assert_eq!(Faction::from_u8(Faction::ALL.len() as u8), None);
    }
}
