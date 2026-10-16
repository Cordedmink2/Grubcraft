//! The manifest written beside `models.pak`, and the check CI runs on it.
//!
//! The manifest records the baker and model schema versions, a hash of
//! `models.ron`, a hash of every source file and a hash of the pack. The
//! check cannot re-bake: most sources are `shared:` files that only live in
//! the project's shared folder. So it hashes what the repository holds (the
//! `repo:` sources, `models.ron` and the pack) and fails when any of them no
//! longer matches the manifest. A `shared:` source is hashed too, and
//! marked in the manifest as not in the repo; it is only compared when the
//! file is found under `--shared` (on a machine that has the folder), and
//! otherwise reported as not checked. Only the files `models.ron` names are
//! hashed, not the `.bin` or images a `.gltf` file points to.

use std::path::{Path, PathBuf};

use gc_assets::manifest::{Manifest, hash_hex};
use gc_assets::model::MODEL_SCHEMA;

use crate::{Config, Options};

/// Bump when a change to the baker changes the bytes it bakes from the same
/// sources, so every committed pack is flagged for a re-bake.
pub const BAKER_VERSION: u32 = 1;

const FORMAT: &str = "1";
const NOT_IN_REPO: &str = "not in the repo";

fn manifest_path(opts: &Options) -> PathBuf {
    opts.out.with_extension("manifest")
}

/// Every source file the config names, each once: models, then their maps.
fn sources(config: &Config) -> Vec<&str> {
    let mut all: Vec<&str> = Vec::new();
    for m in &config.models {
        all.push(&m.source);
        if let Some(t) = &m.textures {
            let maps = [Some(&t.albedo), t.team.as_ref(), t.normal.as_ref()]
                .into_iter()
                .chain([t.orm.as_ref(), t.emissive.as_ref()]);
            all.extend(maps.flatten().map(String::as_str));
        }
    }
    let mut seen = std::collections::HashSet::new();
    all.retain(|s| seen.insert(*s));
    all
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Write the manifest for a pack just baked from `config` (whose file text
/// is `config_text`).
/// The hash of `models.ron`, whatever line endings a checkout gave it.
fn config_hash(text: &str) -> String {
    hash_hex(text.replace("\r\n", "\n").as_bytes())
}

pub fn write(
    opts: &Options,
    config_text: &str,
    config: &Config,
    pack: &[u8],
) -> Result<(), String> {
    let mut m = Manifest::new();
    m.set("manifest", FORMAT);
    m.set("baker", &format!("gc_artbake {BAKER_VERSION}"));
    m.set("schema", &MODEL_SCHEMA.to_string());
    m.set("config", &config_hash(config_text));
    m.set("pack", &hash_hex(pack));
    m.set("pack_bytes", &pack.len().to_string());
    for src in sources(config) {
        let hash = hash_hex(&read(&opts.resolve(src)?)?);
        let value = if src.starts_with("shared:") {
            format!("{hash} {NOT_IN_REPO}")
        } else {
            hash
        };
        m.set(&format!("source {src}"), &value);
    }
    let header = "Written by `cargo run -p gc_artbake --release` beside models.pak and checked by\n\
        `cargo run -p gc_artbake -- --check` in CI. Do not edit by hand.\n\
        Hashes are XXH3-128. Sources marked \"not in the repo\" live in the project's\n\
        shared folder; CI cannot check them, so a change to one shows only in a re-bake.";
    let path = manifest_path(opts);
    std::fs::write(&path, m.render(header)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Compare the committed pack with its manifest, `models.ron` and the
/// sources the repository holds.
pub fn check(opts: &Options) -> Result<(), String> {
    let path = manifest_path(opts);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "{}: {e} (run `cargo run -p gc_artbake --release` to write it)",
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
    expect("baker", "baker", format!("gc_artbake {BAKER_VERSION}"));
    expect("model schema", "schema", MODEL_SCHEMA.to_string());
    expect("models.ron hash", "config", config_hash(&config_text));
    match read(&opts.out) {
        Ok(pack) => {
            expect("pack hash", "pack", hash_hex(&pack));
            expect("pack size", "pack_bytes", pack.len().to_string());
        }
        Err(e) => stale.push(e),
    }

    let (mut compared, mut unchecked) = (0, 0);
    let wanted = sources(&config);
    for src in &wanted {
        let key = format!("source {src}");
        let Some(value) = m.get(&key) else {
            stale.push(format!("{src} is in models.ron but not in the manifest"));
            continue;
        };
        let then = value.split(' ').next().unwrap_or_default();
        // A shared file can only be compared on a machine that has the folder.
        let file = opts.resolve(src)?;
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !src.starts_with("repo:") => {
                unchecked += 1;
                continue;
            }
            // A repo file that is missing, or any file that can't be read.
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
                "{src} is in the manifest but no longer in models.ron"
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
        "{} is stale:\n  {}\nRe-bake with `cargo run -p gc_artbake --release` (it needs the shared sources) and commit assets/models.pak and assets/models.manifest together.",
        opts.out.display(),
        stale.join("\n  ")
    ))
}
