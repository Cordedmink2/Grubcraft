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
        Command::SpawnStructure { .. } => "SpawnStructure",
        Command::AttackStructure { .. } => "AttackStructure",
        Command::RemoveStructure { .. } => "RemoveStructure",
        Command::Gather { .. } => "Gather",
        Command::Build { .. } => "Build",
        Command::Construct { .. } => "Construct",
        Command::CallOff { .. } => "CallOff",
        Command::Train { .. } => "Train",
        Command::Untrain { .. } => "Untrain",
        Command::Upgrade { .. } => "Upgrade",
        Command::Rally { .. } => "Rally",
        Command::FallBack { .. } => "FallBack",
        Command::Brace { .. } => "Brace",
        Command::HandOver { .. } => "HandOver",
        Command::UsePower { .. } => "UsePower",
    };
    const KINDS: [&str; 23] = [
        "SpawnRegiment",
        "MoveTo",
        "Attack",
        "RemoveRegiment",
        "TuneUnit",
        "SetFormation",
        "DeployZone",
        "Ready",
        "SpawnStructure",
        "AttackStructure",
        "RemoveStructure",
        "Gather",
        "Build",
        "Construct",
        "CallOff",
        "Train",
        "Untrain",
        "Upgrade",
        "Rally",
        "FallBack",
        "Brace",
        "HandOver",
        "UsePower",
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
    18,
    &[
        ("conquest-ribcage-1v1", 0xa03fe4505de6f7d2),
        ("field-1v1", 0xe415d2ec04e85ec8),
        ("field-2v2", 0xa7366805a01e729b),
        ("field-8000pt-2v2", 0xe5f173cfc7cca9a6),
        ("field-brace-1v1", 0x816359d5892b65cb),
        ("field-champions-1v1", 0xdf89ba85028628f0),
        ("field-deploy-1v1", 0x32740ae283269f57),
        ("field-handover-2v2", 0xffe57b2092abf72b),
        ("field-woods-1v1", 0xb4153f2e06ab5256),
        ("sandbox-tuning", 0x24db551efd44b1d5),
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
