//! The Grubcraft baker. Reads human-edited sources (RON game data today; art
//! and audio later), validates them, and writes the asset pack.
//!
//! Baking is deterministic: the same sources produce byte-identical packs on
//! every machine, which is what lets players compare data hashes.

use std::fmt;
use std::path::{Path, PathBuf};

use gc_assets::{Kind, PackWriter, table_bytes};
use gc_data::{
    ANGLE_STEPS_PER_TURN, Faction, MANIFEST_PATH, NAME_BYTES, UNIT_SCHEMA, UNITS_PATH, UnitRecord,
};
use gc_fixed::Fx;
use serde::Deserialize;

/// The data files the baker reads, relative to the data directory. Build
/// scripts use this list to know when to re-bake.
pub const DATA_FILES: &[&str] = &["units.ron"];

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitDef {
    name: String,
    faction: FactionDef,
    models: u16,
    hp_per_model: u16,
    files: u16,
    speed: f64,
    /// Degrees per second.
    turn_rate: f64,
    file_spacing: f64,
    rank_spacing: f64,
    melee_attack: u8,
    melee_defence: u8,
    melee_damage: u16,
    /// Percent extra melee damage when a charge hits home.
    charge_bonus: u8,
    /// Shooting: range in tiles, damage per arrow, seconds between volleys,
    /// percent of arrows that hit. Leave out for regiments that don't shoot.
    #[serde(default)]
    ranged: Option<RangedDef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RangedDef {
    range: f64,
    damage: u16,
    reload: f64,
    accuracy: u8,
}

#[derive(Deserialize, Clone, Copy)]
enum FactionDef {
    EmberClans,
    Brood,
    Choir,
    CarrionConcord,
}

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

/// The result of a bake.
pub struct Baked {
    pub pack: Vec<u8>,
    /// BLAKE3 of every data table.
    pub data_hash: [u8; 32],
    pub units: usize,
}

/// Bake everything under `data_dir` into a pack.
pub fn bake(data_dir: &Path) -> Result<Baked, BakeError> {
    let units_file = data_dir.join("units.ron");
    let units = bake_units(&units_file)?;
    let units_table = table_bytes(UNIT_SCHEMA, &units);

    let data_hash = data_hash(&[(UNITS_PATH, &units_table)]);

    let mut pack = PackWriter::new();
    pack.add(UNITS_PATH, Kind::Table, &units_table);
    pack.add(MANIFEST_PATH, Kind::Blob, &data_hash);
    Ok(Baked {
        pack: pack.finish(),
        data_hash,
        units: units.len(),
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
    let defs: Vec<UnitDef> = ron::from_str(&text).map_err(|e| BakeError::Parse(file.into(), e))?;
    let invalid = |entry: &str, why: String| BakeError::Invalid {
        file: file.into(),
        entry: entry.into(),
        why,
    };

    let mut records = Vec::with_capacity(defs.len());
    for def in &defs {
        if records
            .iter()
            .any(|r: &UnitRecord| r.name() == def.name.as_str())
        {
            return Err(invalid(&def.name, "defined twice".into()));
        }
        if def.name.len() > NAME_BYTES || def.name.contains('\0') {
            return Err(invalid(
                &def.name,
                format!("names are at most {NAME_BYTES} bytes, with no NUL"),
            ));
        }
        let mut name = [0u8; NAME_BYTES];
        name[..def.name.len()].copy_from_slice(def.name.as_bytes());
        let record = UnitRecord {
            name,
            speed: to_fx(def.speed).map_err(|why| invalid(&def.name, format!("speed {why}")))?,
            file_spacing: to_fx(def.file_spacing)
                .map_err(|why| invalid(&def.name, format!("file_spacing {why}")))?,
            rank_spacing: to_fx(def.rank_spacing)
                .map_err(|why| invalid(&def.name, format!("rank_spacing {why}")))?,
            turn_rate: to_angle_rate(def.turn_rate)
                .map_err(|why| invalid(&def.name, format!("turn_rate {why}")))?,
            models: def.models,
            hp_per_model: def.hp_per_model,
            files: def.files,
            faction: Faction::from(def.faction) as u8,
            _pad: 0,
            range: match &def.ranged {
                Some(r) => {
                    to_fx(r.range).map_err(|why| invalid(&def.name, format!("range {why}")))?
                }
                None => 0,
            },
            melee_damage: def.melee_damage,
            missile_damage: def.ranged.as_ref().map_or(0, |r| r.damage),
            reload_ticks: match &def.ranged {
                Some(r) => {
                    to_ticks(r.reload).map_err(|why| invalid(&def.name, format!("reload {why}")))?
                }
                None => 0,
            },
            melee_attack: def.melee_attack,
            melee_defence: def.melee_defence,
            charge_bonus: def.charge_bonus,
            accuracy: def.ranged.as_ref().map_or(0, |r| r.accuracy),
            _pad2: [0; 2],
        };
        gc_data::validate_unit(&record).map_err(|why| invalid(&def.name, why.into()))?;
        records.push(record);
    }
    if records.is_empty() {
        return Err(invalid("(file)", "defines no units".into()));
    }
    if records.len() > u16::MAX as usize {
        return Err(invalid("(file)", "too many units".into()));
    }
    Ok(records)
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
    if raw < i32::MIN as f64 || raw > i32::MAX as f64 {
        return Err("is out of range");
    }
    let raw = raw as i32;
    debug_assert!(Fx::from_raw(raw).raw() == raw);
    Ok(raw)
}

/// Convert seconds to simulation ticks (20 per second), rounded.
fn to_ticks(seconds: f64) -> Result<u16, &'static str> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("must be a positive number of seconds");
    }
    let ticks = (seconds * 20.0).round();
    if ticks > u16::MAX as f64 {
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
    let steps = (degrees * ANGLE_STEPS_PER_TURN as f64 / 360.0).round();
    if steps > u32::MAX as f64 {
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
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("units.ron"), units).unwrap();
        dir
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
        charge_bonus: 20)]"#;

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
            "6bd995b907f1517c8d370eda83c33bd7ac10696125fd23f6e73ed913a3ae326b"
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
    }

    #[test]
    fn bakes_ranged_stats() {
        let archers = r#"[(name: "Bows", faction: EmberClans, models: 16, hp_per_model: 8,
            files: 8, speed: 3.0, turn_rate: 70.0, file_spacing: 0.8, rank_spacing: 1.0,
            melee_attack: 15, melee_defence: 20, melee_damage: 2, charge_bonus: 5,
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
    fn rejects_bad_definitions() {
        let dup = r#"[
            (name: "A", faction: Brood, models: 1, hp_per_model: 1, files: 1, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0),
            (name: "A", faction: Brood, models: 1, hp_per_model: 1, files: 1, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0),
        ]"#;
        let err = bake(&temp_data(dup)).err().unwrap().to_string();
        assert!(err.contains("defined twice"), "{err}");

        let slow = r#"[(name: "B", faction: Choir, models: 1, hp_per_model: 1, files: 1, speed: 0.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0)]"#;
        let err = bake(&temp_data(slow)).err().unwrap().to_string();
        assert!(err.contains("speed"), "{err}");

        let wide = r#"[(name: "D", faction: Choir, models: 4, hp_per_model: 1, files: 5, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0)]"#;
        let err = bake(&temp_data(wide)).err().unwrap().to_string();
        assert!(err.contains("files"), "{err}");

        let big = r#"[(name: "E", faction: Brood, models: 33, hp_per_model: 1, files: 8, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0)]"#;
        let err = bake(&temp_data(big)).err().unwrap().to_string();
        assert!(err.contains("32 models"), "{err}");

        let no_reload = r#"[(name: "F", faction: Choir, models: 4, hp_per_model: 1, files: 4, speed: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0, ranged: Some((range: 20.0, damage: 2, reload: 0.0, accuracy: 30)))]"#;
        let err = bake(&temp_data(no_reload)).err().unwrap().to_string();
        assert!(err.contains("reload"), "{err}");

        let typo = r#"[(name: "C", faction: Choir, models: 1, hp_per_model: 1, files: 1, sped: 1.0, turn_rate: 90.0, file_spacing: 1.0, rank_spacing: 1.0, melee_attack: 1, melee_defence: 1, melee_damage: 1, charge_bonus: 0)]"#;
        assert!(matches!(bake(&temp_data(typo)), Err(BakeError::Parse(..))));
    }
}
