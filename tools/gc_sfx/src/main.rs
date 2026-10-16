//! Grubcraft sound generator.
//!
//! Every sound effect and music loop in the game is synthesised here, from
//! scratch, so there is no third-party audio to license: the sounds are
//! original work, released with the rest of the game. The output is the
//! sound bank `assets/audio/sounds.gcsnd`, which `gc_game` embeds.
//!
//! Run from the repository root:
//!
//! ```text
//! cargo run -p gc_sfx                   # rewrite assets/audio/sounds.gcsnd
//! cargo run -p gc_sfx -- --wav out/     # also write each sound as a WAV to listen to
//! cargo run -p gc_sfx -- --check        # CI: is the committed bank what the recipes make?
//! ```
//!
//! Next to the bank the tool writes `sounds.manifest` (baker version, bank
//! hash and size). The recipes are the only source and the output is exactly
//! reproducible, so `--check` re-synthesises the bank and compares it byte
//! for byte with the committed file and its manifest. Bump [`BAKER_VERSION`]
//! when a change to the tool, not to a recipe, changes the bytes.
//!
//! Real recorded sounds (CC0 packs, custom recordings) can replace any of
//! these later: the bank only cares about the clip, not where it came from.

mod synth;

use gc_assets::manifest::{Manifest, hash_hex};
use gc_audio::{ClipSource, SoundId, write_bank};
use std::path::{Path, PathBuf};

const BAKER_VERSION: u32 = 1;

fn main() {
    let mut out = PathBuf::from("assets/audio/sounds.gcsnd");
    let mut wav_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    let mut check = false;
    while let Some(flag) = args.next() {
        if flag == "--check" {
            check = true;
            continue;
        }
        match (flag.as_str(), args.next()) {
            ("--out", Some(v)) => out = v.into(),
            ("--wav", Some(v)) => wav_dir = Some(v.into()),
            _ => {
                eprintln!("usage: gc_sfx [--out FILE] [--wav DIR] | --check");
                std::process::exit(2);
            }
        }
    }

    let clips = synth::all();
    let sources: Vec<ClipSource> = clips
        .iter()
        .map(|c| ClipSource {
            id: c.id,
            rate: c.rate,
            looped: c.looped,
            pcm: &c.pcm,
        })
        .collect();
    let blob = write_bank(&sources);
    let manifest = manifest(&blob, clips.len());
    if check {
        if let Err(e) = check_bank(&out, &blob, &manifest) {
            eprintln!("gc_sfx: {e}");
            std::process::exit(1);
        }
        println!("{} matches its recipes and manifest", out.display());
        return;
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("create output directory");
    }
    std::fs::write(&out, &blob).expect("write sound bank");
    std::fs::write(manifest_path(&out), manifest.render(MANIFEST_HEADER)).expect("write manifest");
    println!("wrote {} ({} bytes)", out.display(), blob.len());
    for c in &clips {
        println!(
            "  {:<12} {:>5.2}s  {:>5} Hz  {}",
            format!("{:?}", c.id),
            c.pcm.len() as f32 / c.rate as f32,
            c.rate,
            if c.looped { "loop" } else { "" }
        );
    }
    if let Some(dir) = wav_dir {
        std::fs::create_dir_all(&dir).expect("create wav directory");
        for c in &clips {
            write_wav(&dir.join(format!("{:?}.wav", c.id)), c.rate, &c.pcm);
        }
        println!("wrote WAV files to {}", dir.display());
    }
    let _ = SoundId::ALL;
}

const MANIFEST_HEADER: &str = "Written by `cargo run -p gc_sfx` beside sounds.gcsnd and checked by\n\
    `cargo run -p gc_sfx -- --check` in CI. Do not edit by hand.\n\
    The sounds are synthesised by tools/gc_sfx/src/synth.rs; there are no other sources.\n\
    Hashes are XXH3-128.";

fn manifest_path(bank: &Path) -> PathBuf {
    bank.with_extension("manifest")
}

fn manifest(blob: &[u8], clips: usize) -> Manifest {
    let mut m = Manifest::new();
    m.set("manifest", "1");
    m.set("baker", &format!("gc_sfx {BAKER_VERSION}"));
    m.set("clips", &clips.to_string());
    m.set("sources", "none (synthesised)");
    m.set("bank", &hash_hex(blob));
    m.set("bank_bytes", &blob.len().to_string());
    m
}

/// Fail unless the committed bank and manifest are both what the recipes
/// make now.
fn check_bank(path: &Path, fresh: &[u8], expected: &Manifest) -> Result<(), String> {
    let committed = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mpath = manifest_path(path);
    let text = std::fs::read_to_string(&mpath).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let found = Manifest::parse(&text).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let mut stale = Vec::new();
    if committed != fresh {
        stale.push(format!(
            "{} differs from what the recipes synthesise now",
            path.display()
        ));
    }
    if found != *expected {
        stale.push(format!("{} does not match the new bank", mpath.display()));
    }
    if stale.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{}\nRun `cargo run -p gc_sfx` and commit the bank and its manifest together.",
        stale.join("\n")
    ))
}

fn write_wav(path: &Path, rate: u32, pcm: &[i16]) {
    let data_len = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + pcm.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b).expect("write wav");
}
