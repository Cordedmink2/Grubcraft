//! The shared detail textures (`assets/detail.pak`): fifteen classes of
//! repeating surface, two 256 × 256 layers each, baked from the PNGs in
//! `assets-src/detail/` (made by `scripts/gen_detail.py`). See
//! `docs/31-surface-detail.md` and `gc_assets::detail`.
//!
//! `gc_artbake --detail --out assets/detail.pak` bakes them and
//! `--detail --check` is the CI check, like the model packs' manifests.

use std::path::{Path, PathBuf};

use gc_assets::detail::{
    BLOCKS_PER_LAYER, DETAIL_MIPS, DETAIL_SCHEMA, DETAIL_SIZE, DetailClass, DetailTables, LAYERS,
};
use gc_assets::manifest::{Manifest, hash_hex};

use crate::manifest::{FORMAT, Stale, manifest_path, read};
use crate::textures::{Encoding, Layer, compress, load_png, mip_chain, pixels};

/// Bump when a change to this baker changes the bytes it bakes from the same
/// images.
const BAKER_VERSION: u32 = 1;

/// Each class's two source images, in layer order.
fn sources(dir: &Path) -> Vec<(PathBuf, Encoding)> {
    DetailClass::ALL
        .into_iter()
        .flat_map(|c| {
            [
                (dir.join(format!("{}_d.png", c.name())), Encoding::Linear),
                // Linear, so averaging the relief's tilt flattens it with distance.
                (dir.join(format!("{}_n.png", c.name())), Encoding::Linear),
            ]
        })
        .collect()
}

fn layer(path: &Path) -> Result<Layer, String> {
    let img = load_png(path)?;
    let side = DETAIL_SIZE as usize;
    if img.width as usize != side || img.height as usize != side {
        return Err(format!(
            "{}: {} × {} texels, must be {side} × {side}",
            path.display(),
            img.width,
            img.height
        ));
    }
    let px = pixels(&img, Encoding::Linear);
    Ok(Layer { side, px })
}

/// The pack's bytes, baked from the images in `dir`.
pub fn bake(dir: &Path) -> Result<Vec<u8>, String> {
    let mut tables = DetailTables::default();
    for (path, enc) in sources(dir) {
        let chain = mip_chain(layer(&path)?, enc, DETAIL_MIPS);
        tables.textures.extend(compress(&chain));
    }
    assert_eq!(tables.textures.len(), LAYERS * BLOCKS_PER_LAYER);
    let mut pack = gc_assets::PackWriter::new();
    tables.write(&mut pack);
    Ok(pack.finish())
}

pub fn run(dir: &Path, out: &Path) -> Result<(), String> {
    let bytes = bake(dir)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut m = Manifest::new();
    m.set("manifest", FORMAT);
    m.set("baker", &format!("gc_artbake detail {BAKER_VERSION}"));
    m.set("schema", &DETAIL_SCHEMA.to_string());
    m.set("pack", &hash_hex(&bytes));
    m.set("pack_bytes", &bytes.len().to_string());
    for (path, _) in sources(dir) {
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        m.set(&format!("source {}", file_name(&path)), &hash_hex(&data));
    }
    let header = "Written by `cargo run -p gc_artbake --release -- --detail` beside detail.pak and\n\
        checked by `cargo run -p gc_artbake -- --detail --check` in CI. Do not edit by hand.\n\
        Hashes are XXH3-128.";
    let path = manifest_path(out);
    std::fs::write(&path, m.render(header)).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "wrote {} ({} KB): {} classes",
        out.display(),
        bytes.len() / 1024,
        DetailClass::ALL.len()
    );
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Compare the committed pack with its manifest and its images.
pub fn check(dir: &Path, out: &Path) -> Result<(), String> {
    let path = manifest_path(out);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "{}: {e} (run `cargo run -p gc_artbake --release -- --detail` to write it)",
            path.display()
        )
    })?;
    let m = Manifest::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut stale = Stale::new(&m);
    stale.pack(
        out,
        format!("gc_artbake detail {BAKER_VERSION}"),
        DETAIL_SCHEMA,
    );
    for (src, _) in sources(dir) {
        let name = file_name(&src);
        match read(&src) {
            Ok(data) => stale.expect(&name, &format!("source {name}"), hash_hex(&data)),
            Err(e) => stale.found.push(e),
        }
    }
    if stale.found.is_empty() {
        println!(
            "{}: {} sources compared",
            path.display(),
            sources(dir).len()
        );
        return Ok(());
    }
    Err(format!(
        "{} is stale:\n  {}\nRe-bake with `cargo run -p gc_artbake --release -- --detail --out {}` and commit the pack and its manifest together.",
        out.display(),
        stale.found.join("\n  "),
        out.display(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_two_sources_in_layer_order() {
        let list = sources(Path::new("x"));
        assert_eq!(list.len(), LAYERS);
        assert_eq!(list[0].0, Path::new("x/stone_d.png"));
        assert_eq!(list[1].0, Path::new("x/stone_n.png"));
        assert_eq!(list[LAYERS - 1].0, Path::new("x/earth_n.png"));
    }
}
