//! Bakes the faction rosters (`data/rosters/*.ron`) into the building,
//! Conquest unit and faction power tables (`gc_data::conquest`): the
//! factions in `Faction` order, and within one the entries in file order.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gc_data::conquest::{self as cq, BuildingRecord, ConquestUnitRecord, PowerRecord};
use gc_data::{Faction, UnitRecord, WEAPON_DISCIPLINE};

use crate::rosters::{self, FactionDef, Roster};
use crate::{BakeError, invalid, name_bytes, to_fx, to_ticks, unit_record};

/// The roster files, relative to the data directory, in `Faction` order, with
/// the faction each must name.
pub(crate) const ROSTER_FILES: [(&str, FactionDef); 4] = [
    ("rosters/ember_clans.ron", FactionDef::EmberClans),
    ("rosters/brood.ron", FactionDef::Brood),
    ("rosters/choir.ron", FactionDef::Choir),
    ("rosters/carrion_concord.ron", FactionDef::CarrionConcord),
];

/// The passive ability that gives a unit the discipline trait
/// (`WEAPON_DISCIPLINE`).
const DISCIPLINE_ABILITY: &str = "Formation Discipline";

/// Where each name stands in a table, within one faction.
type Index<'a> = BTreeMap<&'a str, u16>;

/// The tables the rosters bake into.
pub(crate) struct Tables {
    pub buildings: Vec<BuildingRecord>,
    pub units: Vec<ConquestUnitRecord>,
    pub powers: Vec<PowerRecord>,
}

/// The building, Conquest unit and faction power tables for the four
/// rosters. `units` is the baked `units.ron`: a roster unit of the same
/// name takes its points.
pub(crate) fn bake_rosters(data_dir: &Path, units: &[UnitRecord]) -> Result<Tables, BakeError> {
    let mut loaded = Vec::new();
    for (path, faction) in ROSTER_FILES {
        let file = data_dir.join(path);
        let text = std::fs::read_to_string(&file).map_err(|e| BakeError::Read(file.clone(), e))?;
        let roster: Roster = ron::from_str(&text).map_err(|e| BakeError::Parse(file.clone(), e))?;
        if roster.faction != faction {
            return Err(invalid(
                &file,
                "faction",
                format!("must be {faction:?}, as the file's place in the pack says"),
            ));
        }
        loaded.push((file, roster));
    }

    // Number every building and unit first: entries refer to each other, in
    // either order, by their place in the whole table.
    let mut building_at: Vec<Index> = Vec::new();
    let mut unit_at: Vec<Index> = Vec::new();
    // The summoned units alone, for what a power may summon.
    let mut summoned_at: Vec<Index> = Vec::new();
    let (mut buildings, mut conquest_units) = (0, 0);
    let (mut building_names, mut unit_names) = (BTreeSet::new(), BTreeSet::new());
    for (file, r) in &loaded {
        let mut here = Index::new();
        for b in &r.buildings {
            if !building_names.insert(b.name.as_str()) {
                return Err(invalid(file, &b.name, "defined twice"));
            }
            here.insert(&b.name, index(file, &b.name, buildings)?);
            buildings += 1;
        }
        building_at.push(here);
        let (mut here, mut summoned) = (Index::new(), Index::new());
        for u in &r.units {
            if !unit_names.insert(u.stats.name.as_str()) {
                return Err(invalid(file, &u.stats.name, "defined twice"));
            }
            let at = index(file, &u.stats.name, conquest_units)?;
            here.insert(&u.stats.name, at);
            if matches!(u.from, rosters::From::Summoned(_)) {
                summoned.insert(&u.stats.name, at);
            }
            conquest_units += 1;
        }
        unit_at.push(here);
        summoned_at.push(summoned);
    }

    let mut building_records = Vec::with_capacity(buildings);
    let mut unit_records = Vec::with_capacity(conquest_units);
    let mut power_records = Vec::new();
    for (i, (file, r)) in loaded.iter().enumerate() {
        let faction = Faction::from(r.faction);
        for n in &r.starting_buildings {
            if !building_at[i].contains_key(n.as_str()) {
                return Err(invalid(
                    file,
                    "starting_buildings",
                    format!("no building {n}"),
                ));
            }
        }
        for (n, _) in &r.starting_units {
            if !unit_at[i].contains_key(n.as_str()) {
                return Err(invalid(file, "starting_units", format!("no unit {n}")));
            }
        }
        for b in &r.buildings {
            let record = building_record(file, faction, b, &building_at[i], &r.starting_buildings)?;
            cq::validate_building(&record, buildings).map_err(|why| invalid(file, &b.name, why))?;
            building_records.push(record);
        }
        for u in &r.units {
            let record = UnitContext {
                file,
                faction,
                units,
                building_at: &building_at[i],
                unit_at: &unit_at[i],
                starting: &r.starting_units,
            }
            .unit_record(u)?;
            cq::validate_conquest_unit(&record, buildings, conquest_units)
                .map_err(|why| invalid(file, &u.stats.name, why))?;
            unit_records.push(record);
        }
        for a in &r.meter.abilities {
            let record = power_record(
                file,
                faction,
                &r.meter.name,
                a,
                (&building_at[i], &summoned_at[i]),
            )?;
            cq::validate_power(&record, buildings, &unit_records)
                .map_err(|why| invalid(file, &a.name, why))?;
            power_records.push(record);
        }
    }
    for (i, b) in building_records.iter().enumerate() {
        cq::validate_upgrade(&building_records, i).map_err(|why| {
            let file = &loaded[usize::from(b.faction)].0;
            invalid(file, b.name(), why)
        })?;
    }
    Ok(Tables {
        buildings: building_records,
        units: unit_records,
        powers: power_records,
    })
}

/// A faction meter's ability. One that says nothing is `built` is not
/// built yet: it is listed, but the game doesn't offer it.
fn power_record(
    file: &Path,
    faction: Faction,
    meter: &str,
    a: &rosters::Ability,
    (building_at, summoned_at): (&Index, &Index),
) -> Result<PowerRecord, BakeError> {
    let bad = |why: &str| invalid(file, &a.name, why.to_string());
    let ticks = |seconds: f64| -> Result<u32, BakeError> {
        if seconds == 0.0 {
            Ok(0)
        } else {
            to_ticks(seconds).map(u32::from).map_err(bad)
        }
    };
    use rosters::Built;
    let (effect, arg) = match &a.built {
        None => (cq::POWER_NOT_BUILT, 0),
        Some(Built::Haste(p)) => (cq::POWER_HASTE, u16::from(*p)),
        Some(Built::Shift(n)) => (
            cq::POWER_SHIFT,
            *building_at
                .get(n.as_str())
                .ok_or_else(|| bad(&format!("it shifts to no building {n}")))?,
        ),
        Some(Built::Grant(grub)) => (cq::POWER_GRANT, *grub),
        Some(Built::March(p)) => (cq::POWER_MARCH, u16::from(*p)),
        Some(Built::Steadfast) => (cq::POWER_STEADFAST, whole_tiles(a.radius).map_err(bad)?),
        Some(Built::Summon(n)) => (
            cq::POWER_SUMMON,
            *summoned_at
                .get(n.as_str())
                .ok_or_else(|| bad(&format!("{n} is not a summoned unit")))?,
        ),
    };
    let meter = match cq::METER_NAMES.iter().position(|&n| n == meter) {
        _ if effect == cq::POWER_NOT_BUILT => cq::METER_NOT_BUILT,
        Some(m) if m > 0 => m as u8,
        _ => return Err(bad("its meter is not built yet")),
    };
    let lasts = cq::power_lasts(effect);
    Ok(PowerRecord {
        name: name_bytes(&a.name).map_err(|why| bad(&why))?,
        cooldown_ticks: ticks(a.cooldown)?,
        duration_ticks: if lasts { ticks(a.duration)? } else { 0 },
        arg,
        faction: faction as u8,
        tier: a.tier,
        cost: u8::try_from(a.cost).map_err(|_| bad("costs more than 255"))?,
        meter,
        effect,
        pad: 0,
    })
}

/// A radius in whole tiles, 1 or more.
fn whole_tiles(radius: f64) -> Result<u16, &'static str> {
    if radius >= 1.0 && radius.fract() == 0.0 && radius <= f64::from(u16::MAX) {
        Ok(radius as u16)
    } else {
        Err("its radius is a whole number of tiles, 1 or more")
    }
}

/// `n` as a table index, which must leave `NONE` free.
fn index(file: &Path, entry: &str, n: usize) -> Result<u16, BakeError> {
    u16::try_from(n)
        .ok()
        .filter(|&i| i != cq::NONE)
        .ok_or_else(|| invalid(file, entry, "too many entries"))
}

/// How many times `name` is listed, as a `start_count`.
fn count<'a>(
    file: &Path,
    entry: &str,
    listed: impl Iterator<Item = (&'a str, u8)>,
    name: &str,
) -> Result<u8, BakeError> {
    let total: u32 = listed
        .filter(|&(n, _)| n == name)
        .map(|(_, c)| u32::from(c))
        .sum();
    u8::try_from(total).map_err(|_| invalid(file, entry, "starts with more than 255"))
}

fn building_record(
    file: &Path,
    faction: Faction,
    b: &rosters::Building,
    building_at: &Index,
    starting: &[String],
) -> Result<BuildingRecord, BakeError> {
    let bad = |why: String| invalid(file, &b.name, why);
    let (attack_range, attack_damage, attack_reload_ticks, attack_shots) = match &b.attack {
        Some(a) => (
            to_fx(a.range).map_err(|why| bad(format!("attack range {why}")))?,
            a.damage,
            to_ticks(a.reload).map_err(|why| bad(format!("attack reload {why}")))?,
            a.shots,
        ),
        None => (0, 0, 0, 0),
    };
    let upgrades_from = match &b.upgrades_from {
        Some(from) => *building_at
            .get(from.as_str())
            .ok_or_else(|| bad(format!("upgrades unknown building {from}")))?,
        None => cq::NONE,
    };
    Ok(BuildingRecord {
        name: name_bytes(&b.name).map_err(bad)?,
        attack_range,
        hp: b.hp,
        build_ticks: to_ticks(b.build_time).map_err(|why| bad(format!("build_time {why}")))?,
        cost_grub: b.cost.grub,
        cost_marrow: b.cost.marrow,
        upgrades_from,
        attack_damage,
        attack_reload_ticks,
        faction: faction as u8,
        role: building_role(b.role) as u8,
        tier: b.tier,
        armour: building_armour(b.armour) as u8,
        width: b.footprint.0,
        depth: b.footprint.1,
        supply: b.supply,
        flags: b.flags.iter().fold(0, |bits, &f| bits | building_flag(f)),
        attack_shots,
        start_count: count(
            file,
            &b.name,
            starting.iter().map(|n| (n.as_str(), 1)),
            &b.name,
        )?,
        field_tiles: b.field,
        tunes: match &b.tunes {
            Some(t) => match t.node {
                rosters::Node::Seeps => cq::TUNES_SEEPS,
                rosters::Node::Veins => cq::TUNES_VEINS,
            },
            None => cq::TUNES_NOTHING,
        },
        tune_tiles: b.tunes.as_ref().map_or(0, |t| t.tiles),
        link_tiles: b.links,
        twist: match b.twist {
            rosters::Twist::None => cq::TWIST_NONE,
            rosters::Twist::Stones => cq::TWIST_STONES,
            rosters::Twist::Mends => cq::TWIST_MENDS,
            rosters::Twist::Sings => cq::TWIST_SINGS,
            rosters::Twist::Thorns => cq::TWIST_THORNS,
        },
        pad: [0; 2],
    })
}

/// What one roster's units need to refer to.
struct UnitContext<'a> {
    file: &'a Path,
    faction: Faction,
    /// The baked `units.ron`.
    units: &'a [UnitRecord],
    building_at: &'a Index<'a>,
    unit_at: &'a Index<'a>,
    starting: &'a [(String, u8)],
}

impl UnitContext<'_> {
    fn unit_record(&self, u: &rosters::Unit) -> Result<ConquestUnitRecord, BakeError> {
        let name = &u.stats.name;
        let bad = |why: String| invalid(self.file, name, why);
        if Faction::from(u.stats.faction) != self.faction {
            return Err(bad("stats.faction differs from the roster's".into()));
        }
        let points = self
            .units
            .iter()
            .find(|r| r.name() == name.as_str())
            .map_or(0, |r| r.points);
        let train_ticks = || to_ticks(u.train_time).map_err(|why| bad(format!("train_time {why}")));
        let (trained_at, evolves_from, train_ticks) = match &u.from {
            rosters::From::Building(b) => (
                *self
                    .building_at
                    .get(b.as_str())
                    .ok_or_else(|| bad(format!("trained at unknown building {b}")))?,
                cq::NONE,
                train_ticks()?,
            ),
            rosters::From::Evolves(parent) => (
                cq::NONE,
                *self
                    .unit_at
                    .get(parent.as_str())
                    .ok_or_else(|| bad(format!("evolves from unknown unit {parent}")))?,
                train_ticks()?,
            ),
            rosters::From::Summoned(_) => (cq::NONE, cq::NONE, 0),
        };
        let mut stats = unit_record(self.file, &u.stats, points)?;
        // A unit that drills in close order has the trait.
        if u.abilities.iter().any(|a| a.name == DISCIPLINE_ABILITY) {
            stats.weapons |= WEAPON_DISCIPLINE;
        }
        Ok(ConquestUnitRecord {
            stats,
            cost_grub: u.cost.grub,
            cost_marrow: u.cost.marrow,
            train_ticks,
            trained_at,
            evolves_from,
            flags: u.flags.iter().fold(0, |bits, &f| bits | unit_flag(f)),
            trade_base: u.trade.as_ref().map_or(0, |t| t.base),
            trade_tenths: u.trade.as_ref().map_or(0, |t| t.per_tile_tenths),
            tier: u.tier,
            class: unit_class(u.class) as u8,
            supply: u.supply,
            start_count: count(
                self.file,
                name,
                self.starting.iter().map(|(n, c)| (n.as_str(), *c)),
                name,
            )?,
        })
    }
}

fn building_role(r: rosters::Role) -> cq::Role {
    match r {
        rosters::Role::Main => cq::Role::Main,
        rosters::Role::Outpost => cq::Role::Outpost,
        rosters::Role::Housing => cq::Role::Housing,
        rosters::Role::Economy => cq::Role::Economy,
        rosters::Role::Production => cq::Role::Production,
        rosters::Role::Tech => cq::Role::Tech,
        rosters::Role::Heroes => cq::Role::Heroes,
        rosters::Role::Defence => cq::Role::Defence,
        rosters::Role::Wall => cq::Role::Wall,
        rosters::Role::Bridge => cq::Role::Bridge,
        rosters::Role::Stakes => cq::Role::Stakes,
        rosters::Role::Barricade => cq::Role::Barricade,
    }
}

fn building_armour(a: rosters::Armour) -> cq::Armour {
    match a {
        rosters::Armour::Structure => cq::Armour::Structure,
        rosters::Armour::Fortified => cq::Armour::Fortified,
        rosters::Armour::Living => cq::Armour::Living,
    }
}

fn building_flag(f: rosters::BuildingFlag) -> u16 {
    match f {
        rosters::BuildingFlag::DropOffGrub => cq::BUILDING_DROP_OFF_GRUB,
        rosters::BuildingFlag::DropOffMarrow => cq::BUILDING_DROP_OFF_MARROW,
        rosters::BuildingFlag::PowersField => cq::BUILDING_POWERS_FIELD,
        rosters::BuildingFlag::NeedsPower => cq::BUILDING_NEEDS_POWER,
        rosters::BuildingFlag::AntiAir => cq::BUILDING_ANTI_AIR,
        rosters::BuildingFlag::Larvae => cq::BUILDING_LARVAE,
        rosters::BuildingFlag::Veins => cq::BUILDING_VEINS,
        rosters::BuildingFlag::Honor => cq::BUILDING_HONOR,
        rosters::BuildingFlag::TradePost => cq::BUILDING_TRADE_POST,
        rosters::BuildingFlag::Toll => cq::BUILDING_TOLL,
        rosters::BuildingFlag::Silences => cq::BUILDING_SILENCES,
        rosters::BuildingFlag::Gate => cq::BUILDING_GATE,
        rosters::BuildingFlag::Line => cq::BUILDING_LINE,
        rosters::BuildingFlag::Pass => cq::BUILDING_PASS,
        rosters::BuildingFlag::Grown => cq::BUILDING_GROWN,
    }
}

fn unit_class(c: rosters::Class) -> cq::Class {
    match c {
        rosters::Class::Worker => cq::Class::Worker,
        rosters::Class::Light => cq::Class::Light,
        rosters::Class::Spear => cq::Class::Spear,
        rosters::Class::Heavy => cq::Class::Heavy,
        rosters::Class::Ranged => cq::Class::Ranged,
        rosters::Class::Cavalry => cq::Class::Cavalry,
        rosters::Class::Air => cq::Class::Air,
        rosters::Class::Artillery => cq::Class::Artillery,
        rosters::Class::Siege => cq::Class::Siege,
        rosters::Class::Support => cq::Class::Support,
        rosters::Class::Monster => cq::Class::Monster,
    }
}

fn unit_flag(f: rosters::Flag) -> u16 {
    match f {
        rosters::Flag::Flying => cq::UNIT_FLYING,
        rosters::Flag::AntiAir => cq::UNIT_ANTI_AIR,
        rosters::Flag::Crystal => cq::UNIT_CRYSTAL,
        rosters::Flag::Undead => cq::UNIT_UNDEAD,
        rosters::Flag::Burrows => cq::UNIT_BURROWS,
        rosters::Flag::BuildsOnly => cq::UNIT_BUILDS_ONLY,
        rosters::Flag::SiegeOnly => cq::UNIT_SIEGE_ONLY,
        rosters::Flag::FriendlyFire => cq::UNIT_FRIENDLY_FIRE,
        rosters::Flag::DirectFire => cq::UNIT_DIRECT_FIRE,
        rosters::Flag::Detector => cq::UNIT_DETECTOR,
        rosters::Flag::Terror => cq::UNIT_TERROR,
        rosters::Flag::Hatches => cq::UNIT_HATCHES,
        rosters::Flag::Consumed => cq::UNIT_CONSUMED,
    }
}
