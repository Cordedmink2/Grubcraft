//! Bake the game data (`data/*.ron`) into the asset pack embedded in the
//! executable, so a build can never run with stale or mismatched data.

use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let data_dir = manifest_dir.join("../../data");
    println!("cargo:rerun-if-changed={}", data_dir.display());
    for file in gc_bake::DATA_FILES {
        println!("cargo:rerun-if-changed={}", data_dir.join(file).display());
    }

    let baked = match gc_bake::bake(&data_dir) {
        Ok(b) => b,
        Err(e) => {
            // Shows up as a build error pointing at the bad definition.
            eprintln!("error: game data failed to bake: {e}");
            std::process::exit(1);
        }
    };
    // The build's commit, so players can tell builds apart in the lobby
    // (the version gate compares it).
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(&manifest_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local".into());
    println!("cargo:rustc-env=GRUBCRAFT_COMMIT={commit}");
    let git_head = manifest_dir.join("../../.git/HEAD");
    if git_head.exists() {
        println!("cargo:rerun-if-changed={}", git_head.display());
        println!(
            "cargo:rerun-if-changed={}",
            manifest_dir.join("../../.git/refs/heads").display()
        );
    }

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("grubcraft.pak");
    std::fs::write(&out, &baked.pack).expect("write baked pack");
}
