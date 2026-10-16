//! Golden replays: every recorded battle in `replays/` must still play out
//! exactly as it was recorded. CI runs this on macOS, Windows and Linux, so
//! it also checks that every platform plays them identically (what lets Mac
//! and Windows players share a lockstep match).
//!
//! A failure means the simulation now behaves differently. If that is
//! intended, re-bless the replays in the same pull request and say why:
//! `cargo run --release -p gc_replay -- bless replays`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gc_replay::{EXTENSION, Replay};
use gc_sim::{Command, SIM_VERSION};

/// Every golden replay, by file.
fn goldens() -> Vec<(PathBuf, Replay)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../replays");
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .expect("the replays folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|f| f.extension().is_some_and(|x| x == EXTENSION))
        .collect();
    files.sort();
    assert!(files.len() >= 4, "golden replays missing from {folder:?}");
    files
        .into_iter()
        .map(|file| {
            let replay = Replay::load(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            (file, replay)
        })
        .collect()
}

#[test]
fn golden_replays_play_as_recorded() {
    let mut failures = Vec::new();
    for (file, replay) in &goldens() {
        if let Err(d) = replay.verify() {
            failures.push(format!("{}: {d}", file.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "golden replays no longer play as recorded (re-bless them if this is intended: \
         cargo run --release -p gc_replay -- bless replays):\n{}",
        failures.join("\n")
    );
}

/// Between them the goldens give every kind of command, so a change to how
/// any of them plays out shows up. A new kind of command fails to compile
/// here until `kind` names it; add the name to `KINDS` too, and this test
/// fails until a golden gives it.
#[test]
fn the_goldens_cover_every_kind_of_command() {
    let kind = |c: &Command| match c {
        Command::SpawnRegiment { .. } => "SpawnRegiment",
        Command::MoveTo { .. } => "MoveTo",
        Command::Attack { .. } => "Attack",
        Command::RemoveRegiment { .. } => "RemoveRegiment",
        Command::TuneUnit { .. } => "TuneUnit",
        Command::SetFormation { .. } => "SetFormation",
        Command::DeployZone { .. } => "DeployZone",
        Command::Ready => "Ready",
    };
    const KINDS: [&str; 8] = [
        "SpawnRegiment",
        "MoveTo",
        "Attack",
        "RemoveRegiment",
        "TuneUnit",
        "SetFormation",
        "DeployZone",
        "Ready",
    ];
    let seen: BTreeSet<&str> = goldens()
        .iter()
        .flat_map(|(_, replay)| replay.turns.iter().flat_map(|(_, c)| c))
        .map(|c| kind(&c.command))
        .collect();
    let missing: Vec<&str> = KINDS.into_iter().filter(|k| !seen.contains(k)).collect();
    assert!(missing.is_empty(), "no golden gives {missing:?}");
}

/// Each golden's checkpoint hashes, folded into one number.
fn fingerprint(replay: &Replay) -> u64 {
    replay
        .checkpoints
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |h, &(tick, hash)| {
            (h ^ hash ^ u64::from(tick)).wrapping_mul(0x0100_0000_01b3)
        })
}

/// How the goldens played under [`SIM_VERSION`] (and were blessed with it).
/// A change that makes them play differently must bump `SIM_VERSION`, so
/// that other builds and old replays know: this table then fails, and is
/// updated from the test's output.
const PINNED: (u32, &[(&str, u64)]) = (
    2,
    &[
        ("field-1v1", 0x0997e778566b4664),
        ("field-2v2", 0x5b256c6d516c2d32),
        ("field-8000pt-2v2", 0x7a96510bf26a5a7d),
        ("field-champions-1v1", 0x9306ab052460efc1),
        ("field-deploy-1v1", 0x4b8c1d8dd6b680f9),
        ("field-woods-1v1", 0xa96747e3b4f2652f),
        ("sandbox-tuning", 0xc36a666ef56ef61d),
    ],
);

#[test]
fn the_sim_version_changes_when_the_goldens_do() {
    let goldens = goldens();
    let table: Vec<String> = goldens
        .iter()
        .map(|(file, replay)| {
            let name = file.file_stem().unwrap().to_string_lossy();
            format!("        (\"{name}\", {:#018x}),", fingerprint(replay))
        })
        .collect();
    let table = table.join("\n");
    assert_eq!(
        PINNED.0, SIM_VERSION,
        "SIM_VERSION changed: bless the goldens, then set PINNED to \
         ({SIM_VERSION}, &[\n{table}\n])"
    );
    for (file, replay) in &goldens {
        let name = file.file_stem().unwrap().to_string_lossy();
        assert_eq!(
            replay.header.sim_version,
            Some(SIM_VERSION),
            "{name} wasn't blessed under this sim version: \
             cargo run --release -p gc_replay -- bless replays"
        );
        let pinned = PINNED.1.iter().find(|(n, _)| *n == name).map(|&(_, f)| f);
        assert_eq!(
            pinned,
            Some(fingerprint(replay)),
            "{name} plays differently from how it did under sim version {SIM_VERSION} \
             (or is new). If the sim changed, bump gc_sim::SIM_VERSION; then set PINNED to \
             ({SIM_VERSION} or the new version, &[\n{table}\n])"
        );
    }
}
