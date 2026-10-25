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
        Command::Trade { .. } => "Trade",
        Command::Build { .. } => "Build",
        Command::Clear { .. } => "Clear",
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
        Command::FocusTower { .. } => "FocusTower",
        Command::SetGate { .. } => "SetGate",
        Command::BuildWall { .. } => "BuildWall",
        Command::PlanWall { .. } => "PlanWall",
        Command::DropPlan { .. } => "DropPlan",
        Command::WorkPlan { .. } => "WorkPlan",
        Command::RaiseWall { .. } => "RaiseWall",
        Command::Torches { .. } => "Torches",
        Command::ManWall { .. } => "ManWall",
        Command::Climb { .. } => "Climb",
        Command::Burn { .. } => "Burn",
    };
    const KINDS: [&str; 36] = [
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
        "Trade",
        "Build",
        "Clear",
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
        "FocusTower",
        "SetGate",
        "BuildWall",
        "PlanWall",
        "DropPlan",
        "WorkPlan",
        "RaiseWall",
        "Torches",
        "ManWall",
        "Climb",
        "Burn",
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
    49,
    &[
        ("conquest-ribcage-1v1", 0x6a621bc17b635172),
        ("field-1v1", 0x5ea913e443ae5fb3),
        ("field-2v2", 0x9cd2af7767a156f9),
        ("field-8000pt-2v2", 0x0ae5d8a2a0573f26),
        ("field-brace-1v1", 0xa79e20cca89f9a87),
        ("field-champions-1v1", 0x06f4d00e0560f3b5),
        ("field-deploy-1v1", 0xf31919a6b787c377),
        ("field-fire-1v1", 0xd9f5b5a8b5e58aa3),
        ("field-flood-1v1", 0x26b8d223943b2306),
        ("field-handover-2v2", 0xe528d8aa7e62195f),
        ("field-night-rain-1v1", 0xc08f79658c8ab7b2),
        ("field-woods-1v1", 0xd25bf952ed4a8e8d),
        ("sandbox-tuning", 0xa9fd7f77a1f58e2d),
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
