//! Headless replay runner (docs/10-architecture.md, "Tools").
//!
//! Usage:
//!   gc_replay verify [PATH...]   play each replay back and check its state
//!                                hashes (default: the repository's replays/)
//!   gc_replay bless PATH...      play each back and store the hashes the
//!                                simulation produces now
//!   gc_replay info PATH...       describe each replay
//!
//! A PATH may be a `.gcr` file or a folder of them. `verify` exits non-zero
//! when any replay fails, so CI can run it on every platform.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use gc_replay::{EXTENSION, Replay};
use gc_sim::TICKS_PER_SECOND;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(action) = args.next() else {
        return usage();
    };
    let mut paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if paths.is_empty() {
        if action != "verify" {
            return usage();
        }
        paths.push(default_folder());
    }
    let files = match expand(&paths) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let mut failed = 0;
    for file in &files {
        let ok = match action.as_str() {
            "verify" => verify(file),
            "bless" => bless(file),
            "info" => info(file),
            _ => return usage(),
        };
        failed += !ok as usize;
    }
    if failed > 0 {
        eprintln!("{failed} of {} replays failed", files.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: gc_replay verify [PATH...] | bless PATH... | info PATH...");
    ExitCode::from(2)
}

/// `replays/` in the working directory, or else in the repository this was
/// built from.
fn default_folder() -> PathBuf {
    let here = PathBuf::from("replays");
    if here.is_dir() {
        return here;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../replays")
}

/// Files as given, and the `.gcr` files in folders as given, sorted.
fn expand(paths: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(p)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|f| f.extension().is_some_and(|x| x == EXTENSION))
                .collect();
            found.sort();
            if found.is_empty() {
                return Err(std::io::Error::other(format!(
                    "no .{EXTENSION} replays in {}",
                    p.display()
                )));
            }
            files.extend(found);
        } else {
            files.push(p.clone());
        }
    }
    Ok(files)
}

fn load(file: &Path) -> Option<Replay> {
    match Replay::load(file) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("{}: {e}", file.display());
            None
        }
    }
}

fn name(file: &Path) -> String {
    file.file_name().map_or_else(
        || file.display().to_string(),
        |n| n.to_string_lossy().into(),
    )
}

fn verify(file: &Path) -> bool {
    let Some(replay) = load(file) else {
        return false;
    };
    let t0 = Instant::now();
    match replay.verify() {
        Ok(world) => {
            println!(
                "ok    {:<28} {:>6} ticks  {:>5} checkpoints  final {:#018x}  {:.2} s",
                name(file),
                replay.ticks,
                replay.checkpoints.len(),
                world.state_hash(),
                t0.elapsed().as_secs_f64()
            );
            true
        }
        Err(d) => {
            println!("FAIL  {:<28} {d}", name(file));
            eprintln!(
                "  The simulation no longer plays {} the way it was recorded. If the change \
                 is meant to alter how battles play out, run `cargo run --release -p gc_replay \
                 -- bless {}` and say why in the pull request.",
                name(file),
                file.display()
            );
            false
        }
    }
}

fn bless(file: &Path) -> bool {
    let Some(mut replay) = load(file) else {
        return false;
    };
    let before = replay.checkpoints.clone();
    replay.bless();
    let changed = replay
        .checkpoints
        .iter()
        .zip(&before)
        .position(|(a, b)| a != b);
    if let Err(e) = replay.save(file) {
        eprintln!("{}: {e}", file.display());
        return false;
    }
    match changed {
        None if before.len() == replay.checkpoints.len() => {
            println!("same  {} (hashes unchanged)", name(file));
        }
        _ => println!(
            "new   {} (hashes differ from tick {})",
            name(file),
            changed.map_or(0, |i| replay.checkpoints[i].0)
        ),
    }
    true
}

fn info(file: &Path) -> bool {
    let Some(replay) = load(file) else {
        return false;
    };
    let size = std::fs::metadata(file).map_or(0, |m| m.len());
    let seconds = replay.ticks / TICKS_PER_SECOND;
    let mut spawned = vec![(0u32, 0u32); replay.teams.len().max(1)];
    for (_, commands) in &replay.turns {
        for c in commands {
            if let gc_sim::Command::SpawnRegiment { unit, .. } = c.command {
                let p = c.player as usize;
                if p >= spawned.len() {
                    spawned.resize(p + 1, (0, 0));
                }
                spawned[p].0 += 1;
                spawned[p].1 += replay.units.get(unit as usize).map_or(0, |u| u.models) as u32;
            }
        }
    }
    println!("{}", file.display());
    println!("  {}", replay.note);
    println!(
        "  {}:{:02} of game time ({} ticks), {} commands, {:.1} KB",
        seconds / 60,
        seconds % 60,
        replay.ticks,
        replay.command_count(),
        size as f64 / 1024.0
    );
    println!(
        "  seed {:#x}, {} tiles across, {} unit types, {}{}",
        replay.seed,
        replay.map_tiles,
        replay.units.len(),
        if replay.nav.is_empty() {
            "open plane"
        } else {
            "with obstacles"
        },
        if replay.heights.is_empty() {
            ", flat"
        } else {
            ", with heights"
        }
    );
    for (p, (regiments, soldiers)) in spawned.iter().enumerate() {
        let team = replay
            .teams
            .get(p)
            .map_or(String::new(), |t| format!(" (team {t})"));
        println!("  player {p}{team}: {regiments} regiments, {soldiers} soldiers");
    }
    true
}
