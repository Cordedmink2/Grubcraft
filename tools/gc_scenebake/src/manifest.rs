//! The manifest written beside `scenery.pak`, and the check CI runs on it.
//!
//! Same idea as `gc_artbake`'s: it records the baker and scenery schema
//! versions, a hash of `scenery.ron`, a hash of every source glb and a hash
//! of the pack. The check cannot re-bake (the glbs live in the project's
//! shared folder), so it compares what the repository holds and, for a
//! `shared:` source, only when the file is found under `--shared`.

use std::path::{Path, PathBuf};

use gc_assets::manifest::{Manifest, hash_hex};
use gc_assets::scenery::SCENERY_SCHEMA;

use crate::{Config, Options};

/// Bump when a change to the baker changes the bytes it bakes from the same
/// sources, so every committed pack is flagged for a re-bake.
pub const BAKER_VERSION: u32 = 2;

const FORMAT: &str = "1";
const NOT_IN_REPO: &str = "not in the repo";

fn manifest_path(opts: &Options) -> PathBuf {
    opts.out.with_extension("manifest")
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The hash of `scenery.ron`, whatever line endings a checkout gave it.
fn config_hash(text: &str) -> String {
    hash_hex(text.replace("\r\n", "\n").as_bytes())
}

/// Write the manifest for a pack just baked from the models in `sources`.
pub fn write(
    opts: &Options,
    config_text: &str,
    sources: &[&str],
    pack: &[u8],
) -> Result<(), String> {
    let mut m = Manifest::new();
    m.set("manifest", FORMAT);
    m.set("baker", &format!("gc_scenebake {BAKER_VERSION}"));
    m.set("schema", &SCENERY_SCHEMA.to_string());
    m.set("config", &config_hash(config_text));
    m.set("pack", &hash_hex(pack));
    m.set("pack_bytes", &pack.len().to_string());
    for src in sources {
        let hash = hash_hex(&read(&opts.resolve(src)?)?);
        let value = if src.starts_with("shared:") {
            format!("{hash} {NOT_IN_REPO}")
        } else {
            hash
        };
        m.set(&format!("source {src}"), &value);
    }
    let header = "Written by `cargo run -p gc_scenebake --release` beside scenery.pak and checked by\n\
        `cargo run -p gc_scenebake -- --check` in CI. Do not edit by hand.\n\
        Hashes are XXH3-128. Sources marked \"not in the repo\" live in the project's\n\
        shared folder; CI cannot check them, so a change to one shows only in a re-bake.";
    let path = manifest_path(opts);
    std::fs::write(&path, m.render(header)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Compare the committed pack with its manifest, `scenery.ron` and the
/// sources this checkout holds.
pub fn check(opts: &Options) -> Result<(), String> {
    let path = manifest_path(opts);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "{}: {e} (run `cargo run -p gc_scenebake --release` to write it)",
            path.display()
        )
    })?;
    let m = Manifest::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let config_text = std::fs::read_to_string(&opts.config)
        .map_err(|e| format!("{}: {e}", opts.config.display()))?;
    let config: Config =
        ron::from_str(&config_text).map_err(|e| format!("{}: {e}", opts.config.display()))?;

    let mut stale: Vec<String> = Vec::new();
    let mut expect = |what: &str, key: &str, now: String| {
        let then = m.get(key).unwrap_or("(missing)");
        if then != now {
            stale.push(format!("{what} is `{now}` but the manifest has `{then}`"));
        }
    };
    expect("manifest format", "manifest", FORMAT.into());
    expect("baker", "baker", format!("gc_scenebake {BAKER_VERSION}"));
    expect("scenery schema", "schema", SCENERY_SCHEMA.to_string());
    expect("scenery.ron hash", "config", config_hash(&config_text));
    match read(&opts.out) {
        Ok(pack) => {
            expect("pack hash", "pack", hash_hex(&pack));
            expect("pack size", "pack_bytes", pack.len().to_string());
            if let Err(e) = crate::read_back(&pack) {
                stale.push(format!("the pack does not read back: {e}"));
            }
        }
        Err(e) => stale.push(e),
    }

    let (mut compared, mut unchecked) = (0, 0);
    let wanted: Vec<&str> = config.models.iter().map(|m| m.source.as_str()).collect();
    for src in &wanted {
        let key = format!("source {src}");
        let Some(value) = m.get(&key) else {
            stale.push(format!("{src} is in scenery.ron but not in the manifest"));
            continue;
        };
        let then = value.split(' ').next().unwrap_or_default();
        let file = opts.resolve(src)?;
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !src.starts_with("repo:") => {
                unchecked += 1;
                continue;
            }
            Err(e) => {
                stale.push(format!("{src}: {}: {e}", file.display()));
                continue;
            }
        };
        compared += 1;
        if hash_hex(&bytes) != then {
            stale.push(format!("{src} has changed since the pack was baked"));
        }
    }
    for (key, _) in m.entries() {
        if let Some(src) = key.strip_prefix("source ")
            && !wanted.contains(&src)
        {
            stale.push(format!(
                "{src} is in the manifest but no longer in scenery.ron"
            ));
        }
    }

    println!(
        "{}: {compared} sources compared, {unchecked} shared sources not checked (not in this checkout)",
        path.display()
    );
    if stale.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} is stale:\n  {}\nRe-bake with `cargo run -p gc_scenebake --release` (it needs the shared sources) and commit assets/scenery.pak and assets/scenery.manifest together.",
        opts.out.display(),
        stale.join("\n  ")
    ))
}
