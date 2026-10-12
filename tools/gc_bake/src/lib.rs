//! The Grubcraft baker. Reads human-edited sources (RON game data today; art
//! and audio later), validates them, and writes the asset pack.
//!
//! Baking is deterministic: the same sources produce byte-identical packs on
//! every machine, which is what lets players compare data hashes.

use std::fmt;
use std::path::{Path, PathBuf};

use gc_assets::{Kind, PackWriter, table_bytes};
use gc_data::{Faction, MANIFEST_PATH, NAME_BYTES, UNIT_SCHEMA, UNITS_PATH, UnitRecord};
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
    speed: f64,
    frontage: f64,
    depth: f64,
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
            frontage: to_fx(def.frontage)
                .map_err(|why| invalid(&def.name, format!("frontage {why}")))?,
            depth: to_fx(def.depth).map_err(|why| invalid(&def.name, format!("depth {why}")))?,
            models: def.models,
            hp_per_model: def.hp_per_model,
            faction: Faction::from(def.faction) as u8,
            _pad: [0; 3],
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
        hp_per_model: 10, speed: 3.0, frontage: 6.0, depth: 2.5)]"#;

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
        assert_eq!(u.depth(), Fx::from_ratio(5, 2));
    }

    /// Baking must give the same bytes on every platform, or Mac and Windows
    /// players would see different data hashes. CI runs this on all three.
    #[test]
    fn fixture_hash_is_the_same_everywhere() {
        let baked = bake(&temp_data(ONE)).unwrap();
        let hex: String = baked.data_hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "3b98a5d249a4ddfeb3cd2d6611764bba84f96576d80e9c668ac3d88e57541436"
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
    fn rejects_bad_definitions() {
        let dup = r#"[
            (name: "A", faction: Brood, models: 1, hp_per_model: 1, speed: 1.0, frontage: 1.0, depth: 1.0),
            (name: "A", faction: Brood, models: 1, hp_per_model: 1, speed: 1.0, frontage: 1.0, depth: 1.0),
        ]"#;
        let err = bake(&temp_data(dup)).err().unwrap().to_string();
        assert!(err.contains("defined twice"), "{err}");

        let slow = r#"[(name: "B", faction: Choir, models: 1, hp_per_model: 1, speed: 0.0, frontage: 1.0, depth: 1.0)]"#;
        let err = bake(&temp_data(slow)).err().unwrap().to_string();
        assert!(err.contains("speed"), "{err}");

        let typo = r#"[(name: "C", faction: Choir, models: 1, hp_per_model: 1, sped: 1.0, frontage: 1.0, depth: 1.0)]"#;
        assert!(matches!(bake(&temp_data(typo)), Err(BakeError::Parse(..))));
    }
}
