//! `gc_bake [--data DIR] [--out FILE]`: bake the asset pack.
//!
//! The game bakes its data automatically at build time (see
//! `crates/gc_game/build.rs`); run this by hand to inspect a pack or check
//! data edits quickly.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut data = PathBuf::from("data");
    let mut out = PathBuf::from("target/grubcraft.pak");
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match (flag.as_str(), args.next()) {
            ("--data", Some(v)) => data = v.into(),
            ("--out", Some(v)) => out = v.into(),
            _ => {
                eprintln!("usage: gc_bake [--data DIR] [--out FILE]");
                return ExitCode::from(2);
            }
        }
    }
    match gc_bake::bake(&data) {
        Ok(baked) => {
            if let Err(e) = std::fs::write(&out, &baked.pack) {
                eprintln!("gc_bake: {}: {e}", out.display());
                return ExitCode::FAILURE;
            }
            let hash: String = baked.data_hash.iter().map(|b| format!("{b:02x}")).collect();
            println!(
                "baked {} units, {} buildings and {} Conquest units into {} ({} bytes), data hash {hash}",
                baked.units,
                baked.buildings,
                baked.conquest_units,
                out.display(),
                baked.pack.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("gc_bake: {e}");
            ExitCode::FAILURE
        }
    }
}
