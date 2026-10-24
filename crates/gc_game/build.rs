//! Bake the game data (`data/*.ron`) into the asset pack embedded in the
//! executable, so a build can never run with stale or mismatched data.

use std::path::PathBuf;

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
    // (the version gate compares the sim version instead).
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

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("grubcraft.pak"), &baked.pack).expect("write baked pack");

    join_models(&manifest_dir.join("../../assets"), &out_dir);

    // The unit viewer's catalog (units and heroes of every roster, in words).
    let catalog = gc_bake::viewer::catalog(&data_dir).expect("unit viewer catalog");
    std::fs::write(
        out_dir.join("viewer_catalog.rs"),
        gc_bake::viewer::catalog_source(&catalog),
    )
    .expect("write viewer catalog");
}

/// Join the soldier pack (`assets/models.pak`) and every building pack
/// (`assets/buildings/*.pak`, by name) into the one model pack the game
/// embeds. They are kept apart in the repository because GitHub refuses
/// files over 100 MB, and so each faction's buildings bake on their own.
fn join_models(assets: &std::path::Path, out_dir: &std::path::Path) {
    use gc_assets::model::{ModelTables, Models};
    let buildings = assets.join("buildings");
    println!("cargo:rerun-if-changed={}", buildings.display());
    let mut packs = vec![assets.join("models.pak")];
    let mut more: Vec<PathBuf> = std::fs::read_dir(&buildings)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pak"))
        .collect();
    more.sort();
    packs.extend(more);
    // Load `bytes` as a model pack and hand its models to `then`.
    fn with_models(
        bytes: &[u8],
        then: impl FnOnce(&Models<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        // Tables are read in place, so the bytes must sit 16-byte aligned.
        let mut store = vec![0u128; bytes.len().div_ceil(16)];
        bytemuck::cast_slice_mut::<u128, u8>(&mut store)[..bytes.len()].copy_from_slice(bytes);
        let view = &bytemuck::cast_slice::<u128, u8>(&store)[..bytes.len()];
        let pack = gc_assets::Pack::parse(view).map_err(|e| e.to_string())?;
        then(&Models::from_pack(&pack).map_err(|e| e.to_string())?)
    }
    let mut joined = ModelTables::default();
    for path in &packs {
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if let Err(e) = with_models(&bytes, |m| joined.append(m).map_err(|e| e.to_string())) {
            eprintln!("error: {}: {e}", path.display());
            std::process::exit(1);
        }
    }
    let mut pack = gc_assets::PackWriter::new();
    joined.write(&mut pack);
    let bytes = pack.finish();
    // The game turns every model off if the joined pack does not load (too
    // many rows or materials together, say), so fail the build instead.
    if let Err(e) = with_models(&bytes, |_| Ok(())) {
        eprintln!("error: the joined model pack does not load: {e}");
        std::process::exit(1);
    }
    std::fs::write(out_dir.join("models.pak"), bytes).expect("write joined model pack");
}
