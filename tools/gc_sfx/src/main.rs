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
//! ```
//!
//! Real recorded sounds (CC0 packs, custom recordings) can replace any of
//! these later: the bank only cares about the clip, not where it came from.

mod synth;

use gc_audio::{ClipSource, SoundId, write_bank};
use std::path::{Path, PathBuf};

fn main() {
    let mut out = PathBuf::from("assets/audio/sounds.gcsnd");
    let mut wav_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match (flag.as_str(), args.next()) {
            ("--out", Some(v)) => out = v.into(),
            ("--wav", Some(v)) => wav_dir = Some(v.into()),
            _ => {
                eprintln!("usage: gc_sfx [--out FILE] [--wav DIR]");
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
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("create output directory");
    }
    std::fs::write(&out, &blob).expect("write sound bank");
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
