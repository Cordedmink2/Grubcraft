//! The Grubcraft baker. Reads human-edited sources (RON game data today; art
//! and audio later), validates them, and writes the asset pack.
//!
//! Baking is deterministic: the same sources produce byte-identical packs on
//! every machine, which is what lets players compare data hashes.

use std::fmt;
use std::path::{Path, PathBuf};

use gc_assets::{Kind, PackWriter, table_bytes};
use gc_data::conquest::{BUILDING_SCHEMA, CONQUEST_UNIT_SCHEMA};
use gc_data::{
    ANGLE_STEPS_PER_TURN, BUILDINGS_PATH, CONQUEST_UNITS_PATH, Faction, MANIFEST_PATH, NAME_BYTES,
    UNIT_SCHEMA, UNITS_PATH, UnitRecord, WEAPON_REACH, WEAPON_SHIELD, WEAPON_SWING_ROOM,
};
use gc_fixed::Fx;
use rosters::{FactionDef, Stats};

mod conquest;
pub mod rosters;

/// The data files the baker reads, relative to the data directory. Build
/// scripts use this list to know when to re-bake.
pub const DATA_FILES: &[&str] = &[
    "units.ron",
    "rosters/ember_clans.ron",
    "rosters/brood.ron",
    "rosters/choir.ron",
    "rosters/carrion_concord.ron",
];

#[derive(Debug)]
pub enum BakeError {
    Read(PathBuf, std::io::Error),
    Parse(PathBuf, ron::error::SpannedError),
    /// A definition broke a rule; names the file, the entry and why.
    Invalid {
        file: PathBuf,
        entry: String,
        why: String,
    },
}

impl fmt::Display for BakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BakeError::Read(p, e) => write!(f, "{}: {e}", p.display()),
            BakeError::Parse(p, e) => write!(f, "{}:{e}", p.display()),
            BakeError::Invalid { file, entry, why } => {
                write!(f, "{}: {entry}: {why}", file.display())
            }
        }
    }
}

impl std::error::Error for BakeError {}

impl From<FactionDef> for Faction {
    fn from(f: FactionDef) -> Faction {
        match f {
            FactionDef::EmberClans => Faction::EmberClans,
            FactionDef::Brood => Faction::Brood,
            FactionDef::Choir => Faction::Choir,
            FactionDef::CarrionConcord => Faction::CarrionConcord,
        }
    }
}

fn invalid(file: &Path, entry: &str, why: impl Into<String>) -> BakeError {
    BakeError::Invalid {
        file: file.into(),
        entry: entry.into(),
        why: why.into(),
    }
}

/// The result of a bake.
pub struct Baked {
    pub pack: Vec<u8>,
    /// BLAKE3 of every data table.
    pub data_hash: [u8; 32],
    pub units: usize,
    pub buildings: usize,
    pub conquest_units: usize,
}

/// Bake everything under `data_dir` into a pack.
pub fn bake(data_dir: &Path) -> Result<Baked, BakeError> {
    let units_file = data_dir.join("units.ron");
    let units = bake_units(&units_file)?;
    let (buildings, conquest_units) = conquest::bake_rosters(data_dir, &units)?;
    let units_table = table_bytes(UNIT_SCHEMA, &units);
    let buildings_table = table_bytes(BUILDING_SCHEMA, &buildings);
    let conquest_table = table_bytes(CONQUEST_UNIT_SCHEMA, &conquest_units);

    let data_hash = data_hash(&[
        (UNITS_PATH, &units_table),
        (BUILDINGS_PATH, &buildings_table),
        (CONQUEST_UNITS_PATH, &conquest_table),
    ]);

    let mut pack = PackWriter::new();
    pack.add(UNITS_PATH, Kind::Table, &units_table);
    pack.add(BUILDINGS_PATH, Kind::Table, &buildings_table);
    pack.add(CONQUEST_UNITS_PATH, Kind::Table, &conquest_table);
    pack.add(MANIFEST_PATH, Kind::Blob, &data_hash);
    Ok(Baked {
        pack: pack.finish(),
        data_hash,
        units: units.len(),
        buildings: buildings.len(),
        conquest_units: conquest_units.len(),
    })
}

/// BLAKE3 over every data table, each prefixed by its path and length so
/// tables can't be confused with one another.
fn data_hash(tables: &[(&str, &[u8])]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for (path, bytes) in tables {
        hasher.update(&(path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    *hasher.finalize().as_bytes()
}

fn bake_units(file: &Path) -> Result<Vec<UnitRecord>, BakeError> {
    let text = std::fs::read_to_string(file).map_err(|e| BakeError::Read(file.into(), e))?;
    let defs: Vec<Stats> = ron::from_str(&text).map_err(|e| BakeError::Parse(file.into(), e))?;

    let mut records = Vec::with_capacity(defs.len());
    for def in &defs {
        if records
            .iter()
            .any(|r: &UnitRecord| r.name() == def.name.as_str())
        {
            return Err(invalid(file, &def.name, "defined twice"));
        }
        let record = unit_record(file, def, def.points)?;
        gc_data::validate_unit(&record).map_err(|why| invalid(file, &def.name, why))?;
        records.push(record);
    }
    if records.is_empty() {
        return Err(invalid(file, "(file)", "defines no units"));
    }
    if records.len() > u16::MAX as usize {
        return Err(invalid(file, "(file)", "too many units"));
    }
    Ok(records)
}

/// A name as the zero-padded bytes of a record.
fn name_bytes(name: &str) -> Result<[u8; NAME_BYTES], String> {
    if name.len() > NAME_BYTES || name.contains('\0') {
        return Err(format!("names are at most {NAME_BYTES} bytes, with no NUL"));
    }
    let mut bytes = [0u8; NAME_BYTES];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    Ok(bytes)
}

/// One stats block as the record the simulation reads: a `units.ron` entry,
/// or the `stats` of a roster unit. `points` is its Field Battle cost (0 for
/// a roster unit with none).
fn unit_record(file: &Path, def: &Stats, points: u16) -> Result<UnitRecord, BakeError> {
    let bad = |why: String| invalid(file, &def.name, why);
    let record = UnitRecord {
        name: name_bytes(&def.name).map_err(bad)?,
        speed: to_fx(def.speed).map_err(|why| bad(format!("speed {why}")))?,
        file_spacing: to_fx(def.file_spacing).map_err(|why| bad(format!("file_spacing {why}")))?,
        rank_spacing: to_fx(def.rank_spacing).map_err(|why| bad(format!("rank_spacing {why}")))?,
        turn_rate: to_angle_rate(def.turn_rate).map_err(|why| bad(format!("turn_rate {why}")))?,
        models: def.models,
        hp_per_model: def.hp_per_model,
        files: def.files,
        faction: Faction::from(def.faction) as u8,
        mass: def.mass as u8,
        range: match &def.ranged {
            Some(r) => to_fx(r.range).map_err(|why| bad(format!("range {why}")))?,
            None => 0,
        },
        melee_damage: def.melee_damage,
        missile_damage: def.ranged.as_ref().map_or(0, |r| r.damage),
        reload_ticks: match &def.ranged {
            Some(r) => to_ticks(r.reload).map_err(|why| bad(format!("reload {why}")))?,
            None => 0,
        },
        melee_attack: def.melee_attack,
        melee_defence: def.melee_defence,
        charge_bonus: def.charge_bonus,
        accuracy: def.ranged.as_ref().map_or(0, |r| r.accuracy),
        morale: match (def.morale, def.never_routs) {
            (m @ 1..=100, false) => m,
            (0, true) => 0,
            _ => {
                return Err(bad(
                    "needs either morale (1 to 100) or never_routs: true".into()
                ));
            }
        },
        weapons: (u8::from(def.reach) * WEAPON_REACH)
            | (u8::from(def.swing_room) * WEAPON_SWING_ROOM)
            | (u8::from(def.shield) * WEAPON_SHIELD),
        points,
        reserved: 0,
    };
    gc_data::validate_stats(&record).map_err(|why| bad(why.into()))?;
    Ok(record)
}

/// Convert a decimal from the source file to `Fx` raw. Decimal parsing and
/// scaling by 2^16 are exact or correctly rounded on every platform, so this
/// is deterministic; values must also be exactly representable to 1/65536 of
/// a tile within rounding, which every sensible gameplay number is.
fn to_fx(v: f64) -> Result<i32, &'static str> {
    if !v.is_finite() {
        return Err("must be a finite number");
    }
    let raw = (v * 65536.0).round();
    if raw < f64::from(i32::MIN) || raw > f64::from(i32::MAX) {
        return Err("is out of range");
    }
    let raw = raw as i32;
    debug_assert_eq!(Fx::from_raw(raw).raw(), raw);
    Ok(raw)
}

/// Convert seconds to simulation ticks (20 per second), rounded.
fn to_ticks(seconds: f64) -> Result<u16, &'static str> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("must be a positive number of seconds");
    }
    let ticks = (seconds * 20.0).round();
    if ticks > f64::from(u16::MAX) {
        return Err("is too long");
    }
    Ok(ticks as u16)
}

/// Convert degrees per second to angle steps per second. Like [`to_fx`], the
/// scaling is correctly rounded, so every platform gets the same integer.
fn to_angle_rate(degrees: f64) -> Result<u32, &'static str> {
    if !degrees.is_finite() || degrees <= 0.0 {
        return Err("must be a positive number of degrees per second");
    }
    let steps = (degrees * f64::from(ANGLE_STEPS_PER_TURN) / 360.0).round();
    if steps > f64::from(u32::MAX) {
        return Err("is out of range");
    }
    Ok(steps as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_assets::Pack;
    use gc_data::GameData;

    /// A fresh data directory per call: tests run in parallel, and sharing a
    /// directory let one test read a file another was rewriting.
    fn temp_data(units: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gc_bake_test_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("rosters")).unwrap();
        std::fs::write(dir.join("units.ron"), units).unwrap();
        for (path, faction) in conquest::ROSTER_FILES {
            std::fs::write(dir.join(path), small_roster(&format!("{faction:?}"))).unwrap();
        }
        dir
    }

    /// A faction's roster with a hall and a worker, named for the faction so
    /// no two rosters share a name.
    fn small_roster(faction: &str) -> String {
        format!(
            r#"(faction: {faction}, meter: (name: "Meter", max: 1, abilities: []),
            starting_buildings: ["{faction} Hall"], starting_units: [("{faction} Hand", 2)],
            buildings: [(name: "{faction} Hall", role: Main, tier: 1, cost: (grub: 100, marrow: 0),
                build_time: 30.0, hp: 1000, armour: Fortified, footprint: (4, 4), supply: 12,
                flags: [DropOffGrub], effect: "Main building.")],
            units: [(tier: 1, class: Worker, cost: (grub: 50, marrow: 0), supply: 1, train_time: 12.0,
                from: Building("{faction} Hall"), abilities: [],
                stats: (name: "{faction} Hand", faction: {faction}, models: 1, hp_per_model: 20,
                    files: 1, speed: 3.0, turn_rate: 180.0, file_spacing: 1.0, rank_spacing: 1.0,
                    melee_attack: 5, melee_defence: 5, melee_damage: 2, charge_bonus: 0, morale: 20))],
            heroes: [])"#
        )
    }

    /// Copy a pack into 16-byte-aligned storage, as the game's loaders do.
    fn aligned(bytes: &[u8]) -> Vec<u128> {
        let mut v = vec![0u128; bytes.len().div_ceil(16)];
        bytemuck::cast_slice_mut::<u128, u8>(&mut v)[..bytes.len()].copy_from_slice(bytes);
        v
    }

    const ONE: &str = r#"[(name: "Test Spears", faction: EmberClans, models: 24,
        hp_per_model: 10, files: 8, speed: 3.0, turn_rate: 60.0, file_spacing: 0.75,
        rank_spacing: 0.85, melee_attack: 30, melee_defence: 45, melee_damage: 3,
        charge_bonus: 20, points: 100, morale: 50)]"#;

    #[test]
    fn bakes_and_reads_back() {
        let baked = bake(&temp_data(ONE)).unwrap();
        let store = aligned(&baked.pack);
        let bytes = &bytemuck::cast_slice::<u128, u8>(&store)[..baked.pack.len()];
        let pack = Pack::parse(bytes).unwrap();
        let data = GameData::from_pack(&pack).unwrap();
        assert_eq!(data.hash, baked.data_hash);
        assert_eq!(data.units.len(), 1);
        let u = &data.units[0];
        assert_eq!(u.name(), "Test Spears");
        assert_eq!(u.faction(), Faction::EmberClans);
        assert_eq!(u.speed(), Fx::from_int(3));
        assert_eq!(u.file_spacing(), Fx::from_ratio(3, 4));
        assert_eq!(u.files, 8);
        // 60 degrees per second is a sixth of a turn: 10922.67 steps.
        assert_eq!(u.turn_rate, 10_923);
    }

    /// Baking must give the same bytes on every platform, or Mac and Windows
    /// players would see different data hashes. CI runs this on all three.
    #[test]
    fn fixture_hash_is_the_same_everywhere() {
        let baked = bake(&temp_data(ONE)).unwrap();
        let hex: String = baked.data_hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "4378277b5779a4ab8496f9a82fa41e463ccbc2d97c151aa992075354d4639344"
        );
    }

    #[test]
    fn same_sources_same_bytes() {
        let a = bake(&temp_data(ONE)).unwrap();
        let b = bake(&temp_data(ONE)).unwrap();
        assert_eq!(a.pack, b.pack);
    }

    #[test]
    fn real_data_bakes() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let baked = bake(&dir).unwrap();
        assert!(baked.units >= 4);
        assert!(baked.buildings >= 4 && baked.conquest_units >= 4);
    }

    #[test]
    fn data_files_lists_every_roster() {
        for (path, _) in conquest::ROSTER_FILES {
            assert!(DATA_FILES.contains(&path), "{path}");
        }
    }

    #[test]
    fn bakes_rosters_in_faction_order() {
        let baked = bake(&temp_data(ONE)).unwrap();
        assert_eq!((baked.buildings, baked.conquest_units), (4, 4));
        let store = aligned(&baked.pack);
        let bytes = &bytemuck::cast_slice::<u128, u8>(&store)[..baked.pack.len()];
        let pack = Pack::parse(bytes).unwrap();
        let data = GameData::from_pack(&pack).unwrap();
        for (i, faction) in Faction::ALL.into_iter().enumerate() {
            let (b, u) = (&data.buildings[i], &data.conquest_units[i]);
            assert_eq!(b.name(), format!("{faction:?} Hall"));
            assert_eq!((b.faction(), b.start_count), (faction, 1));
            assert_eq!((b.build_ticks, b.hp, b.supply), (600, 1000, 12));
            assert_eq!(
                (b.width, b.depth, b.upgrades_from),
                (4, 4, gc_data::conquest::NONE)
            );
            assert_eq!(b.flags, gc_data::conquest::BUILDING_DROP_OFF_GRUB);
            assert_eq!(u.name(), format!("{faction:?} Hand"));
            assert_eq!((u.faction(), u.start_count), (faction, 2));
            assert_eq!(u.trained_at, u16::try_from(i).unwrap());
            assert_eq!((u.train_ticks, u.cost_grub, u.supply), (240, 50, 1));
            assert!(u.worker());
            // The roster leaves points out; a unit units.ron doesn't list has none.
            assert_eq!(u.stats.points, 0);
        }
    }

    /// Bake the small fixture with one roster file replaced.
    fn bake_with(file: &str, roster: String) -> String {
        let dir = temp_data(ONE);
        std::fs::write(dir.join("rosters").join(file), roster).unwrap();
        bake(&dir).err().expect("should not bake").to_string()
    }

    #[test]
    fn rejects_bad_rosters() {
        let brood = small_roster("Brood");
        let err = bake_with("brood.ron", small_roster("Choir"));
        assert!(
            err.contains("brood.ron") && err.contains("must be Brood"),
            "{err}"
        );

        let err = bake_with(
            "brood.ron",
            brood.replace(
                "flags: [DropOffGrub]",
                r#"upgrades_from: Some("Nope"), flags: []"#,
            ),
        );
        assert!(
            err.contains("Brood Hall: upgrades unknown building Nope"),
            "{err}"
        );

        let err = bake_with(
            "brood.ron",
            brood.replace(
                "flags: [DropOffGrub]",
                "attack: Some((range: 10.0, damage: 0, reload: 2.0, shots: 1))",
            ),
        );
        assert!(err.contains("Brood Hall: a tower needs"), "{err}");

        let err = bake_with("brood.ron", brood.replace("Brood Hand\", 2", "Nobody\", 2"));
        assert!(err.contains("starting_units: no unit Nobody"), "{err}");

        let err = bake_with(
            "brood.ron",
            brood.replace(
                r#"from: Building("Brood Hall")"#,
                r#"from: Evolves("EmberClans Hand")"#,
            ),
        );
        assert!(
            err.contains("evolves from unknown unit EmberClans Hand"),
            "{err}"
        );

        let err = bake_with(
            "brood.ron",
            brood.replace(
                r#"from: Building("Brood Hall")"#,
                r#"from: Building("Hall")"#,
            ),
        );
        assert!(err.contains("trained at unknown building Hall"), "{err}");

        let err = bake_with(
            "brood.ron",
            brood.replace("train_time: 12.0", "train_time: 0.0"),
        );
        assert!(err.contains("train_time"), "{err}");

        let err = bake_with("brood.ron", brood.replace("Brood Hall", "EmberClans Hall"));
        assert!(err.contains("EmberClans Hall: defined twice"), "{err}");
    }

    #[test]
    fn bakes_ranged_stats() {
        let archers = r#"[(name: "Bows", faction: EmberClans, models: 16, hp_per_model: 8,
            files: 8, speed: 3.0, turn_rate: 70.0, file_spacing: 0.8, rank_spacing: 1.0,
            melee_attack: 15, melee_defence: 20, melee_damage: 2, charge_bonus: 5, points: 100, morale: 50,
            ranged: Some((range: 30.0, damage: 4, reload: 4.0, accuracy: 30)))]"#;
        let baked = bake(&temp_data(archers)).unwrap();
        let store = aligned(&baked.pack);
        let bytes = &bytemuck::cast_slice::<u128, u8>(&store)[..baked.pack.len()];
        let pack = Pack::parse(bytes).unwrap();
        let u = GameData::from_pack(&pack).unwrap().units[0];
        assert_eq!(u.range(), Fx::from_int(30));
        assert_eq!((u.missile_damage, u.reload_ticks, u.accuracy), (4, 80, 30));
    }

    #[test]
    fn bakes_mass_and_weapons() {
        let units = r#"[
            (name: "Pikes", faction: EmberClans, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 70, reach: true),
            (name: "Wall", faction: Brood, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 70, mass: Heavy, swing_room: true, shield: true),
            (name: "Swarm", faction: Brood, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 70, mass: Light),
        ]"#;
        let baked = bake(&temp_data(units)).unwrap();
        let store = aligned(&baked.pack);
        let bytes = &bytemuck::cast_slice::<u128, u8>(&store)[..baked.pack.len()];
        let pack = Pack::parse(bytes).unwrap();
        let u = GameData::from_pack(&pack).unwrap().units;
        assert_eq!(
            (u[0].mass, u[0].reach(), u[0].swing_room(), u[0].shield()),
            (1, true, false, false)
        );
        assert_eq!(
            (u[1].mass, u[1].reach(), u[1].swing_room(), u[1].shield()),
            (2, false, true, true)
        );
        assert_eq!((u[2].mass, u[2].weapons), (0, 0));

        let bad = r#"[(name: "X", faction: Brood, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 70, mass: Huge)]"#;
        assert!(matches!(bake(&temp_data(bad)), Err(BakeError::Parse(..))));
    }

    #[test]
    fn bakes_morale() {
        let undead = r#"[
            (name: "Brave", faction: EmberClans, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 70),
            (name: "Dead", faction: CarrionConcord, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, never_routs: true),
        ]"#;
        let baked = bake(&temp_data(undead)).unwrap();
        let store = aligned(&baked.pack);
        let bytes = &bytemuck::cast_slice::<u128, u8>(&store)[..baked.pack.len()];
        let pack = Pack::parse(bytes).unwrap();
        let units = GameData::from_pack(&pack).unwrap().units;
        assert_eq!((units[0].morale, units[1].morale), (70, 0));
    }

    #[test]
    fn rejects_bad_definitions() {
        let dup = r#"[
            (name: "A", faction: Brood, models: 1, hp_per_model: 1, files: 1, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50),
            (name: "A", faction: Brood, models: 1, hp_per_model: 1, files: 1, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50),
        ]"#;
        let err = bake(&temp_data(dup)).err().unwrap().to_string();
        assert!(err.contains("defined twice"), "{err}");

        let slow = r#"[(name: "B", faction: Choir, models: 1, hp_per_model: 1, files: 1, speed: 0.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50)]"#;
        let err = bake(&temp_data(slow)).err().unwrap().to_string();
        assert!(err.contains("speed"), "{err}");

        let wide = r#"[(name: "D", faction: Choir, models: 4, hp_per_model: 1, files: 5, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50)]"#;
        let err = bake(&temp_data(wide)).err().unwrap().to_string();
        assert!(err.contains("files"), "{err}");

        let big = r#"[(name: "E", faction: Brood, models: 33, hp_per_model: 1, files: 8, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50)]"#;
        let err = bake(&temp_data(big)).err().unwrap().to_string();
        assert!(err.contains("32 models"), "{err}");

        let no_reload = r#"[(name: "F", faction: Choir, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50, ranged: Some((range: 20.0, damage: 2, reload: 0.0, accuracy: 30)))]"#;
        let err = bake(&temp_data(no_reload)).err().unwrap().to_string();
        assert!(err.contains("reload"), "{err}");

        let fearless_and_brave = r#"[(name: "G", faction: CarrionConcord, models: 1, hp_per_model: 1, files: 1, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50, never_routs: true)]"#;
        let err = bake(&temp_data(fearless_and_brave))
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("never_routs"), "{err}");
        let no_morale = r#"[(name: "H", faction: Brood, models: 1, hp_per_model: 1, files: 1, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100)]"#;
        let err = bake(&temp_data(no_morale)).err().unwrap().to_string();
        assert!(err.contains("morale"), "{err}");
        let free = r#"[(name: "I", faction: Brood, models: 1, hp_per_model: 1, files: 1, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 0, morale: 50)]"#;
        let err = bake(&temp_data(free)).err().unwrap().to_string();
        assert!(err.contains("points"), "{err}");

        let typo = r#"[(name: "C", faction: Choir, models: 1, hp_per_model: 1, files: 1, sped: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, points: 100, morale: 50)]"#;
        assert!(matches!(bake(&temp_data(typo)), Err(BakeError::Parse(..))));
    }
}
