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
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("grubcraft.pak");
    std::fs::write(&out, &baked.pack).expect("write baked pack");
}
