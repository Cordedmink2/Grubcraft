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
/// Pack path of the faction powers table (the faction meters' abilities).
pub const POWERS_PATH: &str = "data/powers";
/// Bump whenever [`BuildingRecord`]'s layout or meaning changes.
pub const BUILDING_SCHEMA: u32 = 8;
/// Bump whenever [`ConquestUnitRecord`]'s layout or meaning changes (or
/// [`crate::UNIT_SCHEMA`], whose record it holds).
pub const CONQUEST_UNIT_SCHEMA: u32 = 5;
/// Bump whenever [`PowerRecord`]'s layout or meaning changes.
pub const POWER_SCHEMA: u32 = 3;
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
pub const BUILDING_DROP_OFF_GRUB: u16 = 1;
pub const BUILDING_DROP_OFF_MARROW: u16 = 2;
/// Choir: powers buildings around it.
pub const BUILDING_POWERS_FIELD: u16 = 4;
/// Its attack can hit flying units.
pub const BUILDING_ANTI_AIR: u16 = 8;
/// Brood: holds larvae and grows them (schema 2).
pub const BUILDING_LARVAE: u16 = 16;
/// Brood: spreads root-veins (schema 3).
pub const BUILDING_VEINS: u16 = 32;
/// Choir: works only inside a field of power (schema 4).
pub const BUILDING_NEEDS_POWER: u16 = 64;
/// Clans: earns its owner Honor while it stands (schema 6).
pub const BUILDING_HONOR: u16 = 128;
/// Concord: a trade post its owner's caravans can run to, at half rate
/// (schema 7; the field is 16 bits wide from then on).
pub const BUILDING_TRADE_POST: u16 = 256;
/// Concord: its tower earns its owner Grub for the damage it deals
/// (schema 8).
pub const BUILDING_TOLL: u16 = 512;
/// Choir: its tower tolls a bell that silences enemy powers near it
/// (schema 8).
pub const BUILDING_SILENCES: u16 = 1024;
/// A gate: a wall its owner's side walks through (schema 8).
pub const BUILDING_GATE: u16 = 2048;
/// Every flag there is; a record with any other bit set is refused.
const BUILDING_FLAGS: u16 = (BUILDING_GATE << 1) - 1;

/// [`BuildingRecord::tunes`]: the nodes a building tunes (schema 4).
pub const TUNES_NOTHING: u8 = 0;
pub const TUNES_SEEPS: u8 = 1;
pub const TUNES_VEINS: u8 = 2;
/// The furthest a field, tuning or link reaches, in tiles.
pub const MAX_REACH_TILES: u8 = 30;

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
    /// `BUILDING_*` bits (16 bits wide from schema 7).
    pub flags: u16,
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
    /// Shots in a tower's volley.
    pub attack_shots: u8,
    /// How many of these the faction starts Conquest with.
    pub start_count: u8,
    /// How far its field of power or its root-veins reach from its edge,
    /// in tiles; 0 for a building with neither (schema 4).
    pub field_tiles: u8,
    /// A `TUNES_*`: the nodes it works with no workers, and how far from
    /// its edge, in tiles (0 for none).
    pub tunes: u8,
    pub tune_tiles: u8,
    /// How far apart, centre to centre in tiles, two such buildings of
    /// one owner link (and three mutually linked form a chord); 0 for a
    /// building that doesn't link (schema 5).
    pub link_tiles: u8,
    /// Always 0 (the record is padded to a multiple of 4 bytes).
    pub pad: [u8; 3],
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
/// Brood: hatches from a larva (schema 2).
pub const UNIT_HATCHES: u16 = 1 << 10;
/// Brood: a worker used up by the building it starts (schema 2).
pub const UNIT_CONSUMED: u16 = 1 << 11;
/// Choir: a worker that builds and mends but does not gather (schema 3).
pub const UNIT_BUILDS_ONLY: u16 = 1 << 12;
const UNIT_FLAGS: u16 = (1 << 13) - 1;

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
    /// Concord caravans (schema 5): what a delivery pays at a full-rate
    /// trade post, in Grub, and what it adds for each whole tile of route,
    /// in tenths of a Grub (1.2 is 12); both 0 for a unit that doesn't
    /// trade.
    pub trade_base: u16,
    pub trade_tenths: u16,
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

    /// Whether it shuttles cargo between a main building and a trade post
    /// (a Concord Caravan).
    pub fn trades(&self) -> bool {
        self.trade_base > 0
    }
}

/// Whether a power of `effect` lasts a while (its duration): a haste, a
/// march, a steadfast power or a summons.
pub fn power_lasts(effect: u8) -> bool {
    matches!(
        effect,
        POWER_HASTE | POWER_MARCH | POWER_STEADFAST | POWER_SUMMON
    )
}

/// [`PowerRecord::meter`]: the faction meter a power is paid from.
pub const METER_NOT_BUILT: u8 = 0;
pub const METER_RESONANCE: u8 = 1;
pub const METER_HONOR: u8 = 2;
/// Each `METER_*`'s name, as the rosters and the screen give it.
pub const METER_NAMES: [&str; 3] = ["", "Resonance", "Honor"];
/// [`PowerRecord::effect`]: what a power does, and what its
/// [`PowerRecord::arg`] is.
pub const POWER_NOT_BUILT: u8 = 0;
/// A building or regiment of the caster's works, reloads and strikes `arg`
/// percent faster for the duration.
pub const POWER_HASTE: u8 = 1;
/// The caster's chosen regiments step to one of their buildings of type
/// `arg` (an index into the building table).
pub const POWER_SHIFT: u8 = 2;
/// The caster gains `arg` Grub at once.
pub const POWER_GRANT: u8 = 3;
/// The caster's chosen regiments march `arg` percent faster for the
/// duration.
pub const POWER_MARCH: u8 = 4;
/// The caster's regiments within `arg` tiles of the chosen point can't
/// rout for the duration.
pub const POWER_STEADFAST: u8 = 5;
/// A regiment of unit type `arg` (an index into the Conquest unit table)
/// takes the field beside one of the caster's finished Shrines for the
/// duration.
pub const POWER_SUMMON: u8 = 6;
/// The most percent faster a haste or a march makes anything.
pub const MAX_POWER_PERCENT: u16 = 100;
/// The widest a steadfast power reaches, in tiles.
pub const MAX_POWER_TILES: u16 = 64;
/// The longest a power's cooldown or effect lasts, in ticks (10 minutes).
pub const MAX_POWER_TICKS: u32 = 10 * 60 * 20;

/// One ability a faction meter pays for (docs/17-unit-rosters.md, the
/// meter tables), in [`Faction`] order and then the roster's. Times are
/// simulation ticks.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct PowerRecord {
    /// UTF-8, zero-padded.
    pub name: [u8; NAME_BYTES],
    pub cooldown_ticks: u32,
    /// How long its effect lasts; 0 for an instant one.
    pub duration_ticks: u32,
    /// What the effect works with (see the `POWER_*`); 0 when not built.
    pub arg: u16,
    /// A [`Faction`] as `u8`.
    pub faction: u8,
    /// Main-building tier needed to use it.
    pub tier: u8,
    /// Meter points it costs.
    pub cost: u8,
    /// A `METER_*`.
    pub meter: u8,
    /// A `POWER_*`.
    pub effect: u8,
    /// Always 0.
    pub pad: u8,
}

impl PowerRecord {
    pub fn name(&self) -> &str {
        name_of(&self.name)
    }

    pub fn faction(&self) -> Faction {
        // Validated when the table is loaded.
        Faction::from_u8(self.faction).unwrap_or(Faction::EmberClans)
    }

    /// Whether the game has built what it does yet.
    pub fn built(&self) -> bool {
        self.effect != POWER_NOT_BUILT
    }

    /// The name of the meter it is paid from ("" when not built).
    pub fn meter_name(&self) -> &'static str {
        METER_NAMES
            .get(usize::from(self.meter))
            .copied()
            .unwrap_or("")
    }
}

/// The rules every power record satisfies on its own; `buildings` is the
/// number of buildings and `units` the Conquest unit table, for the records
/// that name one (a summons names a unit of the power's own faction that
/// nothing trains).
pub fn validate_power(
    p: &PowerRecord,
    buildings: usize,
    units: &[ConquestUnitRecord],
) -> Result<(), &'static str> {
    validate_name(&p.name)?;
    if Faction::from_u8(p.faction).is_none() {
        return Err("unknown faction");
    }
    if p.tier == 0 || p.tier > MAX_TIER {
        return Err("tier is 1 to 3");
    }
    if p.meter > METER_HONOR || p.effect > POWER_SUMMON || p.pad != 0 {
        return Err("unknown meter or effect");
    }
    if p.built() && p.meter == METER_NOT_BUILT {
        return Err("a built power is paid from a built meter");
    }
    if p.cooldown_ticks > MAX_POWER_TICKS || p.duration_ticks > MAX_POWER_TICKS {
        return Err("cooldown and duration are at most 10 minutes");
    }
    if power_lasts(p.effect) != (p.duration_ticks > 0) {
        return Err(
            "a haste, a march, a steadfast power or a summons lasts a while, and nothing else does",
        );
    }
    let arg_ok = match p.effect {
        POWER_NOT_BUILT => p.arg == 0,
        POWER_HASTE | POWER_MARCH => (1..=MAX_POWER_PERCENT).contains(&p.arg),
        POWER_SHIFT => usize::from(p.arg) < buildings,
        POWER_GRANT => p.arg > 0,
        POWER_SUMMON => units.get(usize::from(p.arg)).is_some_and(|u| {
            u.trained_at == NONE && u.evolves_from == NONE && u.stats.faction == p.faction
        }),
        _ => (1..=MAX_POWER_TILES).contains(&p.arg),
    };
    if !arg_ok {
        return Err("its argument is out of range for what it does");
    }
    Ok(())
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
    if b.flags & !BUILDING_FLAGS != 0 || b.pad != [0; 3] {
        return Err("unknown building flags");
    }
    let fields = b.flags & (BUILDING_POWERS_FIELD | BUILDING_VEINS) != 0;
    if fields != (b.field_tiles > 0) || b.field_tiles > MAX_REACH_TILES {
        return Err(
            "a building that powers a field or spreads veins reaches 1 to 30 tiles, and nothing else has a field",
        );
    }
    if b.flags & BUILDING_NEEDS_POWER != 0 && fields {
        return Err("a building that needs power doesn't give any or spread veins");
    }
    if b.tunes > TUNES_VEINS
        || (b.tunes != TUNES_NOTHING) != (b.tune_tiles > 0)
        || b.tune_tiles > MAX_REACH_TILES
    {
        return Err("a building tunes seeps or veins 1 to 30 tiles off, or nothing");
    }
    if b.link_tiles > MAX_REACH_TILES {
        return Err("a building links 1 to 30 tiles off, or not at all");
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
    if (from.width, from.depth) != (b.width, b.depth) {
        return Err("upgrades from a building of the same footprint");
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
    if (u.trade_base > 0) != (u.trade_tenths > 0) {
        return Err("a trader has a base pay and a pay per tile, and nothing else has either");
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
        assert_eq!(size_of::<BuildingRecord>(), 68);
        assert_eq!(align_of::<BuildingRecord>(), 4);
        assert_eq!(size_of::<ConquestUnitRecord>(), 96);
        assert_eq!(align_of::<ConquestUnitRecord>(), 4);
        assert_eq!(size_of::<PowerRecord>(), 48);
        assert_eq!(align_of::<PowerRecord>(), 4);
    }

    /// A summons of `Faction::EmberClans` naming unit 1 of `units`.
    fn summons() -> PowerRecord {
        let mut p = PowerRecord::zeroed();
        p.name[0] = b'R';
        p.faction = Faction::EmberClans as u8;
        p.tier = 3;
        p.cost = 80;
        p.meter = METER_HONOR;
        p.effect = POWER_SUMMON;
        p.duration_ticks = 1200;
        p.arg = 1;
        p
    }

    /// Two units of `faction`: unit 0 is trained at building 0, unit 1
    /// nowhere, and evolves from nothing.
    fn units(faction: Faction) -> [ConquestUnitRecord; 2] {
        let mut u = ConquestUnitRecord::zeroed();
        u.stats.faction = faction as u8;
        u.trained_at = NONE;
        u.evolves_from = NONE;
        [ConquestUnitRecord { trained_at: 0, ..u }, u]
    }

    #[test]
    fn a_summons_names_a_unit_nothing_trains() {
        let p = summons();
        assert_eq!(validate_power(&p, 2, &units(Faction::EmberClans)), Ok(()));
        // Out of the table.
        let p_far = PowerRecord { arg: 2, ..p };
        assert!(validate_power(&p_far, 2, &units(Faction::EmberClans)).is_err());
        // A trained unit.
        let p_trained = PowerRecord { arg: 0, ..p };
        assert!(validate_power(&p_trained, 2, &units(Faction::EmberClans)).is_err());
        // An evolved unit.
        let mut evolved = units(Faction::EmberClans);
        evolved[1].evolves_from = 0;
        assert!(validate_power(&p, 2, &evolved).is_err());
    }

    #[test]
    fn a_summons_names_a_unit_of_its_own_faction() {
        let p = summons();
        assert!(validate_power(&p, 2, &units(Faction::Brood)).is_err());
        let theirs = PowerRecord {
            faction: Faction::Brood as u8,
            ..p
        };
        assert_eq!(validate_power(&theirs, 2, &units(Faction::Brood)), Ok(()));
    }
}
