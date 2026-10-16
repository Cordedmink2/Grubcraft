//! Every battle saved as a replay, and the list the Replays screen shows
//! (docs/11-networking.md, "Save, load and replays").
//!
//! A battle is recorded from its first tick with `gc_replay::Recorder` and
//! saved to a `replays` folder beside `settings.ron`: once a minute while it
//! lasts, when it is decided, and when you leave it. The newest
//! [`KEEP`] are kept. A desync report is the same file, saved to
//! `desync-reports` with the tick the worlds parted at.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use gc_platform::logln;
use gc_replay::{EXTENSION, Header, Player, Replay};
use gc_sim::{Capacities, TICKS_PER_SECOND};
use gc_ui::ReplayEntry;

use crate::battle::Party;

/// Saved replays kept in the folder; older ones are deleted.
pub const KEEP: usize = 50;
/// A battle's recording is saved this often while it is played.
pub const SAVE_EVERY: u32 = 60 * TICKS_PER_SECOND;

/// `name` beside the settings file, or in the working folder.
fn beside_settings(name: &str) -> PathBuf {
    gc_ui::settings::app_dir().map_or_else(|| PathBuf::from(name), |d| d.join(name))
}

pub fn folder() -> PathBuf {
    beside_settings("replays")
}

pub fn desync_folder() -> PathBuf {
    beside_settings("desync-reports")
}

/// Seconds since 1970 now.
pub fn now() -> u64 {
    gc_platform::diag::unix_now()
}

/// `secs` since 1970 as "2026-10-15 21:50" (UTC).
pub fn date_time(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's days-to-civil.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// Where to save a new recording made at `secs`: the date and time come
/// first, so the newest sorts last.
/// Never one already there: a second recording in the same second gets a
/// number.
pub fn new_path(dir: &Path, secs: u64, what: &str) -> PathBuf {
    let stamp = format!(
        "{}-{:02} {what}",
        date_time(secs).replace(':', "-"),
        secs % 60
    );
    (1..)
        .map(|n| match n {
            1 => dir.join(format!("{stamp}.{EXTENSION}")),
            n => dir.join(format!("{stamp} {n}.{EXTENSION}")),
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Save `replay` to `path`, making its folder. A crash while saving leaves
/// the earlier copy, not a torn file.
pub fn save(replay: &Replay, path: &Path) -> Result<(), String> {
    gc_platform::fsutil::write_atomic(path, &replay.encode()).map_err(|e| e.to_string())
}

/// The replay files in `dir`, oldest first (none if there is no folder).
fn files(dir: &Path) -> Vec<PathBuf> {
    gc_replay::files_in(dir).unwrap_or_default()
}

/// Delete all but the newest `keep` replays in `dir`.
pub fn prune(dir: &Path, keep: usize) {
    let found = files(dir);
    for old in &found[..found.len().saturating_sub(keep)] {
        if let Err(e) = std::fs::remove_file(old) {
            logln!("grubcraft: could not delete {}: {e}", old.display());
        }
    }
}

/// The sides of a match as a replay header names them.
pub fn players(parties: &[Party]) -> Vec<Player> {
    parties
        .iter()
        .map(|p| Player {
            name: p.name.clone(),
            color: p.color,
            faction: p.faction.map(gc_ai::Faction::index),
        })
        .collect()
}

/// The sides a replay recorded, or none for one recorded without them.
pub fn parties(header: &Header) -> Vec<Party> {
    header
        .players
        .iter()
        .map(|p| Party {
            name: p.name.clone(),
            color: p.color,
            faction: p.faction.map(gc_ai::Faction::from_index),
        })
        .collect()
}

/// "Red (Mario) v Blue (AI 2)", sides on one team joined by "and".
pub fn title(replay: &Replay) -> String {
    let names = &replay.header.players;
    if names.is_empty() {
        return replay.note.clone();
    }
    let mut teams: Vec<u8> = replay.teams.clone();
    teams.sort_unstable();
    teams.dedup();
    let sides: Vec<String> = teams
        .iter()
        .map(|&t| {
            let on: Vec<&str> = (replay.teams.iter().zip(names))
                .filter(|&(&team, _)| team == t)
                .map(|(_, p)| p.name.as_str())
                .collect();
            on.join(" and ")
        })
        .collect();
    sides.join(" v ")
}

/// Why this build can't show `replay`, if it can't: unit ids name entries
/// in the game's unit table, so a table of another size names other
/// regiments, and the game draws no more regiments than its capacity.
pub fn unplayable(replay: &Replay, unit_types: usize, capacity: Capacities) -> Option<String> {
    if replay.units.len() != unit_types {
        Some("Made with other unit data: this build can't show it".into())
    } else if replay.regiments > capacity.regiments {
        Some("Too many regiments for this build to show".into())
    } else {
        None
    }
}

/// The saved replays in `dir`, newest first, as the Replays screen lists
/// them. One this build can't show (another format, or a unit table of a
/// different size, so its unit ids name other regiments) says why.
pub fn list(
    dir: &Path,
    unit_types: usize,
    capacity: Capacities,
    data_hash: u64,
) -> Vec<ReplayEntry> {
    files(dir)
        .into_iter()
        .rev()
        .map(|path| {
            let saved = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(String::new(), |d| {
                    format!("{} UTC, ", date_time(d.as_secs()))
                });
            match Replay::load(&path) {
                Ok(r) => {
                    let mut detail = format!("{saved}{} long", gc_ai::lens::clock(r.ticks));
                    if r.header.data_hash != 0 && r.header.data_hash != data_hash {
                        detail.push_str(", older game data");
                    }
                    if r.header.sim_version != Some(gc_sim::SIM_VERSION) {
                        detail.push_str(", other rules: may play differently");
                    }
                    ReplayEntry {
                        title: title(&r),
                        detail,
                        unplayable: unplayable(&r, unit_types, capacity),
                        path,
                    }
                }
                Err(e) => ReplayEntry {
                    title: path
                        .file_stem()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                    detail: String::new(),
                    unplayable: Some(e.to_string()),
                    path,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_as_people_write_them() {
        assert_eq!(date_time(0), "1970-01-01 00:00");
        // 2026-10-15 21:50:07 UTC.
        assert_eq!(date_time(1_792_101_007), "2026-10-15 21:50");
        assert_eq!(date_time(951_782_400), "2000-02-29 00:00");
        let p = new_path(Path::new("r"), 1_792_101_007, "Field Battle");
        assert_eq!(p, Path::new("r/2026-10-15 21-50-07 Field Battle.gcr"));
    }

    #[test]
    fn only_the_newest_are_kept_and_listed_first() {
        let dir = std::env::temp_dir().join(format!("grubcraft-replays-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let units = gc_sim::scenario::DRILL_UNITS;
        let setup = gc_sim::MatchSetup {
            seed: 1,
            capacities: gc_sim::Capacities { regiments: 8 },
            units: &units,
            map_tiles: 32,
            nav: &[],
            teams: &[0, 1],
            ground: gc_sim::Ground::FLAT,
            champions: false,
        };
        let world = gc_sim::World::new(&setup);
        let mut rec = gc_replay::Recorder::new(&setup, &world, "test");
        rec.header_mut().players = players(&[
            Party {
                name: "Red (Mario)".into(),
                color: [1, 2, 3],
                faction: Some(gc_ai::Faction::from_index(1)),
            },
            Party {
                name: "Blue (AI 2)".into(),
                color: [4, 5, 6],
                faction: None,
            },
        ]);
        let replay = rec.finish();
        for s in 0..5 {
            save(&replay, &new_path(&dir, 1_792_101_000 + s, "Field Battle")).unwrap();
        }
        // A second recording in the same second gets its own name.
        let again = new_path(&dir, 1_792_101_004, "Field Battle");
        assert!(again.ends_with("2026-10-15 21-50-04 Field Battle 2.gcr"));
        std::fs::write(dir.join("junk.gcr"), b"not a replay").unwrap();
        prune(&dir, 3);
        let listed = list(&dir, units.len(), Capacities::default(), 0);
        assert_eq!(listed.len(), 3, "{listed:?}");
        // "junk" sorts after the dates, so it is the newest.
        assert!(listed[0].unplayable.is_some());
        assert_eq!(listed[1].title, "Red (Mario) v Blue (AI 2)");
        assert!(
            listed[1]
                .path
                .ends_with("2026-10-15 21-50-04 Field Battle.gcr")
        );
        assert!(listed[1].unplayable.is_none());
        assert_eq!(
            list(&dir, units.len() + 1, Capacities::default(), 0)[1]
                .unplayable
                .as_deref()
                .map(|s| &s[..10]),
            Some("Made with ")
        );
        let back = parties(&Replay::load(&listed[1].path).unwrap().header);
        assert_eq!(back[0].faction, Some(gc_ai::Faction::from_index(1)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
