//! Conquest data: every faction's buildings and trainable units, baked from
//! `data/rosters/*.ron` (docs/17-unit-rosters.md, docs/21-buildings.md).
//!
//! Both tables list the factions in [`Faction`] order and, within one,
//! entries in the order the roster file gives them, so an index is the same
//! on every machine with the same data. Buildings and units refer to each
//! other by index into these tables ([`NONE`] for none).

use bytemuck::{Pod, Zeroable};
use gc_fixed::Fx;

use crate::{Faction, NAME_BYTES, UnitRecord, name_of, validate_name, validate_stats};

/// Pack path of the building table.
pub const BUILDINGS_PATH: &str = "data/buildings";
/// Pack path of the Conquest unit table.
pub const CONQUEST_UNITS_PATH: &str = "data/conquest_units";
/// Bump whenever [`BuildingRecord`]'s layout or meaning changes.
pub const BUILDING_SCHEMA: u32 = 1;
/// Bump whenever [`ConquestUnitRecord`]'s layout or meaning changes (or
/// [`crate::UNIT_SCHEMA`], whose record it holds).
pub const CONQUEST_UNIT_SCHEMA: u32 = 1;
/// An index field that names nothing.
pub const NONE: u16 = u16::MAX;
/// Largest footprint side, in tiles (docs/21: buildings are 2×2 to 5×5,
/// walls one tile thick).
pub const MAX_FOOTPRINT: u8 = 5;
/// Highest main-building tier.
pub const MAX_TIER: u8 = 3;

/// What a building is for (docs/21-buildings.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    /// The main building and its tier upgrades.
    Main = 0,
    /// A cheaper hall for expansions.
    Outpost = 1,
    Housing = 2,
    Economy = 3,
    Production = 4,
    Tech = 5,
    Heroes = 6,
    Defence = 7,
    Wall = 8,
}

impl Role {
    pub const ALL: [Role; 9] = [
        Role::Main,
        Role::Outpost,
        Role::Housing,
        Role::Economy,
        Role::Production,
        Role::Tech,
        Role::Heroes,
        Role::Defence,
        Role::Wall,
    ];

    pub fn from_u8(v: u8) -> Option<Role> {
        Role::ALL.get(v as usize).copied()
    }
}

/// A building's armour type (docs/21, "Damage against buildings").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Armour {
    Structure = 0,
    Fortified = 1,
    Living = 2,
}

impl Armour {
    pub const ALL: [Armour; 3] = [Armour::Structure, Armour::Fortified, Armour::Living];

    pub fn from_u8(v: u8) -> Option<Armour> {
        Armour::ALL.get(v as usize).copied()
    }
}

/// Bits of [`BuildingRecord::flags`].
pub const BUILDING_DROP_OFF_GRUB: u8 = 1;
pub const BUILDING_DROP_OFF_MARROW: u8 = 2;
/// Choir: powers buildings around it.
pub const BUILDING_POWERS_FIELD: u8 = 4;
/// Its attack can hit flying units.
pub const BUILDING_ANTI_AIR: u8 = 8;
const BUILDING_FLAGS: u8 =
    BUILDING_DROP_OFF_GRUB | BUILDING_DROP_OFF_MARROW | BUILDING_POWERS_FIELD | BUILDING_ANTI_AIR;

/// One building. Times are simulation ticks at Standard tempo, lengths `Fx`
/// raw values, so the simulation reads them without floating point.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct BuildingRecord {
    /// UTF-8, zero-padded.
    pub name: [u8; NAME_BYTES],
    /// Tower range in tiles (`Fx` raw); zero for buildings that don't shoot.
    pub attack_range: i32,
    pub hp: u16,
    /// Ticks to build (or, for a tier upgrade, to upgrade).
    pub build_ticks: u16,
    pub cost_grub: u16,
    pub cost_marrow: u16,
    /// For a main building's tier 2 and 3: the building it upgrades, as an
    /// index into the building table; [`NONE`] otherwise.
    pub upgrades_from: u16,
    /// Damage per shot of a tower.
    pub attack_damage: u16,
    /// Ticks between a tower's volleys.
    pub attack_reload_ticks: u16,
    /// A [`Faction`] as `u8`.
    pub faction: u8,
    /// A [`Role`] as `u8`.
    pub role: u8,
    /// Main-building tier needed to place it (for an upgrade, the tier it
    /// gives).
    pub tier: u8,
    /// An [`Armour`] as `u8`.
    pub armour: u8,
    /// Footprint in tiles, across (x) and deep (y).
    pub width: u8,
    pub depth: u8,
    /// What it adds to the supply cap (for a main building's upgrade, the
    /// total for that tier).
    pub supply: u8,
    /// `BUILDING_*` bits.
    pub flags: u8,
    /// Shots in a tower's volley.
    pub attack_shots: u8,
    /// How many of these the faction starts Conquest with.
    pub start_count: u8,
}

impl BuildingRecord {
    pub fn name(&self) -> &str {
        name_of(&self.name)
    }

    pub fn faction(&self) -> Faction {
        // Validated when the table is loaded.
        Faction::from_u8(self.faction).unwrap_or(Faction::EmberClans)
    }

    pub fn role(&self) -> Role {
        Role::from_u8(self.role).unwrap_or(Role::Main)
    }

    pub fn armour(&self) -> Armour {
        Armour::from_u8(self.armour).unwrap_or(Armour::Structure)
    }

    pub fn attack_range(&self) -> Fx {
        Fx::from_raw(self.attack_range)
    }
}

/// A unit's role on the field (docs/17-unit-rosters.md); it also decides
/// how hard it hits buildings (docs/21).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Class {
    Worker = 0,
    Light = 1,
    Spear = 2,
    Heavy = 3,
    Ranged = 4,
    Cavalry = 5,
    Air = 6,
    Artillery = 7,
    Siege = 8,
    Support = 9,
    Monster = 10,
}

impl Class {
    pub const ALL: [Class; 11] = [
        Class::Worker,
        Class::Light,
        Class::Spear,
        Class::Heavy,
        Class::Ranged,
        Class::Cavalry,
        Class::Air,
        Class::Artillery,
        Class::Siege,
        Class::Support,
        Class::Monster,
    ];

    pub fn from_u8(v: u8) -> Option<Class> {
        Class::ALL.get(v as usize).copied()
    }
}

/// Bits of [`ConquestUnitRecord::flags`], one per roster unit flag.
pub const UNIT_FLYING: u16 = 1;
pub const UNIT_ANTI_AIR: u16 = 1 << 1;
pub const UNIT_CRYSTAL: u16 = 1 << 2;
pub const UNIT_UNDEAD: u16 = 1 << 3;
pub const UNIT_BURROWS: u16 = 1 << 4;
pub const UNIT_SIEGE_ONLY: u16 = 1 << 5;
pub const UNIT_FRIENDLY_FIRE: u16 = 1 << 6;
pub const UNIT_DIRECT_FIRE: u16 = 1 << 7;
pub const UNIT_DETECTOR: u16 = 1 << 8;
pub const UNIT_TERROR: u16 = 1 << 9;
const UNIT_FLAGS: u16 = (1 << 10) - 1;

/// One unit a Conquest player can field: its battle stats exactly as a
/// `data/units.ron` entry bakes, and how it is raised. Heroes are not in
/// this table.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct ConquestUnitRecord {
    pub stats: UnitRecord,
    pub cost_grub: u16,
    pub cost_marrow: u16,
    /// Ticks to train (for a Brood evolution, to cocoon).
    pub train_ticks: u16,
    /// The building that trains it, as an index into the building table;
    /// [`NONE`] for evolved and summoned units.
    pub trained_at: u16,
    /// The unit it evolves from (Brood), as an index into this table;
    /// [`NONE`] otherwise.
    pub evolves_from: u16,
    /// `UNIT_*` bits.
    pub flags: u16,
    /// 1 to 3; 0 for summoned units.
    pub tier: u8,
    /// A [`Class`] as `u8`.
    pub class: u8,
    pub supply: u8,
    /// How many of these the faction starts Conquest with.
    pub start_count: u8,
}

impl ConquestUnitRecord {
    pub fn name(&self) -> &str {
        self.stats.name()
    }

    pub fn faction(&self) -> Faction {
        self.stats.faction()
    }

    pub fn class(&self) -> Class {
        Class::from_u8(self.class).unwrap_or(Class::Light)
    }

    pub fn worker(&self) -> bool {
        self.class() == Class::Worker
    }
}

/// The rules every building record satisfies on its own; `count` is the
/// number of buildings, for the index fields. Shared by the baker and the
/// loader.
pub fn validate_building(b: &BuildingRecord, count: usize) -> Result<(), &'static str> {
    validate_name(&b.name)?;
    if Faction::from_u8(b.faction).is_none() {
        return Err("unknown faction");
    }
    if Role::from_u8(b.role).is_none() {
        return Err("unknown role");
    }
    if Armour::from_u8(b.armour).is_none() {
        return Err("unknown armour");
    }
    if b.tier == 0 || b.tier > MAX_TIER {
        return Err("tier is 1 to 3");
    }
    if b.hp == 0 {
        return Err("hit points must be above 0");
    }
    if b.build_ticks == 0 {
        return Err("build time must be above 0");
    }
    if b.width == 0 || b.depth == 0 || b.width > MAX_FOOTPRINT || b.depth > MAX_FOOTPRINT {
        return Err("footprint sides are 1 to 5 tiles");
    }
    if b.flags & !BUILDING_FLAGS != 0 {
        return Err("unknown building flags");
    }
    if b.upgrades_from != NONE && usize::from(b.upgrades_from) >= count {
        return Err("upgrades_from is past the end of the table");
    }
    if b.attack_range < 0 || b.attack_range > Fx::from_int(100).raw() {
        return Err("attack range must be 0 to 100 tiles");
    }
    let shoots = b.attack_range > 0;
    if shoots != (b.attack_damage > 0 && b.attack_reload_ticks > 0 && b.attack_shots > 0) {
        return Err("a tower needs range, damage, reload and shots, and nothing else has any");
    }
    Ok(())
}

/// What building `i`'s upgrade must be: a Main building of the same
/// faction, one tier below it (so a chain of upgrades can't loop). The
/// table has passed [`validate_building`].
pub fn validate_upgrade(table: &[BuildingRecord], i: usize) -> Result<(), &'static str> {
    let b = &table[i];
    if b.upgrades_from == NONE {
        return Ok(());
    }
    let from = &table[usize::from(b.upgrades_from)];
    let main = Role::Main as u8;
    if b.role != main || from.role != main {
        return Err("only a Main building upgrades, and only from a Main building");
    }
    if from.faction != b.faction || from.tier + 1 != b.tier {
        return Err("upgrades from a building of its own faction one tier below");
    }
    Ok(())
}

/// The rules every Conquest unit record satisfies on its own; `buildings`
/// and `units` are the two tables' lengths, for the index fields.
pub fn validate_conquest_unit(
    u: &ConquestUnitRecord,
    buildings: usize,
    units: usize,
) -> Result<(), &'static str> {
    validate_stats(&u.stats)?;
    if Class::from_u8(u.class).is_none() {
        return Err("unknown class");
    }
    if u.tier > MAX_TIER {
        return Err("tier is 0 (summoned) to 3");
    }
    if u.flags & !UNIT_FLAGS != 0 {
        return Err("unknown unit flags");
    }
    if u.trained_at != NONE && usize::from(u.trained_at) >= buildings {
        return Err("trained_at is past the end of the building table");
    }
    if u.evolves_from != NONE && usize::from(u.evolves_from) >= units {
        return Err("evolves_from is past the end of the unit table");
    }
    if u.trained_at != NONE && u.train_ticks == 0 {
        return Err("a trained unit needs a train time");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_layouts_are_stable() {
        // Changing these means bumping the schemas.
        assert_eq!(size_of::<BuildingRecord>(), 60);
        assert_eq!(align_of::<BuildingRecord>(), 4);
        assert_eq!(size_of::<ConquestUnitRecord>(), 92);
        assert_eq!(align_of::<ConquestUnitRecord>(), 4);
    }
}
